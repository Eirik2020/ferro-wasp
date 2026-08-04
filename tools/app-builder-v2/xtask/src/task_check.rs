//! Host-only harness generation for type-checking reusable task bodies.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};

use crate::{
    app, board,
    board::MonotonicDeclaration,
    generator, resolve,
    task::{
        TaskDeclaration, TaskDefinition, TaskResourceCapability, TaskResourceDefinition,
        read_body_source,
    },
};

/// Validates the selected declarations and writes a host task-check harness.
pub(crate) fn write(repository_root: &Path, destination: &Path) -> Result<()> {
    let selected_board = generator::selected_board();
    let selected_app = generator::selected_app();
    app::validate(selected_app)?;
    board::validate(selected_board)?;
    resolve::resolve(selected_board, selected_app)?;

    let monotonic_id = match selected_board.monotonic {
        MonotonicDeclaration::SysTick { id, .. } => id,
    };
    let mut definitions = BTreeMap::new();
    for task in selected_app.tasks {
        definitions.insert(task.definition.id, task.definition);
    }

    let mut rendered = String::from("// Generated host task-body checks.\n");
    for definition in definitions.values() {
        read_body_source(repository_root, definition)?;
        rendered.push_str(&render_definition(
            repository_root,
            definition,
            selected_app.tasks,
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
    tasks: &[TaskDeclaration],
    monotonic_id: &str,
) -> String {
    let module_id = format!("__check_{}", definition.id);
    let mut namespaces = BTreeMap::<&str, Namespace<'_>>::new();
    namespaces.entry(definition.id).or_default().context = Some(definition);
    for task in tasks {
        if matches!(task.trigger, crate::task::TaskTrigger::Spawned) {
            namespaces.entry(task.id).or_default().spawn = Some(task);
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
    use sbus_rs::StreamingParser;

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
    spawn: Option<&'a TaskDeclaration>,
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
    let local_fields = render_context_fields(definition.local_resources, false);
    let shared_fields = render_context_fields(definition.shared_resources, true);
    format!(
        "pub struct Context {{\n    pub local: LocalResources,\n    pub shared: SharedResources,\n}}\n\npub struct LocalResources {{{local_fields}\n}}\n\npub struct SharedResources {{{shared_fields}\n}}"
    )
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
        TaskResourceCapability::Bool => "bool",
        TaskResourceCapability::UartRxDma => "crate::support::UartRxDma",
    }
}

fn render_spawn(task: &TaskDeclaration) -> String {
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
    use crate::task::{TaskArgument, boolean, digital_output};

    #[test]
    fn renders_logical_context_and_spawn_signatures() {
        const DEFINITION: TaskDefinition = TaskDefinition::asynchronous("blink")
            .with_local(&[digital_output("led")])
            .with_shared(&[boolean("enabled")]);
        const REPORT: TaskDefinition =
            TaskDefinition::asynchronous("report").with_args(&[TaskArgument::new("count", "u32")]);
        const REPORT_TASK: TaskDeclaration = REPORT.spawned_as("report_blink").priority(1);
        let rendered = render_definition(
            Path::new("/workspace/builder"),
            &DEFINITION,
            &[REPORT_TASK],
            "Mono",
        );

        assert!(rendered.contains("pub led: &'static mut crate::support::DigitalOutput"));
        assert!(rendered.contains("Shared<'static, bool>"));
        assert!(rendered.contains("pub fn spawn(count: u32)"));
        assert!(rendered.contains("DurationExt as _, Monotonic as Mono"));
        assert!(rendered.contains("UartRxIrqOutcome"));
        assert!(rendered.contains("include!(\"/workspace/builder/tasks/blink.rs\")"));
    }
}
