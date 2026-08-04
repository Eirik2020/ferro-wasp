//! STM32F401 board validation and algorithmic GPIO/EXTI rendering.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use super::RenderedBoardInit;
use crate::{
    board::{BoardDeclaration, MonotonicDeclaration},
    hw_resources::{
        ClockSource, Drive, GpioMode, HardwareResource, InterruptEdge, Level, PinId, Pull,
        SerialProtocol,
    },
    resolve::{ResolvedApp, ResolvedResourceUsage, ResolvedTaskTrigger},
    task::HardwareInterrupt,
};

mod pins;

/// STM32F401 board declaration with validated physical-pin assignments.
#[derive(Debug)]
pub(crate) struct ValidatedBoard<'a> {
    declaration: &'a BoardDeclaration,
    pins_by_id: BTreeMap<&'static str, PinId>,
}

/// Validates STM32F401 clock, monotonic, package-pin, and pin-ownership facts.
pub fn validate(board: &BoardDeclaration) -> Result<ValidatedBoard<'_>> {
    if board.target.clock.source != ClockSource::InternalHighSpeed
        || board.target.clock.sysclk_hz != 84_000_000
    {
        bail!(
            "STM32F401RE backend requires board `{}` to use an 84 MHz HSI clock",
            board.id
        );
    }
    let MonotonicDeclaration::SysTick { id, clock_hz } = board.monotonic;
    if id != "Mono" || clock_hz != board.target.clock.sysclk_hz {
        bail!(
            "STM32F401RE backend requires board `{}` Mono to use the system clock",
            board.id
        );
    }
    let mut hardware_by_pin = BTreeMap::new();
    let mut pins_by_id = BTreeMap::new();
    for hardware in board.hardware {
        let pin = hardware.pin();
        pins::validate_f401re_lqfp64(pin)?;
        if let Some(uart) = hardware.uart_rx_dma()
            && (uart.uart.number != 2
                || uart.rx_pin != PinId::new(0, 3)
                || uart.dma.controller != 0
                || uart.dma.stream != 5
                || uart.dma.channel != 4
                || uart.protocol != SerialProtocol::Sbus)
        {
            bail!(
                "STM32F401 backend currently supports DMA SBUS only as USART2 RX on PA3 using DMA1 Stream 5 Channel 4; resource `{}` requests UART{}, {}, DMA{} Stream {} Channel {}",
                uart.id,
                uart.uart.number,
                pins::pin_name(uart.rx_pin)?,
                uart.dma.controller + 1,
                uart.dma.stream,
                uart.dma.channel,
            );
        }
        if let Some(existing_id) = hardware_by_pin.insert(pin, hardware.id()) {
            bail!(
                "board `{}` assigns physical pin `{}` to both `{existing_id}` and `{}`",
                board.id,
                pins::pin_name(pin)?,
                hardware.id()
            );
        }
        pins_by_id.insert(hardware.id(), pin);
    }
    Ok(ValidatedBoard {
        declaration: board,
        pins_by_id,
    })
}

/// Renders STM32F401 RTIC resources and initialization for a resolved application.
pub fn render(board: &ValidatedBoard<'_>, app: &ResolvedApp<'_>) -> Result<RenderedBoardInit> {
    if app.tasks.is_empty() {
        return Ok(RenderedBoardInit {
            dispatchers: "EXTI0".to_owned(),
            interrupt_bindings: BTreeMap::new(),
            imports: String::new(),
            monotonic_declaration: String::new(),
            init_attribute: "#[init]".to_owned(),
            shared_struct: "struct Shared {}".to_owned(),
            shared_value: "Shared {}".to_owned(),
            local_struct: "struct Local {}".to_owned(),
            local_value: "Local {}".to_owned(),
            initialization: String::new(),
        });
    }

    let declaration = board.declaration;
    let MonotonicDeclaration::SysTick {
        id,
        clock_hz: monotonic_clock_hz,
    } = declaration.monotonic;
    let resources_by_id = app
        .resources
        .iter()
        .map(|resource| (resource.hardware.id(), resource))
        .collect::<BTreeMap<_, _>>();
    let pins_by_id = app
        .resources
        .iter()
        .map(|resource| {
            let pin = board
                .pins_by_id
                .get(resource.hardware.id())
                .copied()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "resolved resource `{}` was not present in the validated board",
                        resource.hardware.id()
                    )
                })?;
            Ok((resource.hardware.id(), pin))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut local_fields = Vec::new();
    let mut local_values = Vec::new();
    let mut shared_fields = Vec::new();
    let mut shared_values = Vec::new();
    for resource in &app.resources {
        let pin = pins_by_id
            .get(resource.hardware.id())
            .copied()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "resolved resource `{}` has no validated STM32 pin",
                    resource.hardware.id()
                )
            })?;
        let field = render_resource_field(resource.hardware, pin)?;
        match resource.usage {
            ResolvedResourceUsage::Local { .. } => {
                local_fields.push(field);
                local_values.push(resource.hardware.id().to_owned());
            }
            ResolvedResourceUsage::Shared { .. } => {
                shared_fields.push(field);
                shared_values.push(resource.hardware.id().to_owned());
            }
        }
    }
    for resource in &app.software_local_resources {
        debug_assert_eq!(resource.task_ids.len(), 1);
        let declaration = resource.declaration;
        local_fields.push(format!(
            "{}: {},",
            declaration.id(),
            declaration.rust_type()
        ));
        local_values.push(format!(
            "{}: {}",
            declaration.id(),
            declaration.initial_value()
        ));
    }
    for resource in &app.software_shared_resources {
        debug_assert!(!resource.task_ids.is_empty());
        let declaration = resource.declaration;
        shared_fields.push(format!(
            "{}: {},",
            declaration.id(),
            declaration.rust_type()
        ));
        shared_values.push(format!(
            "{}: {}",
            declaration.id(),
            declaration.initial_value()
        ));
    }

    let has_resources = !app.resource_initialization_order.is_empty();
    let rcc_binding = if has_resources { "mut rcc" } else { "_rcc" };
    let mut initialization = format!(
        "let {rcc_binding} =\n    ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), {}, false);\n{id}::start(cx.core.SYST, {monotonic_clock_hz});",
        declaration.target.clock.sysclk_hz,
    );
    let used_ports = pins_by_id
        .values()
        .map(|pin| pin.port)
        .collect::<BTreeSet<_>>();
    for port in used_ports {
        let port = pins::port_letter(port)?;
        let port_lowercase = port.to_ascii_lowercase();
        initialization.push_str(&format!(
            "\nlet gpio{port_lowercase} = cx.device.GPIO{port}.split(&mut rcc);"
        ));
    }
    let has_exti_input = app.resources.iter().any(|resource| {
        matches!(
            resource.hardware.gpio().map(|gpio| gpio.mode),
            Some(GpioMode::Input { interrupt: Some(_) })
        )
    });
    if has_exti_input {
        initialization.push_str(
            "\nlet mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);\nlet mut exti = cx.device.EXTI;",
        );
    }
    if app
        .resources
        .iter()
        .any(|resource| resource.hardware.uart_rx_dma().is_some())
    {
        initialization.push_str("\nlet dma1 = StreamsTuple::new(cx.device.DMA1, &mut rcc);");
    }
    for resource_id in &app.resource_initialization_order {
        let resource = resources_by_id.get(resource_id).ok_or_else(|| {
            anyhow::anyhow!(
                "resolved initialization order references unknown resource `{resource_id}`"
            )
        })?;
        let pin = pins_by_id.get(resource_id).copied().ok_or_else(|| {
            anyhow::anyhow!(
                "resolved initialization order references resource `{resource_id}` without a validated STM32 pin"
            )
        })?;
        initialization.push('\n');
        initialization.push_str(&render_resource_initialization(resource.hardware, pin)?);
    }

    let has_digital_output = app.resources.iter().any(|resource| {
        matches!(
            resource.hardware.gpio().map(|gpio| gpio.mode),
            Some(GpioMode::Output { .. })
        )
    });
    let has_uart_rx_dma = app
        .resources
        .iter()
        .any(|resource| resource.hardware.uart_rx_dma().is_some());
    let mut imports = vec!["use ferrowasp_stm32f4::rtic::prelude::*;"];
    if has_digital_output {
        imports.insert(
            0,
            "use ferrowasp_io_core::digital::prelude::{OutputPin, StatefulOutputPin};",
        );
    }
    if has_uart_rx_dma {
        imports.insert(
            0,
            "use ferrowasp_stm32f4::uart_dma::{UartRxIrqOutcome, UartRxReadOutcome, UART_RX_BUFFER_SIZE};\nuse sbus_rs::StreamingParser;",
        );
    }

    let (dispatchers, interrupt_bindings) = render_interrupt_bindings(app, &board.pins_by_id)?;
    Ok(RenderedBoardInit {
        dispatchers,
        interrupt_bindings,
        imports: imports.join("\n"),
        monotonic_declaration: format!("systick_monotonic!({id}, 1_000);"),
        init_attribute: render_init_attribute(app),
        shared_struct: render_resource_struct("Shared", &shared_fields),
        shared_value: render_resource_value("Shared", &shared_values),
        local_struct: render_resource_struct("Local", &local_fields),
        local_value: render_resource_value("Local", &local_values),
        initialization,
    })
}

fn render_resource_field(hardware: &HardwareResource, pin: PinId) -> Result<String> {
    match hardware {
        HardwareResource::Gpio(gpio) => {
            let port = pins::port_letter(pin.port)?;
            match gpio.mode {
                GpioMode::Output {
                    drive: Drive::PushPull,
                    ..
                } => {
                    let rust_type = format!("Pin<'{port}', {}, Output<PushPull>>", pin.pin);
                    Ok(format!("{}: {rust_type},", gpio.id))
                }
                GpioMode::Input { .. } => {
                    Ok(format!("{}: Pin<'{port}', {}, Input>,", gpio.id, pin.pin))
                }
            }
        }
        HardwareResource::UartRxDma(uart) => Ok(format!(
            "{}: ferrowasp_stm32f4::uart_dma::Uart2SbusRx,",
            uart.id
        )),
    }
}

fn render_resource_initialization(hardware: &HardwareResource, pin: PinId) -> Result<String> {
    match hardware {
        HardwareResource::Gpio(gpio) => {
            let port = pins::port_letter(pin.port)?.to_ascii_lowercase();
            let number = pin.pin;
            let pull = match gpio.pull {
                Pull::None => "None",
                Pull::Up => "Up",
                Pull::Down => "Down",
            };
            match gpio.mode {
                GpioMode::Output {
                    drive: Drive::PushPull,
                    initial_level,
                } => {
                    let initial_state = match initial_level {
                        Level::Low => "Low",
                        Level::High => "High",
                    };
                    let id = gpio.id;
                    Ok(format!(
                        "let mut {id} = gpio{port}.p{port}{number}.into_push_pull_output_in_state(PinState::{initial_state});\n{id}.set_internal_resistor(Pull::{pull});\n{id}.set_speed(Speed::Low);"
                    ))
                }
                GpioMode::Input { interrupt } => {
                    let id = gpio.id;
                    let mut initialization =
                        format!("let {id} = Input::new(gpio{port}.p{port}{number}, Pull::{pull});");
                    if let Some(interrupt) = interrupt {
                        let edge = match interrupt.edge {
                            InterruptEdge::Rising => "Rising",
                            InterruptEdge::Falling => "Falling",
                            InterruptEdge::Both => "RisingFalling",
                        };
                        initialization.push_str(&format!(
                            "\nlet {id} = ferrowasp_stm32f4::exti::init_input({id}, &mut syscfg, &mut exti, Edge::{edge});"
                        ));
                    }
                    Ok(initialization)
                }
            }
        }
        HardwareResource::UartRxDma(uart) => {
            let port = pins::port_letter(pin.port)?.to_ascii_lowercase();
            let id = uart.id;
            let number = pin.pin;
            Ok(format!(
                "let {id} = ferrowasp_stm32f4::uart_dma::init_usart2_sbus_rx_only(\n    ferrowasp_stm32f4::uart_dma::Usart2SbusRxOnlyResources {{\n        rx_pin: gpio{port}.p{port}{number},\n        usart: cx.device.USART2,\n        rx_dma: dma1.5,\n    }},\n    &mut rcc,\n    ferrowasp_stm32f4::app_storage::UartRxStorageResources {{\n        buffers: cx.local.{id}_buffers,\n        free_queue: cx.local.{id}_free_queue,\n        filled_queue: cx.local.{id}_filled_queue,\n    }},\n);"
            ))
        }
    }
}

fn render_init_attribute(app: &ResolvedApp<'_>) -> String {
    let resources = app
        .resources
        .iter()
        .filter_map(|resource| resource.hardware.uart_rx_dma())
        .flat_map(|uart| {
            let id = uart.id;
            [
                format!(
                    "{id}_buffers: ferrowasp_stm32f4::app_storage::UartRxBufferBank = ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank()"
                ),
                format!(
                    "{id}_free_queue: ferrowasp_stm32f4::app_storage::UartRxFreeQueue = ferrowasp_stm32f4::app_storage::UartRxFreeQueue::new()"
                ),
                format!(
                    "{id}_filled_queue: ferrowasp_stm32f4::app_storage::UartRxFilledQueue = ferrowasp_stm32f4::app_storage::UartRxFilledQueue::new()"
                ),
            ]
        })
        .collect::<Vec<_>>();
    if resources.is_empty() {
        "#[init]".to_owned()
    } else {
        format!("#[init(local = [\n    {},\n])]", resources.join(",\n    "))
    }
}

fn render_interrupt_bindings(
    app: &ResolvedApp<'_>,
    pins_by_id: &BTreeMap<&str, PinId>,
) -> Result<(String, BTreeMap<&'static str, String>)> {
    let mut interrupt_owners = BTreeMap::<String, &str>::new();
    let mut bindings_by_task = BTreeMap::new();
    for task in &app.tasks {
        let ResolvedTaskTrigger::Interrupt {
            resource,
            interrupt,
        } = task.trigger
        else {
            continue;
        };
        let binding = match interrupt {
            HardwareInterrupt::Primary => {
                let pin = pins_by_id.get(resource.id()).copied().ok_or_else(|| {
                    anyhow::anyhow!(
                        "interrupt task `{}` trigger resource `{}` was not present in the validated board",
                        task.declaration.id,
                        resource.id()
                    )
                })?;
                pins::exti_binding(pin)?.to_owned()
            }
            HardwareInterrupt::DmaRx => {
                let uart = resource.uart_rx_dma().ok_or_else(|| {
                    anyhow::anyhow!(
                        "DMA RX interrupt task `{}` resolved non-UART resource `{}`",
                        task.declaration.id,
                        resource.id()
                    )
                })?;
                format!("DMA{}_STREAM{}", uart.dma.controller + 1, uart.dma.stream)
            }
            HardwareInterrupt::Peripheral => {
                let uart = resource.uart_rx_dma().ok_or_else(|| {
                    anyhow::anyhow!(
                        "UART interrupt task `{}` resolved non-UART resource `{}`",
                        task.declaration.id,
                        resource.id()
                    )
                })?;
                format!("USART{}", uart.uart.number)
            }
        };
        if let Some(existing_task) = interrupt_owners.insert(binding.clone(), task.declaration.id) {
            bail!(
                "interrupt tasks `{existing_task}` and `{}` both resolve to `{binding}`; grouped STM32 EXTI vectors require one demultiplexing task",
                task.declaration.id
            );
        }
        bindings_by_task.insert(task.declaration.id, binding);
    }

    let dispatcher_count = app
        .tasks
        .iter()
        .filter(|task| matches!(task.trigger, ResolvedTaskTrigger::Spawned))
        .map(|task| task.declaration.priority)
        .collect::<BTreeSet<_>>()
        .len()
        .max(1);
    let dispatchers = ["EXTI0", "EXTI1", "EXTI2", "EXTI3", "EXTI4"]
        .into_iter()
        .filter(|candidate| !interrupt_owners.contains_key(*candidate))
        .take(dispatcher_count)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if dispatchers.len() != dispatcher_count {
        return Err(anyhow::anyhow!(
            "STM32F401RE backend needs {dispatcher_count} software dispatchers but does not have enough free EXTI0 through EXTI4 vectors"
        ));
    }
    Ok((dispatchers.join(", "), bindings_by_task))
}

fn render_resource_struct(name: &str, fields: &[String]) -> String {
    if fields.is_empty() {
        format!("struct {name} {{}}")
    } else {
        format!("struct {name} {{\n    {}\n}}", fields.join("\n    "))
    }
}

fn render_resource_value(name: &str, resources: &[String]) -> String {
    if resources.is_empty() {
        format!("{name} {{}}")
    } else {
        format!("{name} {{ {} }}", resources.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{
            AppDeclaration, InitDeclaration, SoftwareResourceDeclaration,
            SoftwareResourcesDeclaration,
        },
        board::MonotonicDeclaration,
        hw_resources::{
            Clock, DmaChannel, ExternalInterrupt, Gpio, HardwareResource, Mcu, PinId, Target,
            UartId, UartRxDma,
        },
        resolve::resolve,
        task::{
            TaskDeclaration, TaskDefinition, boolean, digital_output, interrupt_input, resource,
        },
    };

    const fn output(id: &'static str, port: u8, pin: u8, level: Level) -> HardwareResource {
        HardwareResource::Gpio(Gpio::output(id, PinId::new(port, pin), level))
    }

    const fn exti_input(id: &'static str, port: u8, pin: u8) -> HardwareResource {
        HardwareResource::Gpio(
            Gpio::input(id, PinId::new(port, pin))
                .pull_up()
                .interrupt_on(InterruptEdge::Falling),
        )
    }

    const LED2: HardwareResource = output("led2", 0, 5, Level::Low);
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "nucleo_f401re",
        target: Target {
            mcu: Mcu::Stm32F401,
            clock: Clock {
                source: ClockSource::InternalHighSpeed,
                sysclk_hz: 84_000_000,
            },
        },
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[LED2],
    };
    const BLINK_DEFINITION: TaskDefinition =
        TaskDefinition::asynchronous("blink").with_local(&[digital_output("led")]);
    const BLINK: TaskDeclaration = BLINK_DEFINITION
        .spawned_as("blink_led")
        .priority(1)
        .with_local(&[resource("led").to_hw("led2")]);

    #[test]
    fn maps_a5_to_stm32f4_type_and_initialization() {
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        let board = validate(&BOARD).unwrap();
        let resolved = resolve(&BOARD, &app).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert!(
            rendered
                .local_struct
                .contains("Pin<'A', 5, Output<PushPull>>")
        );
        assert!(rendered.initialization.contains("gpioa.pa5"));
        assert!(rendered.initialization.contains("PinState::Low"));
    }

    #[test]
    fn maps_pc13_to_exti_input_and_renders_shared_state() {
        const SOFTWARE_SHARED: &[SoftwareResourceDeclaration] =
            &[SoftwareResourceDeclaration::bool("blink_enabled", true)];
        const BUTTON: HardwareResource = exti_input("user_button", 2, 13);
        const BUTTON_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[BUTTON],
            ..BOARD
        };
        const BUTTON_DEFINITION: TaskDefinition = TaskDefinition::synchronous("button")
            .with_local(&[interrupt_input("button")])
            .with_shared(&[boolean("enabled")]);
        const BUTTON_TASK: TaskDeclaration = BUTTON_DEFINITION
            .interrupt_as("button_exti", "button")
            .priority(2)
            .with_local(&[resource("button").to_hw("user_button")])
            .with_shared(&[resource("enabled").to_sw("blink_enabled")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BUTTON_TASK],
            software_resources: SoftwareResourcesDeclaration {
                shared: SOFTWARE_SHARED,
                local: &[],
            },
        };

        let board = validate(&BUTTON_BOARD).unwrap();
        let resolved = resolve(&BUTTON_BOARD, &app).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert!(rendered.local_struct.contains("Pin<'C', 13, Input>"));
        assert!(rendered.shared_struct.contains("blink_enabled: bool"));
        assert!(rendered.shared_value.contains("blink_enabled: true"));
        assert!(rendered.initialization.contains("GPIOC.split"));
        assert!(rendered.initialization.contains("exti::init_input"));
        assert!(rendered.initialization.contains("Edge::Falling"));
        assert!(!rendered.initialization.contains("GPIOA.split"));
        assert!(!rendered.imports.contains("ferrowasp_io_core"));
        assert_eq!(
            rendered.interrupt_bindings.get("button_exti"),
            Some(&"EXTI15_10".to_owned())
        );
    }

    #[test]
    fn renders_input_pull_and_interrupt_edge_from_the_declaration() {
        const INPUT: HardwareResource = HardwareResource::Gpio(Gpio {
            id: "input",
            pin: PinId::new(1, 4),
            pull: Pull::Down,
            mode: GpioMode::Input {
                interrupt: Some(ExternalInterrupt {
                    edge: InterruptEdge::Both,
                }),
            },
        });

        let rendered = render_resource_initialization(&INPUT, INPUT.pin()).unwrap();

        assert!(rendered.contains("Input::new(gpiob.pb4, Pull::Down)"));
        assert!(rendered.contains("Edge::RisingFalling"));
    }

    #[test]
    fn renders_floating_input_without_interrupt_initialization() {
        const INPUT: HardwareResource = HardwareResource::Gpio(Gpio {
            id: "input",
            pin: PinId::new(0, 1),
            pull: Pull::None,
            mode: GpioMode::Input { interrupt: None },
        });

        let rendered = render_resource_initialization(&INPUT, INPUT.pin()).unwrap();

        assert_eq!(rendered, "let input = Input::new(gpioa.pa1, Pull::None);");
        assert!(!rendered.contains("exti::init_input"));
    }

    #[test]
    fn rejects_pin_not_exposed_by_stm32f401re_package() {
        const UNSUPPORTED: BoardDeclaration = BoardDeclaration {
            hardware: &[output("unsupported", 1, 11, Level::Low)],
            ..BOARD
        };

        let error = validate(&UNSUPPORTED).unwrap_err();
        assert!(error.to_string().contains("does not expose"));
        assert!(error.to_string().contains("PB11"));
    }

    #[test]
    fn validates_only_the_supported_f401_sbus_dma_route() {
        const INVALID_ROUTE: HardwareResource = HardwareResource::UartRxDma(UartRxDma::sbus(
            "sbus",
            UartId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 4, 4),
        ));
        const INVALID_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[INVALID_ROUTE],
            ..BOARD
        };

        let error = validate(&INVALID_BOARD).unwrap_err();
        assert!(error.to_string().contains("DMA1 Stream 5 Channel 4"));
        assert!(error.to_string().contains("DMA1 Stream 4 Channel 4"));
    }

    #[test]
    fn renders_pb4_output_without_a_pin_specific_branch() {
        const PB4_LED: HardwareResource = output("led2", 1, 4, Level::Low);
        const PB4_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[PB4_LED],
            ..BOARD
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&PB4_BOARD).unwrap();
        let resolved = resolve(&PB4_BOARD, &app).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert!(
            rendered
                .local_struct
                .contains("Pin<'B', 4, Output<PushPull>>")
        );
        assert!(rendered.initialization.contains("GPIOB.split"));
        assert!(rendered.initialization.contains("gpiob.pb4"));
        assert!(!rendered.initialization.contains("GPIOA.split"));
    }

    #[test]
    fn splits_a_used_gpio_port_once() {
        const AUX: HardwareResource = output("aux", 0, 6, Level::Low);
        const TWO_OUTPUT_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[LED2, AUX],
            ..BOARD
        };
        const AUX_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("aux").with_local(&[digital_output("output")]);
        const AUX_TASK: TaskDeclaration = AUX_DEFINITION
            .spawned_as("aux_task")
            .priority(1)
            .with_local(&[resource("output").to_hw("aux")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK, AUX_TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&TWO_OUTPUT_BOARD).unwrap();
        let resolved = resolve(&TWO_OUTPUT_BOARD, &app).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert_eq!(rendered.initialization.matches("GPIOA.split").count(), 1);
        assert!(rendered.initialization.contains("gpioa.pa5"));
        assert!(rendered.initialization.contains("gpioa.pa6"));
    }

    #[test]
    fn exti_zero_moves_the_software_dispatcher_to_exti_one() {
        const BUTTON: HardwareResource = exti_input("button", 0, 0);
        const BUTTON_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[BUTTON],
            ..BOARD
        };
        const BUTTON_DEFINITION: TaskDefinition =
            TaskDefinition::synchronous("button").with_local(&[interrupt_input("input")]);
        const BUTTON_TASK: TaskDeclaration = BUTTON_DEFINITION
            .interrupt_as("button_irq", "input")
            .priority(2)
            .with_local(&[resource("input").to_hw("button")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BUTTON_TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&BUTTON_BOARD).unwrap();
        let resolved = resolve(&BUTTON_BOARD, &app).unwrap();
        let rendered = render(&board, &resolved).unwrap();
        assert_eq!(
            rendered.interrupt_bindings.get("button_irq"),
            Some(&"EXTI0".to_owned())
        );
        assert_eq!(rendered.dispatchers, "EXTI1");
    }

    #[test]
    fn rejects_two_tasks_on_one_grouped_exti_vector() {
        const FIRST_INPUT: HardwareResource = exti_input("first_input", 0, 5);
        const SECOND_INPUT: HardwareResource = exti_input("second_input", 0, 6);
        const INPUT_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[FIRST_INPUT, SECOND_INPUT],
            ..BOARD
        };
        const INTERRUPT_DEFINITION: TaskDefinition =
            TaskDefinition::synchronous("input_irq").with_local(&[interrupt_input("input")]);
        const FIRST_TASK: TaskDeclaration = INTERRUPT_DEFINITION
            .interrupt_as("first_irq", "input")
            .priority(2)
            .with_local(&[resource("input").to_hw("first_input")]);
        const SECOND_TASK: TaskDeclaration = INTERRUPT_DEFINITION
            .interrupt_as("second_irq", "input")
            .priority(2)
            .with_local(&[resource("input").to_hw("second_input")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[FIRST_TASK, SECOND_TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&INPUT_BOARD).unwrap();
        let resolved = resolve(&INPUT_BOARD, &app).unwrap();
        let error = render(&board, &resolved).unwrap_err();
        assert!(error.to_string().contains("both resolve to `EXTI9_5`"));
    }

    #[test]
    fn unused_board_hardware_is_not_initialized() {
        const UNUSED: HardwareResource = output("unused", 1, 4, Level::High);
        const BOARD_WITH_UNUSED: BoardDeclaration = BoardDeclaration {
            hardware: &[UNUSED, LED2],
            ..BOARD
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        let board = validate(&BOARD_WITH_UNUSED).unwrap();
        let resolved = resolve(&BOARD_WITH_UNUSED, &app).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert!(!rendered.local_struct.contains("unused"));
        assert!(!rendered.initialization.contains("unused"));
        assert!(rendered.initialization.contains("led2"));
        assert!(!rendered.initialization.contains("GPIOB.split"));
    }

    #[test]
    fn rejects_duplicate_physical_pin_assignments() {
        const DUPLICATE: BoardDeclaration = BoardDeclaration {
            hardware: &[LED2, output("other", 0, 5, Level::Low)],
            ..BOARD
        };

        let error = validate(&DUPLICATE).unwrap_err();
        assert!(error.to_string().contains("assigns physical pin `PA5`"));
        assert!(error.to_string().contains("led2"));
        assert!(error.to_string().contains("other"));
    }

    #[test]
    fn empty_application_renders_without_backend_imports() {
        let resolved = resolve(&BOARD, &AppDeclaration::EMPTY).unwrap();
        let board = validate(&BOARD).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert!(rendered.imports.is_empty());
        assert_eq!(rendered.shared_struct, "struct Shared {}");
        assert_eq!(rendered.local_struct, "struct Local {}");
        assert!(rendered.initialization.is_empty());
    }

    #[test]
    fn rejects_out_of_range_numeric_pin_coordinates() {
        const INVALID: BoardDeclaration = BoardDeclaration {
            hardware: &[output("invalid", 0, 16, Level::Low)],
            ..BOARD
        };

        let error = validate(&INVALID).unwrap_err();
        assert!(error.to_string().contains("must be 0 through 15"));
    }
}
