//! End-to-end orchestration and template assembly for the selected target.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::{
    app, backend, board, component,
    resolve::{self, ResolvedTask},
    task::{read_body_source, render_expanded},
};

#[path = "../../targets/ferrowasp_fcu3/src/board.rs"]
mod ferrowasp_fcu3;
#[path = "../../targets/ferrowasp_fcu3/src/app_composition.rs"]
mod ferrowasp_fcu3_app;
#[path = "../../targets/nucleo_f401re/src/board.rs"]
mod nucleo_f401re;
#[path = "../../targets/nucleo_f401re/src/app_composition.rs"]
mod nucleo_f401re_app;

const APP_TEMPLATE: &str = include_str!("../../templates/app.rs.tpl");
const PRELUDE_TEMPLATE: &str = include_str!("../../templates/prelude.rs.tpl");

struct GenerationTarget {
    id: &'static str,
    board: &'static board::BoardDeclaration,
    app: &'static app::AppDeclaration,
    generated_app_output: &'static str,
    prelude_output: &'static str,
}

struct RenderedTarget {
    id: &'static str,
    generated_app_output: &'static str,
    prelude_output: &'static str,
    application: String,
    prelude: String,
}

type TargetDeclaration = (
    &'static board::BoardDeclaration,
    &'static app::AppDeclaration,
);

fn generation_targets() -> [GenerationTarget; 2] {
    [
        GenerationTarget {
            id: "nucleo_f401re",
            board: &nucleo_f401re::BOARD,
            app: &nucleo_f401re_app::APP,
            generated_app_output: "targets/nucleo_f401re/src/generated_app.rs",
            prelude_output: "targets/nucleo_f401re/src/prelude.rs",
        },
        GenerationTarget {
            id: "ferrowasp_fcu3",
            board: &ferrowasp_fcu3::BOARD,
            app: &ferrowasp_fcu3_app::APP,
            generated_app_output: "targets/ferrowasp_fcu3/src/generated_app.rs",
            prelude_output: "targets/ferrowasp_fcu3/src/prelude.rs",
        },
    ]
}

pub(crate) fn target_declarations() -> [TargetDeclaration; 2] {
    generation_targets().map(|target| (target.board, target.app))
}

/// Validates declarations and writes every supported generated RTIC target.
pub fn generate(repository_root: &Path) -> Result<()> {
    let rendered = generation_targets()
        .iter()
        .map(|target| render_target(repository_root, target))
        .collect::<Result<Vec<_>>>()?;

    for target in rendered {
        write_target(repository_root, &target)?;
    }
    Ok(())
}

fn render_target(repository_root: &Path, target: &GenerationTarget) -> Result<RenderedTarget> {
    app::validate(target.app).with_context(|| format!("validate app for `{}`", target.id))?;
    board::validate(target.board).with_context(|| format!("validate board `{}`", target.id))?;
    let validated_board = backend::validate(target.board)
        .with_context(|| format!("validate backend for `{}`", target.id))?;
    let expanded = component::expand(target.board, target.app)
        .with_context(|| format!("expand components for `{}`", target.id))?;
    let resolved = resolve::resolve(target.board, &expanded)
        .with_context(|| format!("resolve application for `{}`", target.id))?;
    let board = backend::render(&validated_board, &resolved)?;
    let tasks = render_tasks(repository_root, &resolved.tasks, &board)?;
    let init_spawns = render_init_spawns(&resolved.init_spawned_tasks);
    let application = substitute(APP_TEMPLATE, &tasks, &init_spawns, &board)?;
    let prelude = render_prelude(PRELUDE_TEMPLATE, &board.prelude_exports)?;
    syn::parse_file(&application).context("parse complete generated RTIC application")?;
    syn::parse_file(&prelude).context("parse generated RTIC prelude")?;

    Ok(RenderedTarget {
        id: target.id,
        generated_app_output: target.generated_app_output,
        prelude_output: target.prelude_output,
        application,
        prelude,
    })
}

fn write_target(repository_root: &Path, target: &RenderedTarget) -> Result<()> {
    let app_destination = repository_root.join(target.generated_app_output);
    let parent = app_destination
        .parent()
        .expect("generated app path has a parent directory");
    fs::create_dir_all(parent)
        .with_context(|| format!("create generated source directory {}", parent.display()))?;
    fs::write(&app_destination, &target.application)
        .with_context(|| format!("write generated RTIC app {}", app_destination.display()))?;
    let prelude_destination = repository_root.join(target.prelude_output);
    fs::write(&prelude_destination, &target.prelude).with_context(|| {
        format!(
            "write generated RTIC prelude {}",
            prelude_destination.display()
        )
    })?;
    println!(
        "Generated RTIC app for `{}`: {}",
        target.id,
        app_destination.display()
    );
    println!(
        "Generated RTIC prelude for `{}`: {}",
        target.id,
        prelude_destination.display()
    );
    Ok(())
}

fn render_tasks(
    repository_root: &Path,
    tasks: &[ResolvedTask<'_>],
    board: &backend::RenderedBoardInit,
) -> Result<String> {
    let rendered = tasks
        .iter()
        .map(|task| {
            let resolved_locals = task
                .local_resources
                .iter()
                .map(|resource| (resource.task_resource(), resource.id()))
                .collect::<Vec<_>>();
            let resolved_shared = task
                .shared_resources
                .iter()
                .map(|resource| (resource.task_resource(), resource.id()))
                .collect::<Vec<_>>();
            let declared_locals = task
                .declaration
                .local_resources
                .iter()
                .map(|resource| (resource.task_resource.as_str(), resource.target.id()))
                .collect::<Vec<_>>();
            let declared_shared = task
                .declaration
                .shared_resources
                .iter()
                .map(|resource| (resource.task_resource.as_str(), resource.target.id()))
                .collect::<Vec<_>>();
            if resolved_locals != declared_locals || resolved_shared != declared_shared {
                bail!(
                    "resolved resources for task `{}` do not match its declaration",
                    task.declaration.id
                );
            }
            let interrupt_binding = board
                .interrupt_bindings
                .get(&task.declaration.id)
                .map(String::as_str);
            let rendered = render_expanded(
                task.declaration,
                &read_body_source(repository_root, &task.declaration.definition)?,
                interrupt_binding,
            )?;
            Ok((task.declaration.owner_component.as_deref(), rendered))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut sections = rendered
        .iter()
        .filter(|(owner, _)| owner.is_none())
        .map(|(_, task)| task.clone())
        .collect::<Vec<_>>();
    let mut component_order = Vec::new();
    for (owner, _) in &rendered {
        if let Some(owner) = owner
            && !component_order.contains(owner)
        {
            component_order.push(*owner);
        }
    }
    for owner in component_order {
        let component_tasks = rendered
            .iter()
            .filter(|(candidate, _)| *candidate == Some(owner))
            .map(|(_, task)| task.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let start = crate::component_divider(owner, "tasks", false);
        let end = crate::component_divider(owner, "tasks", true);
        sections.push(format!("{start}\n{component_tasks}\n{end}"));
    }
    Ok(sections.join("\n\n"))
}

fn render_init_spawns(tasks: &[&crate::component::ExpandedTask]) -> String {
    if tasks.is_empty() {
        return String::new();
    }
    let spawns = tasks
        .iter()
        .map(|task| {
            let ownership = match task.owner_component.as_deref() {
                Some(owner) => format!("// Component `{owner}` task `{}`", task.id),
                None => format!("// Task `{}`", task.id),
            };
            format!("{ownership}\n{}", render_spawn(task))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let header = crate::scope_divider("Initial task startup", false, 88, '=');
    format!("{header}\n\n{spawns}")
}

fn render_spawn(task: &crate::component::ExpandedTask) -> String {
    format!(
        "{}::spawn().expect(\"init must spawn declared task {}\");",
        task.id, task.id
    )
}

fn substitute(
    template: &str,
    tasks: &str,
    init_spawns: &str,
    board: &crate::backend::RenderedBoardInit,
) -> Result<String> {
    let replacements = [
        ("{{DISPATCHERS}}", board.dispatchers.clone(), 0),
        (
            "{{PRELUDE_IMPORT}}",
            if board.prelude_exports.is_empty() {
                String::new()
            } else {
                "use crate::prelude::*;".to_owned()
            },
            4,
        ),
        (
            "{{TIMING_DECLARATIONS}}",
            board.timing_declarations.clone(),
            4,
        ),
        ("{{INIT_ATTRIBUTE}}", board.init_attribute.clone(), 4),
        ("{{SHARED_STRUCT}}", board.shared_struct.clone(), 4),
        ("{{SHARED_VALUE}}", board.shared_value.clone(), 8),
        ("{{LOCAL_STRUCT}}", board.local_struct.clone(), 4),
        ("{{LOCAL_VALUE}}", board.local_value.clone(), 8),
        ("{{BOARD_INIT}}", board.initialization.clone(), 8),
        ("{{INIT_SPAWNS}}", init_spawns.to_owned(), 8),
        ("{{TASKS}}", tasks.to_owned(), 4),
    ];
    let mut rendered = template.to_owned();
    for (marker, value, indentation) in replacements {
        if rendered.matches(marker).count() != 1 {
            bail!("RTIC app template must contain exactly one {marker} marker");
        }
        rendered = rendered.replace(marker, &indent(&value, indentation));
    }
    if board.initialization.is_empty() && init_spawns.trim().is_empty() {
        rendered = rendered.replace(
            "fn init(cx: init::Context) -> (Shared, Local) {\n        \n",
            "fn init(_cx: init::Context) -> (Shared, Local) {\n",
        );
    }
    let mut normalized = rendered
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    while normalized.contains("\n\n\n") {
        normalized = normalized.replace("\n\n\n", "\n\n");
    }
    if board.initialization.is_empty() && init_spawns.trim().is_empty() {
        normalized = normalized.replace(
            "fn init(_cx: init::Context) -> (Shared, Local) {\n\n",
            "fn init(_cx: init::Context) -> (Shared, Local) {\n",
        );
    }
    if tasks.trim().is_empty() {
        normalized = normalized.replace("\n\n}", "\n}");
    }
    normalized.push('\n');
    Ok(normalized)
}

fn render_prelude(template: &str, exports: &str) -> Result<String> {
    const MARKER: &str = "{{PRELUDE_EXPORTS}}";
    if template.matches(MARKER).count() != 1 {
        bail!("RTIC prelude template must contain exactly one {MARKER} marker");
    }
    let mut rendered = template.replace(MARKER, exports);
    while rendered.contains("\n\n\n") {
        rendered = rendered.replace("\n\n\n", "\n\n");
    }
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

fn indent(source: &str, spaces: usize) -> String {
    source.replace('\n', &format!("\n{}", " ".repeat(spaces)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hw_resources::PinId;

    #[test]
    fn nucleo_board_declares_port_zero_pin_five_as_typed_physical_data() {
        assert_eq!(nucleo_f401re::BOARD.hardware[0].pin(), PinId::new(0, 5));
    }

    #[test]
    fn fcu3_renders_green_blink_sbus_and_uart4_comport() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let rendered = render_target(
            root,
            &GenerationTarget {
                id: "ferrowasp_fcu3",
                board: &ferrowasp_fcu3::BOARD,
                app: &ferrowasp_fcu3_app::APP,
                generated_app_output: "unused.rs",
                prelude_output: "unused_prelude.rs",
            },
        )
        .unwrap();
        let application = rendered.application;
        let prelude = rendered.prelude;

        assert!(application.contains("const SYSTEM_CLOCK_HZ: u32 = 168_000_000;"));
        assert!(application.contains("green_led: Pin<'B', 1, Output<PushPull>>"));
        assert!(application.contains("gpiob.pb1"));
        assert!(!application.contains("user_button"));
        assert!(!application.contains("button_exti"));

        assert!(application.contains("binds = DMA1_STREAM5"));
        assert!(application.contains("binds = USART2"));
        assert!(application.contains("rx_pin: gpioa.pa3"));
        assert!(application.contains("rx_dma: dma1.5"));
        assert!(application.contains("SerialProtocol::Sbus"));
        assert!(application.contains("uart2_rc_endpoint: Uart2Rx"));

        assert!(application.contains("binds = DMA1_STREAM2"));
        assert!(application.contains("binds = UART4"));
        assert!(!application.contains("binds = USART4"));
        assert!(application.contains("rx_pin: gpioa.pa1"));
        assert!(application.contains("rx_dma: dma1.2"));
        assert!(application.contains("SerialProtocol::Raw"));
        assert!(application.contains("uart4_comport_endpoint: Uart4Rx"));
        assert!(!application.contains("gpioa.pa0"));
        assert!(!application.contains("dma1.4"));

        assert_eq!(
            application
                .matches("StreamsTuple::new(cx.device.DMA1")
                .count(),
            1
        );
        assert!(application.contains("init_usart2_rx_only"));
        assert!(application.contains("init_uart4_rx_only"));
        assert!(application.contains("uart2_rc_input.lock"));
        assert!(!application.contains("uart4_rc_input.lock(|snapshot|"));
        assert!(application.contains("Component `uart2` tasks"));
        assert!(application.contains("Component `uart4` tasks"));

        assert!(prelude.contains("Uart2Rx"));
        assert!(prelude.contains("Usart2RxOnlyResources"));
        assert!(prelude.contains("Uart4Rx"));
        assert!(prelude.contains("Uart4RxOnlyResources"));
    }

    #[test]
    fn empty_application_omits_tasks_and_init_spawns() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let empty_app = app::AppDeclaration::EMPTY;
        app::validate(&empty_app).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        let validated_board = backend::validate(&nucleo_f401re::BOARD).unwrap();
        let expanded = component::expand(&nucleo_f401re::BOARD, &empty_app).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &expanded).unwrap();
        let spawns = render_init_spawns(&resolved.init_spawned_tasks);
        let board = backend::render(&validated_board, &resolved).unwrap();
        let tasks = render_tasks(root, &resolved.tasks, &board).unwrap();
        let application = substitute(APP_TEMPLATE, &tasks, &spawns, &board).unwrap();
        let prelude = render_prelude(PRELUDE_TEMPLATE, &board.prelude_exports).unwrap();

        assert!(!application.contains("::spawn()"));
        assert!(!application.contains("#[task("));
        assert!(application.contains("mod prelude;"));
        assert!(application.contains("use crate::prelude::*;"));
        assert!(application.contains("device = stm32f4xx_hal::pac"));
        assert!(prelude.contains("pub(crate) use ferrowasp_stm32f4::rtic::hal as stm32f4xx_hal;"));
        assert!(syn::parse_file(&application).is_ok());
        assert!(syn::parse_file(&prelude).is_ok());
    }

    #[test]
    fn selected_init_spawn_is_rendered() {
        app::validate(&nucleo_f401re_app::APP).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        let validated_board = backend::validate(&nucleo_f401re::BOARD).unwrap();
        let expanded = component::expand(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &expanded).unwrap();
        let spawns = render_init_spawns(&resolved.init_spawned_tasks);
        backend::render(&validated_board, &resolved).unwrap();

        assert!(spawns.contains("blink_led::spawn()"));
        assert!(spawns.contains("rc_heartbeat::spawn()"));
        assert!(spawns.contains("uart2_consumer::spawn()"));
    }

    #[test]
    fn blink_includes_one_shot_report_task_with_count_argument() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        app::validate(&nucleo_f401re_app::APP).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        let validated_board = backend::validate(&nucleo_f401re::BOARD).unwrap();
        let expanded = component::expand(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &expanded).unwrap();
        let board = backend::render(&validated_board, &resolved).unwrap();
        let tasks = render_tasks(root, &resolved.tasks, &board).unwrap();
        let init_spawns = render_init_spawns(&resolved.init_spawned_tasks);

        assert!(tasks.contains("async fn report_blink(cx: report_blink::Context, count: u32)"));
        assert!(tasks.contains("async fn blink_led(mut cx: blink_led::Context)"));
        assert!(tasks.contains("const TOGGLE_INTERVAL_MS: u32 = 1_000;"));
        assert!(tasks.contains("Mono::delay(TOGGLE_INTERVAL_MS.millis()).await;"));
        assert!(!tasks.contains("cx.config.toggle_interval"));
        assert!(!tasks.contains("Mono::delay(5000.millis()).await;"));
        assert!(tasks.contains("cx.local.led3"));
        assert!(tasks.contains("cx.shared.blink_enabled"));
        assert!(!tasks.contains("cx.local.led;"));
        assert!(!tasks.contains("cx.shared.enabled"));
        assert!(tasks.contains("report_blink::spawn(blink_count)"));
        assert!(tasks.contains("defmt::info!(\"Blink {}\", count)"));
        assert!(!init_spawns.contains("report_blink::spawn"));
    }

    #[test]
    fn button_exti_task_and_blink_enable_state_are_rendered() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        app::validate(&nucleo_f401re_app::APP).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        let validated_board = backend::validate(&nucleo_f401re::BOARD).unwrap();
        let expanded = component::expand(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &expanded).unwrap();
        let spawns = render_init_spawns(&resolved.init_spawned_tasks);
        let board = backend::render(&validated_board, &resolved).unwrap();
        let tasks = render_tasks(root, &resolved.tasks, &board).unwrap();
        let application = substitute(APP_TEMPLATE, &tasks, &spawns, &board).unwrap();

        assert!(application.contains("binds = EXTI15_10"));
        assert!(application.contains("user_button: Pin<'C', 13, Input>"));
        assert!(application.contains("blink_enabled: bool"));
        assert!(application.contains("clear_interrupt_pending_bit"));
        assert!(application.contains("blink_enabled.lock"));
    }

    #[test]
    fn configurable_uart_component_and_comport_default_are_rendered() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        app::validate(&nucleo_f401re_app::APP).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        let validated_board = backend::validate(&nucleo_f401re::BOARD).unwrap();
        let expanded = component::expand(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &expanded).unwrap();
        let spawns = render_init_spawns(&resolved.init_spawned_tasks);
        let board = backend::render(&validated_board, &resolved).unwrap();
        let tasks = render_tasks(root, &resolved.tasks, &board).unwrap();
        let application = substitute(APP_TEMPLATE, &tasks, &spawns, &board).unwrap();
        let prelude = render_prelude(PRELUDE_TEMPLATE, &board.prelude_exports).unwrap();

        assert!(application.contains("dispatchers = [EXTI0, EXTI1]"));
        assert!(application.contains(
            "#[rtic::app(\n    device = stm32f4xx_hal::pac,\n    peripherals = true,\n    dispatchers = [EXTI0, EXTI1]\n)]"
        ));
        assert!(application.contains(
            "#[task(\n        binds = EXTI15_10,\n        priority = 2,\n        local = [user_button],\n        shared = [blink_enabled]\n    )]"
        ));
        assert!(application.contains("#[task(priority = 1)]"));
        assert!(application.contains("binds = DMA1_STREAM5"));
        assert!(application.contains("binds = USART2"));
        assert!(application.contains("rx_pin: gpioa.pa3"));
        assert!(application.contains("rx_dma: dma1.5"));
        assert!(application.contains("SerialProtocol::Raw"));
        assert!(application.contains("uart2_endpoint: Uart2Rx"));
        assert!(application.contains("uart2_rc_input: RcInputSnapshot"));
        assert!(application.contains("uart2_consumer_state: SerialConsumer"));
        assert!(application.contains("Usart2RxOnlyResources"));
        assert!(application.contains("UartRxStorageResources"));
        assert!(application.contains("SerialConsumer::new(SerialPortAssignment::ComPort)"));
        assert!(!application.contains("ferrowasp_io_core::serial::RcInputSnapshot"));
        assert!(!application.contains("ferrowasp_drivers::serial_consumer::SerialConsumer"));
        assert!(application.contains("ferrowasp_stm32f4::uart_dma::init_usart2_rx_only"));
        assert!(application.contains("ferrowasp_stm32f4::clocks::freeze_hsi"));
        assert!(application.contains("ferrowasp_stm32f4::exti::init_input"));
        assert!(application.contains("const SYSTEM_CLOCK_HZ: u32 = 84_000_000;"));
        assert!(application.contains(
            "ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), SYSTEM_CLOCK_HZ, false)"
        ));
        assert!(application.contains("Mono::start(cx.core.SYST, SYSTEM_CLOCK_HZ);"));
        assert!(!application.contains("Mono::start(cx.core.SYST, 84000000);"));
        assert!(application.contains("uart2_consumer_state"));
        assert!(application.contains("uart2_rc_input"));
        assert!(application.contains("uart2_rx_buffers"));
        assert!(application.contains(
            "uart2_rx_buffers: UartRxBufferBank =\n            ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank(),"
        ));
        assert!(application.contains("COMPORT: {}"));
        assert!(application.contains("RC heartbeat: no RC frame"));
        assert!(application.contains("const REPORT_INTERVAL_MS: u32 = 1_000;"));
        assert!(application.contains("Mono::delay(REPORT_INTERVAL_MS.millis()).await;"));
        assert!(!application.contains("cx.config.report_interval"));
        assert!(application.starts_with("// ================================="));
        assert!(application.contains("GENERATED FILE — DO NOT EDIT DIRECTLY"));
        assert!(prelude.starts_with("// ================================="));
        assert!(prelude.contains("GENERATED FILE — DO NOT EDIT DIRECTLY"));
        assert!(application.contains("mod prelude;"));
        assert!(application.contains("use crate::prelude::*;"));
        assert!(!application.contains("use ferrowasp_"));
        assert!(!application.contains("use panic_halt"));
        assert!(!application.contains("use defmt_rtt"));
        assert!(prelude.contains("// Firmware runtime handlers."));
        assert!(prelude.contains("use panic_halt as _;"));
        assert!(prelude.contains("use defmt_rtt as _;"));
        assert!(
            prelude
                .contains("// Application-facing imports selected from the resolved application.")
        );
        assert!(prelude.contains("pub(crate) use ferrowasp_stm32f4::rtic::hal as stm32f4xx_hal;"));
        assert!(prelude.contains("pub(crate) use ferrowasp_stm32f4::rtic::prelude::*;"));
        assert!(prelude.contains("pub(crate) use ferrowasp_drivers::serial_consumer"));
        assert!(prelude.contains("RcInputSnapshot"));
        assert!(prelude.contains("SerialPortAssignment"));
        assert!(prelude.contains("Uart2Rx"));
        assert!(prelude.contains("UartRxBufferBank"));

        let resources_start = crate::component_divider("uart2", "resources", false);
        let resources_end = crate::component_divider("uart2", "resources", true);
        assert_eq!(application.matches(&resources_start).count(), 5);
        assert_eq!(application.matches(&resources_end).count(), 5);
        assert!(!application.contains("Component `uart2` shared resources"));
        assert!(!application.contains("Component `uart2` local resources"));
        assert!(!application.contains("Component `uart2` init-local resources"));

        let init_start = application.find("fn init(").unwrap();
        let first_task = application[init_start..].find("#[task(").unwrap() + init_start;
        let init_body = &application[init_start..first_task];
        assert_eq!(init_body.matches(&resources_start).count(), 2);
        assert_eq!(init_body.matches(&resources_end).count(), 2);

        let system_start = crate::scope_divider("System initialization", false, 88, '=');
        let system_end = crate::scope_divider("System initialization", true, 88, '=');
        assert_eq!(init_body.matches(&system_start).count(), 1);
        assert_eq!(init_body.matches(&system_end).count(), 1);

        let component_start = crate::scope_divider("Component `uart2`", false, 88, '=');
        let component_end = crate::scope_divider("Component `uart2`", true, 88, '=');
        assert_eq!(init_body.matches(&component_start).count(), 1);
        assert_eq!(init_body.matches(&component_end).count(), 1);
        assert!(!init_body.contains("Component resources"));
        assert!(!init_body.contains("Task resources"));
        assert!(init_body.contains("let dma1 = StreamsTuple::new"));
        assert!(init_body.contains("// Task `blink_led`\n"));
        assert!(init_body.contains("// Task `rc_heartbeat`\n"));
        assert!(init_body.contains("// Component `uart2` task `uart2_consumer`\n"));
        assert!(init_body.contains("uart2_consumer::spawn()"));
        let startup_header = crate::scope_divider("Initial task startup", false, 88, '=');
        assert_eq!(init_body.matches(&startup_header).count(), 1);
        assert!(init_body.find(&startup_header) < init_body.find("blink_led::spawn()"));

        let tasks_start = crate::component_divider("uart2", "tasks", false);
        let tasks_end = crate::component_divider("uart2", "tasks", true);
        assert_eq!(application.matches(&tasks_start).count(), 1);
        assert_eq!(application.matches(&tasks_end).count(), 1);
        assert_eq!(tasks_start.len(), 88);
        assert_eq!(tasks_end.len(), 88);

        let component_tasks_start = application.find(&tasks_start).unwrap();
        let component_tasks_end = application.find(&tasks_end).unwrap();
        for task in ["fn uart2_dma_irq", "fn uart2_idle_irq", "fn uart2_consumer"] {
            let position = application.find(task).unwrap();
            assert!(component_tasks_start < position && position < component_tasks_end);
        }
    }
}
