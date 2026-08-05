//! Host-only harness generation for type-checking reusable task bodies.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::{
    app, board,
    board::MonotonicDeclaration,
    component, generator, resolve,
    task::{
        SOFTWARE_BOOL, SOFTWARE_LINE_CONSUMER, SOFTWARE_MOTOR_CMD, SOFTWARE_RC_INPUT_SNAPSHOT,
        SOFTWARE_SBUS_CONSUMER, SOFTWARE_SERIAL_RX, TaskDefinition, TaskParameterDefinition,
        TaskParameterKind, TaskResourceCapability, TaskResourceDefinition, read_body_source,
    },
};

/// Validates all target declarations and writes a host task-check harness.
pub(crate) fn write(repository_root: &Path, destination: &Path) -> Result<()> {
    let mut expanded_apps = Vec::new();
    let mut monotonic_id = None;
    for (target_board, target_app) in generator::target_declarations() {
        app::validate(target_app)?;
        board::validate(target_board)?;
        let expanded = component::expand(target_board, target_app)?;
        resolve::resolve(target_board, &expanded)?;
        let target_monotonic_id = match target_board.monotonic {
            MonotonicDeclaration::SysTick { id, .. } => id,
        };
        if monotonic_id.is_some_and(|existing| existing != target_monotonic_id) {
            bail!("task-check targets must use one shared monotonic identifier");
        }
        monotonic_id = Some(target_monotonic_id);
        expanded_apps.push(expanded);
    }
    let monotonic_id = monotonic_id.context("task-check requires at least one target")?;
    let expanded_tasks = expanded_apps
        .iter()
        .flat_map(|expanded| expanded.tasks.iter().cloned())
        .collect::<Vec<_>>();
    let mut definitions = BTreeMap::new();
    for task in &expanded_tasks {
        if let Some(existing) = definitions.insert(task.definition.id, task.definition)
            && existing != task.definition
        {
            bail!(
                "task-check targets define conflicting `{}` task contracts",
                task.definition.id
            );
        }
    }

    let mut rendered = String::from("// Generated host task-body checks.\n");
    for definition in definitions.values() {
        read_body_source(repository_root, definition)?;
        rendered.push_str(&render_definition(
            repository_root,
            definition,
            &expanded_tasks,
            monotonic_id,
        ));
        rendered.push('\n');
    }
    syn::parse_file(&rendered).context("parse generated host task-check harness")?;
    fs::write(destination, rendered)
        .with_context(|| format!("write task-check harness {}", destination.display()))?;
    Ok(())
}

fn render_definition(
    repository_root: &Path,
    definition: &TaskDefinition,
    tasks: &[component::ExpandedTask],
    monotonic_id: &str,
) -> String {
    let module_id = format!("__check_{}", definition.id);
    let mut namespaces = BTreeMap::<&str, Namespace<'_>>::new();
    namespaces.entry(definition.id).or_default().context = Some(definition);
    for task in tasks {
        if matches!(task.trigger, component::ExpandedTaskTrigger::Spawned) {
            namespaces.entry(task.id.as_str()).or_default().spawn = Some(task);
        }
    }
    let namespaces = namespaces
        .into_iter()
        .map(|(id, namespace)| render_namespace(id, namespace))
        .collect::<Vec<_>>()
        .join("\n");
    let body_path = repository_root
        .join("tasks")
        .join(format!("{}.rs", definition.id));
    let body_path = format!("{:?}", body_path.to_string_lossy());

    format!(
        r#"#[allow(dead_code, unused_imports)]
mod {module_id} {{
    use crate::support::{{DurationExt as _, Monotonic as {monotonic_id}, UartRxIrqOutcome, UartRxReadOutcome, UART_RX_BUFFER_SIZE}};
    use embedded_hal::digital::{{OutputPin, StatefulOutputPin}};
    use ferrowasp_drivers::serial_consumer::{{LineConsumer, LineConsumerEvent, SbusConsumer}};

{namespaces}

    include!({body_path});
}}
"#,
        namespaces = indent(&namespaces, 4),
    )
}

#[derive(Default)]
struct Namespace<'a> {
    context: Option<&'a TaskDefinition>,
    spawn: Option<&'a component::ExpandedTask>,
}

fn render_namespace(id: &str, namespace: Namespace<'_>) -> String {
    let context = namespace.context.map(render_context).unwrap_or_default();
    let spawn = namespace.spawn.map(render_spawn).unwrap_or_default();
    let contents = [context, spawn]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("mod {id} {{\n{}\n}}", indent(&contents, 4))
}

fn render_context(definition: &TaskDefinition) -> String {
    let parameter_fields = render_parameter_fields(definition.parameters);
    let local_fields = render_context_fields(definition.local_resources, false);
    let shared_fields = render_context_fields(definition.shared_resources, true);
    format!(
        "pub struct Context {{\n    pub config: Config,\n    pub local: LocalResources,\n    pub shared: SharedResources,\n}}\n\npub struct Config {{{parameter_fields}\n}}\n\npub struct LocalResources {{{local_fields}\n}}\n\npub struct SharedResources {{{shared_fields}\n}}"
    )
}

fn render_parameter_fields(parameters: &[TaskParameterDefinition]) -> String {
    parameters
        .iter()
        .map(|parameter| {
            let parameter_type = match parameter.kind() {
                TaskParameterKind::Duration => "crate::support::Milliseconds",
            };
            format!("\n    pub {}: {parameter_type},", parameter.id())
        })
        .collect()
}

fn render_context_fields(resources: &[TaskResourceDefinition], shared: bool) -> String {
    resources
        .iter()
        .map(|resource| {
            let resource_type = capability_type(resource.capability());
            let field_type = if shared {
                format!("crate::support::Shared<'static, {resource_type}>")
            } else {
                format!("&'static mut {resource_type}")
            };
            format!("\n    pub {}: {field_type},", resource.id())
        })
        .collect()
}

fn capability_type(capability: TaskResourceCapability) -> &'static str {
    match capability {
        TaskResourceCapability::DigitalOutput => "crate::support::DigitalOutput",
        TaskResourceCapability::InterruptInput => "crate::support::InterruptInput",
        TaskResourceCapability::UartRxDma => "crate::support::UartRxDma",
        TaskResourceCapability::Software(SOFTWARE_BOOL) => "bool",
        TaskResourceCapability::Software(SOFTWARE_SERIAL_RX) => "crate::support::SerialRx",
        TaskResourceCapability::Software(SOFTWARE_RC_INPUT_SNAPSHOT) => {
            "ferrowasp_io_core::serial::RcInputSnapshot"
        }
        TaskResourceCapability::Software(SOFTWARE_SBUS_CONSUMER) => {
            "ferrowasp_drivers::serial_consumer::SbusConsumer"
        }
        TaskResourceCapability::Software(SOFTWARE_LINE_CONSUMER) => {
            "ferrowasp_drivers::serial_consumer::LineConsumer"
        }
        TaskResourceCapability::Software(type_id) => {
            panic!("task checker has no Rust type for software resource `{type_id}`")
        }
        TaskResourceCapability::ObserverPublisher(SOFTWARE_RC_INPUT_SNAPSHOT) => {
            "ferrowasp_core::observer_channel::ObserverPublisher<'static, ferrowasp_io_core::serial::RcInputSnapshot>"
        }
        TaskResourceCapability::ObserverReader(SOFTWARE_RC_INPUT_SNAPSHOT) => {
            "ferrowasp_core::observer_channel::ObserverReader<'static, ferrowasp_io_core::serial::RcInputSnapshot>"
        }
        TaskResourceCapability::ObserverPublisher(type_id) => {
            panic!("task checker has no publisher type for observer `{type_id}`")
        }
        TaskResourceCapability::ObserverReader(type_id) => {
            panic!("task checker has no reader type for observer `{type_id}`")
        }
        TaskResourceCapability::SafetyProducer(SOFTWARE_MOTOR_CMD) => {
            "ferrowasp_core::safety_channel::SafetyProducer<'static, ferrowasp_core::safety::MotorCmd>"
        }
        TaskResourceCapability::SafetyConsumer(SOFTWARE_MOTOR_CMD) => {
            "ferrowasp_core::safety_channel::SafetyConsumer<'static, ferrowasp_core::safety::MotorCmd>"
        }
        TaskResourceCapability::SafetyProducer(type_id) => {
            panic!("task checker has no producer type for safety channel `{type_id}`")
        }
        TaskResourceCapability::SafetyConsumer(type_id) => {
            panic!("task checker has no consumer type for safety channel `{type_id}`")
        }
    }
}

fn render_spawn(task: &component::ExpandedTask) -> String {
    let arguments = task
        .definition
        .args
        .iter()
        .map(|argument| format!("{}: {}", argument.name, argument.rust_type))
        .collect::<Vec<_>>()
        .join(", ");
    let unused = task
        .definition
        .args
        .iter()
        .map(|argument| argument.name)
        .collect::<Vec<_>>()
        .join(", ");
    let ignore_arguments = match task.definition.args.len() {
        0 => String::new(),
        1 => format!("\n    let _ = {unused};"),
        _ => format!("\n    let _ = ({unused});"),
    };
    format!(
        "pub fn spawn({arguments}) -> Result<(), crate::support::SpawnError> {{{ignore_arguments}\n    Ok(())\n}}"
    )
}

fn indent(source: &str, spaces: usize) -> String {
    let padding = " ".repeat(spaces);
    source
        .lines()
        .map(|line| format!("{padding}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        component::{ExpandedTask, ExpandedTaskTrigger},
        task::{
            SOFTWARE_MOTOR_CMD, TaskArgument, TaskResourceCapability, TaskSafetyClass, boolean,
            digital_output, duration,
        },
    };

    #[test]
    fn renders_logical_context_and_spawn_signatures() {
        const DEFINITION: TaskDefinition = TaskDefinition::asynchronous("blink")
            .with_parameters(&[duration("interval")])
            .with_local(&[digital_output("led")])
            .with_shared(&[boolean("enabled")]);
        const REPORT: TaskDefinition =
            TaskDefinition::asynchronous("report").with_args(&[TaskArgument::new("count", "u32")]);
        let report_task = ExpandedTask {
            id: "report_blink".to_owned(),
            definition: REPORT,
            safety_class: TaskSafetyClass::NonSafetyCritical,
            priority: 1,
            trigger: ExpandedTaskTrigger::Spawned,
            parameters: vec![],
            local_resources: vec![],
            shared_resources: vec![],
            owner_component: None,
        };
        let rendered = render_definition(
            Path::new("/workspace/builder"),
            &DEFINITION,
            &[report_task],
            "Mono",
        );

        assert!(rendered.contains("pub led: &'static mut crate::support::DigitalOutput"));
        assert!(rendered.contains("pub interval: crate::support::Milliseconds"));
        assert!(rendered.contains("Shared<'static, bool>"));
        assert!(rendered.contains("pub fn spawn(count: u32)"));
        assert!(rendered.contains("DurationExt as _, Monotonic as Mono"));
        assert!(rendered.contains("UartRxIrqOutcome"));
        assert!(rendered.contains("include!(\"/workspace/builder/tasks/blink.rs\")"));

        assert_eq!(
            capability_type(TaskResourceCapability::SafetyProducer(SOFTWARE_MOTOR_CMD)),
            "ferrowasp_core::safety_channel::SafetyProducer<'static, ferrowasp_core::safety::MotorCmd>"
        );
        assert_eq!(
            capability_type(TaskResourceCapability::SafetyConsumer(SOFTWARE_MOTOR_CMD)),
            "ferrowasp_core::safety_channel::SafetyConsumer<'static, ferrowasp_core::safety::MotorCmd>"
        );
    }
}
