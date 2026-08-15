//! STM32F4 board validation and algorithmic GPIO, EXTI, and serial rendering.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use ferrowasp_io_core::serial::SerialProtocol;

use super::RenderedBoardInit;
use crate::{
    board::{BoardDeclaration, MonotonicDeclaration},
    component::{
        ComponentConfiguration, ComponentInitLocalResource, ComponentSoftwareResourceInitializer,
        ExpandedSoftwareResourceKind,
    },
    hw_resources::{
        ClockSource, Drive, GpioMode, HardwareResource, InterruptEdge, Level, Mcu, PinId, Pull,
        UartRxDma,
    },
    resolve::{ResolvedApp, ResolvedResource, ResolvedResourceUsage, ResolvedTaskTrigger},
    task::{
        HardwareInterrupt, SOFTWARE_LINE_CONSUMER, SOFTWARE_MOTOR_CMD, SOFTWARE_RC_INPUT_SNAPSHOT,
        SOFTWARE_SBUS_CONSUMER, SOFTWARE_SERIAL_RX,
    },
};

mod pins;

const HAL_ALIAS_EXPORT: &str = "pub(crate) use ferrowasp_stm32f4::rtic::hal as stm32f4xx_hal;";

/// STM32F4 board declaration with validated physical-pin and serial-route assignments.
#[derive(Debug)]
pub(crate) struct ValidatedBoard<'a> {
    declaration: &'a BoardDeclaration,
    pins_by_id: BTreeMap<&'static str, PinId>,
    serial_routes_by_id: BTreeMap<&'static str, Stm32SerialRoute>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Stm32SerialRoute {
    Usart2Rx,
    Uart4Rx,
}

impl Stm32SerialRoute {
    const fn irq_type(self) -> &'static str {
        match self {
            Self::Usart2Rx => "Uart2RxIrq",
            Self::Uart4Rx => "Uart4RxIrq",
        }
    }

    const fn peripheral_interrupt(self) -> &'static str {
        match self {
            Self::Usart2Rx => "USART2",
            Self::Uart4Rx => "UART4",
        }
    }
}

struct RenderedResourceField {
    owner_component: Option<String>,
    declaration: String,
}

/// Validates STM32F4 clock, monotonic, package-pin, and pin-ownership facts.
pub fn validate(board: &BoardDeclaration) -> Result<ValidatedBoard<'_>> {
    let (part, expected_system_clock_hz) = match board.target.mcu {
        Mcu::Stm32F401 => ("STM32F401RE", 84_000_000),
        Mcu::Stm32F405 => ("STM32F405RG", 168_000_000),
    };
    if board.target.clock.sysclk_hz != expected_system_clock_hz {
        bail!(
            "{part} backend requires board `{}` to use a {} MHz system clock",
            board.id,
            expected_system_clock_hz / 1_000_000,
        );
    }
    match (board.target.mcu, board.target.clock.source) {
        (Mcu::Stm32F401 | Mcu::Stm32F405, ClockSource::InternalHighSpeed) => {}
        (
            Mcu::Stm32F405,
            ClockSource::ExternalCrystal {
                frequency_hz: 8_000_000,
            },
        ) => {}
        (Mcu::Stm32F405, ClockSource::ExternalCrystal { frequency_hz }) => {
            bail!(
                "STM32F405RG backend supports an external crystal only at 8 MHz; board `{}` declares {frequency_hz} Hz",
                board.id
            );
        }
        (Mcu::Stm32F401, ClockSource::ExternalCrystal { frequency_hz }) => {
            bail!(
                "STM32F401RE backend does not yet support an external crystal; board `{}` declares {frequency_hz} Hz",
                board.id
            );
        }
        (_, ClockSource::ExternalClock { frequency_hz }) => {
            bail!(
                "{part} backend does not yet support an externally generated clock; board `{}` declares {frequency_hz} Hz",
                board.id
            );
        }
    }
    let MonotonicDeclaration::SysTick { id, clock_hz } = board.monotonic;
    if id != "Mono" || clock_hz != board.target.clock.sysclk_hz {
        bail!(
            "{part} backend requires board `{}` Mono to use the system clock",
            board.id
        );
    }
    let mut hardware_by_pin = BTreeMap::new();
    let mut pins_by_id = BTreeMap::new();
    let mut serial_routes_by_id = BTreeMap::new();
    for hardware in board.hardware {
        let pin = hardware.pin();
        pins::validate_lqfp64(board.target.mcu, pin)?;
        if let Some(serial) = hardware.uart_rx_dma() {
            serial_routes_by_id.insert(
                hardware.id(),
                validate_serial_route(board.target.mcu, serial)?,
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
        serial_routes_by_id,
    })
}

fn validate_serial_route(mcu: Mcu, serial: &UartRxDma) -> Result<Stm32SerialRoute> {
    let (route, expected_pin, expected_dma, concrete_name) = match (mcu, serial.serial_port.number)
    {
        (Mcu::Stm32F401 | Mcu::Stm32F405, 2) => (
            Stm32SerialRoute::Usart2Rx,
            PinId::new(0, 3),
            (0, 5, 4),
            "USART2",
        ),
        (Mcu::Stm32F405, 4) => (
            Stm32SerialRoute::Uart4Rx,
            PinId::new(0, 1),
            (0, 2, 4),
            "UART4",
        ),
        (Mcu::Stm32F401, number) => {
            bail!(
                "STM32F401RE backend supports DMA receive only on serial port 2; resource `{}` requests serial port {number}",
                serial.id
            )
        }
        (Mcu::Stm32F405, number) => {
            bail!(
                "STM32F405RG backend supports DMA receive only on serial ports 2 and 4; resource `{}` requests serial port {number}",
                serial.id
            )
        }
    };
    if serial.rx_pin != expected_pin
        || serial.dma.controller != expected_dma.0
        || serial.dma.stream != expected_dma.1
        || serial.dma.channel != expected_dma.2
    {
        bail!(
            "{} backend requires serial port {} ({concrete_name}) RX on {} using DMA{} Stream {} Channel {}; resource `{}` requests {}, DMA{} Stream {} Channel {}",
            match mcu {
                Mcu::Stm32F401 => "STM32F401RE",
                Mcu::Stm32F405 => "STM32F405RG",
            },
            serial.serial_port.number,
            pins::pin_name(expected_pin)?,
            expected_dma.0 + 1,
            expected_dma.1,
            expected_dma.2,
            serial.id,
            pins::pin_name(serial.rx_pin)?,
            serial.dma.controller + 1,
            serial.dma.stream,
            serial.dma.channel,
        );
    }
    Ok(route)
}

/// Renders STM32F4 RTIC resources and initialization for a resolved application.
pub fn render(board: &ValidatedBoard<'_>, app: &ResolvedApp<'_>) -> Result<RenderedBoardInit> {
    if app.tasks.is_empty() {
        return Ok(RenderedBoardInit {
            dispatchers: "EXTI0".to_owned(),
            interrupt_bindings: BTreeMap::new(),
            prelude_exports: HAL_ALIAS_EXPORT.to_owned(),
            timing_declarations: String::new(),
            init_attribute: "#[init]".to_owned(),
            shared_struct: "struct Shared {}".to_owned(),
            shared_value: "Shared {}".to_owned(),
            local_struct: "struct Local {}".to_owned(),
            local_value: "Local {}".to_owned(),
            initialization: String::new(),
        });
    }

    let declaration = board.declaration;
    let MonotonicDeclaration::SysTick { id, .. } = declaration.monotonic;
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
        let serial_route = board
            .serial_routes_by_id
            .get(resource.hardware.id())
            .copied();
        let field = render_resource_field(resource.hardware, pin, serial_route)?;
        let owner_component = resolved_hardware_owner(app, resource.hardware.id());
        match resource.usage {
            ResolvedResourceUsage::Local { .. } => {
                local_fields.push(RenderedResourceField {
                    owner_component: owner_component.clone(),
                    declaration: field,
                });
                local_values.push(RenderedResourceField {
                    owner_component,
                    declaration: resource.hardware.id().to_owned(),
                });
            }
            ResolvedResourceUsage::Shared { .. } => {
                shared_fields.push(RenderedResourceField {
                    owner_component: owner_component.clone(),
                    declaration: field,
                });
                shared_values.push(RenderedResourceField {
                    owner_component,
                    declaration: resource.hardware.id().to_owned(),
                });
            }
        }
    }
    for resource in &app.software_local_resources {
        debug_assert_eq!(resource.task_ids.len(), 1);
        let declaration = resource.declaration;
        local_fields.push(RenderedResourceField {
            owner_component: declaration.owner_component.clone(),
            declaration: format!("{}: {},", declaration.id, declaration.kind.rust_type()),
        });
        local_values.push(RenderedResourceField {
            owner_component: declaration.owner_component.clone(),
            declaration: format!(
                "{}: {}",
                declaration.id,
                declaration.kind.initial_value(&declaration.id)
            ),
        });
    }
    for resource in &app.software_shared_resources {
        debug_assert!(!resource.task_ids.is_empty());
        let declaration = resource.declaration;
        shared_fields.push(RenderedResourceField {
            owner_component: declaration.owner_component.clone(),
            declaration: format!("{}: {},", declaration.id, declaration.kind.rust_type()),
        });
        shared_values.push(RenderedResourceField {
            owner_component: declaration.owner_component.clone(),
            declaration: format!(
                "{}: {}",
                declaration.id,
                declaration.kind.initial_value(&declaration.id)
            ),
        });
    }

    let has_resources = !app.resource_initialization_order.is_empty();
    let rcc_binding = if has_resources { "mut rcc" } else { "_rcc" };
    let requires_pll48 = declaration.target.clock.requires_pll48;
    let clock_initialization = match declaration.target.clock.source {
        ClockSource::InternalHighSpeed => format!(
            "ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), SYSTEM_CLOCK_HZ, {requires_pll48})"
        ),
        ClockSource::ExternalCrystal { frequency_hz } => format!(
            "ferrowasp_stm32f4::clocks::freeze_hse(cx.device.RCC.constrain(), {}, SYSTEM_CLOCK_HZ, {requires_pll48})",
            render_u32_literal(frequency_hz)
        ),
        ClockSource::ExternalClock { .. } => {
            unreachable!("unsupported external clock was rejected during backend validation")
        }
    };
    let mut system_initialization = format!(
        "let {rcc_binding} =\n    {clock_initialization};\n{id}::start(cx.core.SYST, SYSTEM_CLOCK_HZ);",
    );
    let used_ports = pins_by_id
        .values()
        .map(|pin| pin.port)
        .collect::<BTreeSet<_>>();
    for port in used_ports {
        let port = pins::port_letter(port)?;
        let port_lowercase = port.to_ascii_lowercase();
        system_initialization.push_str(&format!(
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
        system_initialization.push_str(
            "\nlet mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);\nlet mut exti = cx.device.EXTI;",
        );
    }
    let mut task_initializations = Vec::<(String, Vec<String>)>::new();
    let mut component_initializations = Vec::<(String, Vec<String>)>::new();
    let mut dma1_initialized = false;
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
        let uart_configuration = resource
            .hardware
            .uart_rx_dma()
            .map(|_| resolved_uart_configuration(app, resource.hardware.id()))
            .transpose()?;
        let serial_route = board.serial_routes_by_id.get(*resource_id).copied();
        let mut resource_initialization = render_resource_initialization(
            resource.hardware,
            pin,
            serial_route,
            uart_configuration,
        )?;
        if resource.hardware.uart_rx_dma().is_some() && !dma1_initialized {
            resource_initialization = format!(
                "let dma1 = StreamsTuple::new(cx.device.DMA1, &mut rcc);\n{resource_initialization}"
            );
            dma1_initialized = true;
        }
        if let Some(owner) = resolved_hardware_owner(app, resource.hardware.id()) {
            push_initialization(
                &mut component_initializations,
                &owner,
                resource_initialization,
            );
        } else if let Some(task_id) = standalone_hardware_owner(app, resource) {
            push_initialization(&mut task_initializations, task_id, resource_initialization);
        } else {
            system_initialization.push('\n');
            system_initialization.push_str(&resource_initialization);
        }
    }
    for channel in app.init_local_resources {
        let owner = channel.owner_component.as_str();
        match channel.kind {
            ComponentInitLocalResource::ObserverChannel {
                publisher, reader, ..
            } => {
                let publisher_id = format!("{owner}_{publisher}");
                let reader_id = format!("{owner}_{reader}");
                let resolved_publisher = app
                    .software_local_resources
                    .iter()
                    .find(|resource| resource.declaration.id == publisher_id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "observer channel `{}` has no resolved local publisher",
                            channel.id
                        )
                    })?;
                let resolved_reader = app
                    .software_shared_resources
                    .iter()
                    .find(|resource| resource.declaration.id == reader_id);
                let reader_binding = resolved_reader
                    .map(|resource| resource.declaration.id.as_str())
                    .unwrap_or("_observer_reader");
                push_initialization(
                    &mut component_initializations,
                    owner,
                    format!(
                        "let ({}, {}) =\n    cx.local.{}.split();",
                        resolved_publisher.declaration.id, reader_binding, channel.id
                    ),
                );
            }
            ComponentInitLocalResource::SafetyChannel {
                producer, consumer, ..
            } => {
                let producer_id = format!("{owner}_{producer}");
                let consumer_id = format!("{owner}_{consumer}");
                let resolved_producer = app
                    .software_local_resources
                    .iter()
                    .find(|resource| resource.declaration.id == producer_id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "safety channel `{}` has no resolved local producer",
                            channel.id
                        )
                    })?;
                let resolved_consumer = app
                    .software_local_resources
                    .iter()
                    .find(|resource| resource.declaration.id == consumer_id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "safety channel `{}` has no resolved local consumer",
                            channel.id
                        )
                    })?;
                push_initialization(
                    &mut component_initializations,
                    owner,
                    format!(
                        "let ({}, {}) =\n    cx.local.{}.split();",
                        resolved_producer.declaration.id,
                        resolved_consumer.declaration.id,
                        channel.id
                    ),
                );
            }
            ComponentInitLocalResource::ObserverOutput { .. } => {
                let output_id = channel.id.strip_suffix("_channel").ok_or_else(|| {
                    anyhow::anyhow!(
                        "conceptual observer storage `{}` does not end in `_channel`",
                        channel.id
                    )
                })?;
                let publisher_id = format!("{output_id}_publisher");
                let reader_id = format!("{output_id}_reader");
                let resolved_publisher = app
                    .software_local_resources
                    .iter()
                    .find(|resource| resource.declaration.id == publisher_id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "observer output `{output_id}` has no resolved local publisher"
                        )
                    })?;
                let resolved_reader = app
                    .software_shared_resources
                    .iter()
                    .find(|resource| resource.declaration.id == reader_id);
                let reader_binding = resolved_reader
                    .map(|resource| resource.declaration.id.as_str())
                    .unwrap_or("_observer_reader");
                push_initialization(
                    &mut component_initializations,
                    owner,
                    format!(
                        "let ({}, {}) =\n    cx.local.{}.split();",
                        resolved_publisher.declaration.id, reader_binding, channel.id
                    ),
                );
            }
            ComponentInitLocalResource::SafetyOutput { .. } => {
                let output_id = channel.id.strip_suffix("_channel").ok_or_else(|| {
                    anyhow::anyhow!(
                        "conceptual safety storage `{}` does not end in `_channel`",
                        channel.id
                    )
                })?;
                let producer_id = format!("{output_id}_producer");
                let consumer_id = format!("{output_id}_consumer");
                let resolved_producer = app
                    .software_local_resources
                    .iter()
                    .find(|resource| resource.declaration.id == producer_id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "safety output `{output_id}` has no resolved local producer"
                        )
                    })?;
                let resolved_consumer = app
                    .software_local_resources
                    .iter()
                    .find(|resource| resource.declaration.id == consumer_id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "safety output `{output_id}` has no resolved local consumer"
                        )
                    })?;
                push_initialization(
                    &mut component_initializations,
                    owner,
                    format!(
                        "let ({}, {}) =\n    cx.local.{}.split();",
                        resolved_producer.declaration.id,
                        resolved_consumer.declaration.id,
                        channel.id
                    ),
                );
            }
            ComponentInitLocalResource::UartRxBuffers
            | ComponentInitLocalResource::UartRxFreeQueue
            | ComponentInitLocalResource::UartRxFilledQueue => {}
        }
    }
    let mut initialization = vec![guarded_init_section(
        "System initialization",
        88,
        '=',
        &system_initialization,
    )];
    initialization.extend(task_initializations.iter().map(|(task_id, statements)| {
        guarded_init_section(
            &format!("Task `{task_id}`"),
            80,
            '-',
            &statements.join("\n"),
        )
    }));
    initialization.extend(component_initializations.iter().map(|(owner, statements)| {
        guarded_init_section(
            &format!("Component `{owner}`"),
            88,
            '=',
            &statements.join("\n"),
        )
    }));
    let initialization = initialization.join("\n\n");

    let has_digital_output = app.resources.iter().any(|resource| {
        matches!(
            resource.hardware.gpio().map(|gpio| gpio.mode),
            Some(GpioMode::Output { .. })
        )
    });
    let used_serial_routes = app
        .resources
        .iter()
        .filter_map(|resource| {
            board
                .serial_routes_by_id
                .get(resource.hardware.id())
                .copied()
        })
        .collect::<BTreeSet<_>>();
    let mut imports = vec![
        HAL_ALIAS_EXPORT.to_owned(),
        "pub(crate) use ferrowasp_stm32f4::rtic::prelude::*;".to_owned(),
    ];
    if has_digital_output {
        imports.insert(
            0,
            "pub(crate) use ferrowasp_io_core::digital::prelude::{OutputPin, StatefulOutputPin};"
                .to_owned(),
        );
    }
    if !used_serial_routes.is_empty() {
        let mut route_imports = Vec::new();
        if used_serial_routes.contains(&Stm32SerialRoute::Usart2Rx) {
            route_imports.extend(["Uart2RxIrq", "Usart2RxOnlyResources"]);
        }
        if used_serial_routes.contains(&Stm32SerialRoute::Uart4Rx) {
            route_imports.extend(["Uart4RxIrq", "Uart4RxOnlyResources"]);
        }
        route_imports.push("UartRxParserSide");
        imports.insert(
            0,
            format!(
                "pub(crate) use ferrowasp_io_core::serial::SerialProtocol;\npub(crate) use ferrowasp_stm32f4::{{\n    app_storage::{{UartRxBufferBank, UartRxFilledQueue, UartRxFreeQueue, UartRxStorageResources}},\n    uart_dma::{{{}, UartRxIrqOutcome}},\n}};",
                route_imports.join(", ")
            ),
        );
    }
    let has_value_type = |type_id| {
        app.software_shared_resources
            .iter()
            .chain(&app.software_local_resources)
            .any(|resource| {
                matches!(
                    resource.declaration.kind.capability(),
                    crate::task::TaskResourceCapability::Software(actual)
                        | crate::task::TaskResourceCapability::ObserverPublisher(actual)
                        | crate::task::TaskResourceCapability::ObserverReader(actual)
                        | crate::task::TaskResourceCapability::SafetyProducer(actual)
                        | crate::task::TaskResourceCapability::SafetyConsumer(actual)
                        if actual == type_id
                )
            })
    };
    let has_serial_rx = has_value_type(SOFTWARE_SERIAL_RX);
    if has_serial_rx {
        imports.insert(
            0,
            "pub(crate) use ferrowasp_stm32f4::uart_dma::{UartRxReadOutcome, UART_RX_BUFFER_SIZE};"
                .to_owned(),
        );
    }
    let has_rc_input_snapshot = has_value_type(SOFTWARE_RC_INPUT_SNAPSHOT);
    if has_rc_input_snapshot {
        imports.insert(
            0,
            "pub(crate) use ferrowasp_io_core::serial::RcInputSnapshot;".to_owned(),
        );
    }
    let has_observer_channel = app.init_local_resources.iter().any(|resource| {
        matches!(
            resource.kind,
            ComponentInitLocalResource::ObserverChannel { .. }
                | ComponentInitLocalResource::ObserverOutput { .. }
        )
    });
    if has_observer_channel {
        imports.insert(
            0,
            "pub(crate) use ferrowasp_core::observer_channel::{ObserverChannel, ObserverPublisher, ObserverReader};"
                .to_owned(),
        );
    }
    let has_safety_channel = app.init_local_resources.iter().any(|resource| {
        matches!(
            resource.kind,
            ComponentInitLocalResource::SafetyChannel { .. }
                | ComponentInitLocalResource::SafetyOutput { .. }
        )
    });
    if has_safety_channel {
        imports.insert(
            0,
            "pub(crate) use ferrowasp_core::safety_channel::{SafetyChannel, SafetyConsumer, SafetyProducer};"
                .to_owned(),
        );
    }
    if has_value_type(SOFTWARE_MOTOR_CMD) {
        imports.insert(
            0,
            "pub(crate) use ferrowasp_core::safety::MotorCmd;".to_owned(),
        );
    }
    let has_sbus_consumer = has_value_type(SOFTWARE_SBUS_CONSUMER);
    let has_line_consumer = has_value_type(SOFTWARE_LINE_CONSUMER);
    if has_sbus_consumer || has_line_consumer {
        let mut consumer_imports = Vec::new();
        if has_sbus_consumer {
            consumer_imports.push("SbusConsumer");
        }
        if has_line_consumer {
            consumer_imports.extend(["LineConsumer", "LineConsumerEvent"]);
        }
        imports.insert(
            0,
            format!(
                "pub(crate) use ferrowasp_drivers::serial_consumer::{{{}}};",
                consumer_imports.join(", ")
            ),
        );
    }

    let (dispatchers, interrupt_bindings) =
        render_interrupt_bindings(app, &board.pins_by_id, &board.serial_routes_by_id)?;
    Ok(RenderedBoardInit {
        dispatchers,
        interrupt_bindings,
        prelude_exports: imports.join("\n"),
        timing_declarations: format!(
            "// Timing configuration generated from the board declaration.\nconst SYSTEM_CLOCK_HZ: u32 = {};\n\nsystick_monotonic!({id}, 1_000);",
            render_u32_literal(declaration.target.clock.sysclk_hz)
        ),
        init_attribute: render_init_attribute(app),
        shared_struct: render_resource_struct("Shared", &shared_fields),
        shared_value: render_resource_value("Shared", &shared_values),
        local_struct: render_resource_struct("Local", &local_fields),
        local_value: render_resource_value("Local", &local_values),
        initialization,
    })
}

fn render_u32_literal(value: u32) -> String {
    let digits = value.to_string();
    let mut rendered = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            rendered.push('_');
        }
        rendered.push(digit);
    }
    rendered
}

fn render_resource_field(
    hardware: &HardwareResource,
    pin: PinId,
    serial_route: Option<Stm32SerialRoute>,
) -> Result<String> {
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
        HardwareResource::UartRxDma(serial) => {
            let route = serial_route.ok_or_else(|| {
                anyhow::anyhow!(
                    "serial resource `{}` has no validated STM32 route",
                    serial.id
                )
            })?;
            Ok(format!("{}: {},", serial.id, route.irq_type()))
        }
    }
}

fn render_resource_initialization(
    hardware: &HardwareResource,
    pin: PinId,
    serial_route: Option<Stm32SerialRoute>,
    uart_configuration: Option<(&str, &str, SerialProtocol)>,
) -> Result<String> {
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
        HardwareResource::UartRxDma(serial) => {
            let port = pins::port_letter(pin.port)?.to_ascii_lowercase();
            let id = serial.id;
            let number = pin.pin;
            let (owner, rx_output, protocol) = uart_configuration.ok_or_else(|| {
                anyhow::anyhow!("serial resource `{id}` has no owning serial component")
            })?;
            let mode = match protocol {
                SerialProtocol::Sbus => "Sbus",
                SerialProtocol::Raw => "Raw",
                SerialProtocol::Disabled => {
                    bail!("disabled serial resource `{id}` reached STM32 initialization")
                }
                unsupported => {
                    bail!(
                        "serial resource `{id}` profile {unsupported:?} is not implemented by this generated endpoint"
                    )
                }
            };
            let route = serial_route.ok_or_else(|| {
                anyhow::anyhow!("serial resource `{id}` has no validated STM32 route")
            })?;
            let (initializer, resources_type, peripheral_field, peripheral, dma_stream) =
                match route {
                    Stm32SerialRoute::Usart2Rx => (
                        "init_usart2_rx_only",
                        "Usart2RxOnlyResources",
                        "usart",
                        "USART2",
                        5,
                    ),
                    Stm32SerialRoute::Uart4Rx => (
                        "init_uart4_rx_only",
                        "Uart4RxOnlyResources",
                        "uart",
                        "UART4",
                        2,
                    ),
                };
            Ok(format!(
                "let {owner}_parts = ferrowasp_stm32f4::uart_dma::{initializer}(\n    {resources_type} {{\n        rx_pin: gpio{port}.p{port}{number},\n        {peripheral_field}: cx.device.{peripheral},\n        rx_dma: dma1.{dma_stream},\n    }},\n    &mut rcc,\n    SerialProtocol::{mode},\n    UartRxStorageResources {{\n        buffers: cx.local.{owner}_rx_buffers,\n        free_queue: cx.local.{owner}_free_queue,\n        filled_queue: cx.local.{owner}_filled_queue,\n    }},\n);\nlet {id} = {owner}_parts.irq;\nlet {rx_output} = {owner}_parts.parser;"
            ))
        }
    }
}

fn render_init_attribute(app: &ResolvedApp<'_>) -> String {
    let resources = app
        .init_local_resources
        .iter()
        .map(|resource| {
            let id = &resource.id;
            let declaration = match resource.kind {
                ComponentInitLocalResource::UartRxBuffers => render_init_local(
                    id,
                    "UartRxBufferBank",
                    "ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank()",
                ),
                ComponentInitLocalResource::UartRxFreeQueue => {
                    render_init_local(id, "UartRxFreeQueue", "UartRxFreeQueue::new()")
                }
                ComponentInitLocalResource::UartRxFilledQueue => {
                    render_init_local(id, "UartRxFilledQueue", "UartRxFilledQueue::new()")
                }
                ComponentInitLocalResource::ObserverChannel { rust_type, .. } => render_init_local(
                    id,
                    &format!("ObserverChannel<{rust_type}>"),
                    "ObserverChannel::new()",
                ),
                ComponentInitLocalResource::SafetyChannel {
                    rust_type,
                    queue_length,
                    ..
                } => render_init_local(
                    id,
                    &format!("SafetyChannel<{rust_type}, {queue_length}>"),
                    "SafetyChannel::new()",
                ),
                ComponentInitLocalResource::ObserverOutput { rust_type, .. } => render_init_local(
                    id,
                    &format!("ObserverChannel<{rust_type}>"),
                    "ObserverChannel::new()",
                ),
                ComponentInitLocalResource::SafetyOutput {
                    rust_type,
                    usable_capacity,
                    ..
                } => render_init_local(
                    id,
                    &format!("SafetyChannel<{rust_type}, {}>", usable_capacity + 1),
                    "SafetyChannel::new()",
                ),
            };
            (resource.owner_component.as_str(), declaration)
        })
        .collect::<Vec<_>>();
    if resources.is_empty() {
        "#[init]".to_owned()
    } else {
        let grouped = render_component_sections("resources", &resources, ",");
        format!("#[init(local = [\n    {grouped}\n])]")
    }
}

fn render_init_local(id: &str, rust_type: &str, initializer: &str) -> String {
    const MODULE_AND_ATTRIBUTE_INDENT: usize = 8;
    const LINE_WIDTH: usize = 88;

    let compact = format!("{id}: {rust_type} = {initializer}");
    if MODULE_AND_ATTRIBUTE_INDENT + compact.len() <= LINE_WIDTH {
        compact
    } else {
        format!("{id}: {rust_type} =\n        {initializer}")
    }
}

fn resolved_uart_configuration<'a>(
    app: &'a ResolvedApp<'a>,
    hardware_id: &str,
) -> Result<(&'a str, &'a str, SerialProtocol)> {
    let owner = app
        .tasks
        .iter()
        .filter(|task| {
            task.shared_resources.iter().any(|resource| {
                matches!(resource, crate::resolve::ResolvedTaskResource::Hardware { hardware, .. } if hardware.id() == hardware_id)
            })
        })
        .find_map(|task| task.declaration.owner_component.as_deref())
        .ok_or_else(|| anyhow::anyhow!("UART resource `{hardware_id}` is not owned by a component"))?;
    let output = app
        .software_shared_resources
        .iter()
        .find_map(|resource| {
            (resource.declaration.owner_component.as_deref() == Some(owner)
                && matches!(
                    resource.declaration.kind,
                    ExpandedSoftwareResourceKind::Component(component_resource)
                        if component_resource.initializer
                            == ComponentSoftwareResourceInitializer::SerialRxOutput
                ))
            .then_some(resource.declaration)
        })
        .ok_or_else(|| anyhow::anyhow!("serial component `{owner}` has no raw RX output"))?;
    let Some(ComponentConfiguration::SerialPort(protocol)) = output.owner_configuration else {
        bail!("serial component `{owner}` has no serial profile configuration");
    };
    Ok((owner, &output.id, protocol))
}

fn render_interrupt_bindings(
    app: &ResolvedApp<'_>,
    pins_by_id: &BTreeMap<&str, PinId>,
    serial_routes_by_id: &BTreeMap<&str, Stm32SerialRoute>,
) -> Result<(String, BTreeMap<String, String>)> {
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
                resource.uart_rx_dma().ok_or_else(|| {
                    anyhow::anyhow!(
                        "serial interrupt task `{}` resolved non-serial resource `{}`",
                        task.declaration.id,
                        resource.id()
                    )
                })?;
                serial_routes_by_id
                    .get(resource.id())
                    .copied()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "serial interrupt task `{}` resource `{}` has no validated STM32 route",
                            task.declaration.id,
                            resource.id()
                        )
                    })?
                    .peripheral_interrupt()
                    .to_owned()
            }
        };
        if let Some(existing_task) =
            interrupt_owners.insert(binding.clone(), task.declaration.id.as_str())
        {
            bail!(
                "interrupt tasks `{existing_task}` and `{}` both resolve to `{binding}`; grouped STM32 EXTI vectors require one demultiplexing task",
                task.declaration.id
            );
        }
        bindings_by_task.insert(task.declaration.id.clone(), binding);
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
            "STM32F4 backend needs {dispatcher_count} software dispatchers but does not have enough free EXTI0 through EXTI4 vectors"
        ));
    }
    Ok((dispatchers.join(", "), bindings_by_task))
}

fn render_resource_struct(name: &str, fields: &[RenderedResourceField]) -> String {
    if fields.is_empty() {
        format!("struct {name} {{}}")
    } else {
        let mut contents = fields
            .iter()
            .filter(|field| field.owner_component.is_none())
            .map(|field| field.declaration.clone())
            .collect::<Vec<_>>();
        let mut component_order = Vec::new();
        for field in fields {
            if let Some(owner) = field.owner_component.as_deref()
                && !component_order.contains(&owner)
            {
                component_order.push(owner);
            }
        }
        for owner in component_order {
            let declarations = fields
                .iter()
                .filter(|field| field.owner_component.as_deref() == Some(owner))
                .map(|field| field.declaration.as_str())
                .collect::<Vec<_>>();
            contents.push(component_section(owner, "resources", &declarations, ""));
        }
        format!("struct {name} {{\n    {}\n}}", contents.join("\n    "))
    }
}

fn resolved_hardware_owner(app: &ResolvedApp<'_>, hardware_id: &str) -> Option<String> {
    let mut owner = None::<&str>;
    for task in &app.tasks {
        let uses_hardware = task
            .local_resources
            .iter()
            .chain(&task.shared_resources)
            .any(|resource| {
                matches!(resource, crate::resolve::ResolvedTaskResource::Hardware { hardware, .. } if hardware.id() == hardware_id)
            });
        if !uses_hardware {
            continue;
        }
        let task_owner = task.declaration.owner_component.as_deref()?;
        if owner.is_some_and(|existing| existing != task_owner) {
            return None;
        }
        owner = Some(task_owner);
    }
    owner.map(str::to_owned)
}

fn standalone_hardware_owner<'a>(
    app: &'a ResolvedApp<'a>,
    resource: &ResolvedResource<'a>,
) -> Option<&'a str> {
    let task_id = match &resource.usage {
        ResolvedResourceUsage::Local { task_id } => *task_id,
        ResolvedResourceUsage::Shared { task_ids } if task_ids.len() == 1 => task_ids[0],
        ResolvedResourceUsage::Shared { .. } => return None,
    };
    app.tasks
        .iter()
        .find(|task| task.declaration.id == task_id)
        .filter(|task| task.declaration.owner_component.is_none())
        .map(|task| task.declaration.id.as_str())
}

fn push_initialization(
    groups: &mut Vec<(String, Vec<String>)>,
    owner: &str,
    initialization: String,
) {
    if let Some((_, statements)) = groups.iter_mut().find(|(candidate, _)| candidate == owner) {
        statements.push(initialization);
    } else {
        groups.push((owner.to_owned(), vec![initialization]));
    }
}

fn guarded_init_section(label: &str, width: usize, fill: char, body: &str) -> String {
    let start = crate::scope_divider(label, false, width, fill);
    let end = crate::scope_divider(label, true, width, fill);
    format!("{start}\n{body}\n{end}")
}

fn render_component_sections(kind: &str, resources: &[(&str, String)], separator: &str) -> String {
    let mut owners = Vec::new();
    for (owner, _) in resources {
        if !owners.contains(owner) {
            owners.push(*owner);
        }
    }
    owners
        .into_iter()
        .map(|owner| {
            let declarations = resources
                .iter()
                .filter(|(candidate, _)| *candidate == owner)
                .map(|(_, declaration)| declaration.as_str())
                .collect::<Vec<_>>();
            component_section(owner, kind, &declarations, separator)
        })
        .collect::<Vec<_>>()
        .join("\n    ")
}

fn component_section(owner: &str, kind: &str, declarations: &[&str], separator: &str) -> String {
    let body = declarations.join(&format!("{separator}\n    "));
    let start = crate::component_divider(owner, kind, false);
    let end = crate::component_divider(owner, kind, true);
    format!("{start}\n    {body}{separator}\n    {end}")
}

fn render_resource_value(name: &str, resources: &[RenderedResourceField]) -> String {
    if resources.is_empty() {
        format!("{name} {{}}")
    } else {
        let mut contents = resources
            .iter()
            .filter(|resource| resource.owner_component.is_none())
            .map(|resource| format!("{},", resource.declaration))
            .collect::<Vec<_>>();
        let mut component_order = Vec::new();
        for resource in resources {
            if let Some(owner) = resource.owner_component.as_deref()
                && !component_order.contains(&owner)
            {
                component_order.push(owner);
            }
        }
        for owner in component_order {
            let declarations = resources
                .iter()
                .filter(|resource| resource.owner_component.as_deref() == Some(owner))
                .map(|resource| resource.declaration.as_str())
                .collect::<Vec<_>>();
            contents.push(component_section(owner, "resources", &declarations, ","));
        }
        format!("{name} {{\n    {}\n}}", contents.join("\n    "))
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
            Clock, DmaChannel, ExternalInterrupt, Gpio, HardwareResource, Mcu, PinId, SerialPortId,
            Target, UartRxDma,
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
                requires_pll48: false,
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
            components: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        let board = validate(&BOARD).unwrap();
        let expanded = crate::component::expand(&BOARD, &app).unwrap();
        let resolved = resolve(&BOARD, &expanded).unwrap();
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
            components: &[],
            software_resources: SoftwareResourcesDeclaration {
                shared: SOFTWARE_SHARED,
                local: &[],
            },
        };

        let board = validate(&BUTTON_BOARD).unwrap();
        let expanded = crate::component::expand(&BUTTON_BOARD, &app).unwrap();
        let resolved = resolve(&BUTTON_BOARD, &expanded).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert!(rendered.local_struct.contains("Pin<'C', 13, Input>"));
        assert!(rendered.shared_struct.contains("blink_enabled: bool"));
        assert!(rendered.shared_value.contains("blink_enabled: true"));
        assert!(rendered.initialization.contains("GPIOC.split"));
        assert!(rendered.initialization.contains("exti::init_input"));
        assert!(rendered.initialization.contains("Edge::Falling"));
        assert!(!rendered.initialization.contains("GPIOA.split"));
        assert!(!rendered.prelude_exports.contains("ferrowasp_io_core"));
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

        let rendered = render_resource_initialization(&INPUT, INPUT.pin(), None, None).unwrap();

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

        let rendered = render_resource_initialization(&INPUT, INPUT.pin(), None, None).unwrap();

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
        const INVALID_ROUTE: HardwareResource = HardwareResource::UartRxDma(UartRxDma::new(
            "sbus",
            SerialPortId::new(2),
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
    fn validates_both_fcu3_receive_routes() {
        const USART2: HardwareResource = HardwareResource::UartRxDma(UartRxDma::new(
            "uart2",
            SerialPortId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 5, 4),
        ));
        const UART4: HardwareResource = HardwareResource::UartRxDma(UartRxDma::new(
            "uart4",
            SerialPortId::new(4),
            PinId::new(0, 1),
            DmaChannel::new(0, 2, 4),
        ));
        const FCU3: BoardDeclaration = BoardDeclaration {
            id: "ferrowasp_fcu3",
            target: Target::internal_high_speed(Mcu::Stm32F405, 168_000_000),
            monotonic: MonotonicDeclaration::SysTick {
                id: "Mono",
                clock_hz: 168_000_000,
            },
            hardware: &[USART2, UART4],
        };

        let validated = validate(&FCU3).unwrap();
        assert_eq!(
            validated.serial_routes_by_id.get("uart2"),
            Some(&Stm32SerialRoute::Usart2Rx)
        );
        assert_eq!(
            validated.serial_routes_by_id.get("uart4"),
            Some(&Stm32SerialRoute::Uart4Rx)
        );
        assert_eq!(
            validated.serial_routes_by_id["uart2"].peripheral_interrupt(),
            "USART2"
        );
        assert_eq!(
            validated.serial_routes_by_id["uart4"].peripheral_interrupt(),
            "UART4"
        );
    }

    #[test]
    fn validates_foxeer_eight_megahertz_hse_and_rejects_other_crystals() {
        const FOXEER_CLOCK: BoardDeclaration = BoardDeclaration {
            id: "foxeer_f405_v2",
            target: Target::external_crystal(Mcu::Stm32F405, 8_000_000, 168_000_000, true),
            monotonic: MonotonicDeclaration::SysTick {
                id: "Mono",
                clock_hz: 168_000_000,
            },
            hardware: &[LED2],
        };
        const WRONG_CRYSTAL: BoardDeclaration = BoardDeclaration {
            target: Target::external_crystal(Mcu::Stm32F405, 12_000_000, 168_000_000, true),
            ..FOXEER_CLOCK
        };

        assert!(validate(&FOXEER_CLOCK).is_ok());
        let error = validate(&WRONG_CRYSTAL).unwrap_err().to_string();
        assert!(error.contains("external crystal only at 8 MHz"));
        assert!(error.contains("12000000 Hz"));
    }

    #[test]
    fn rejects_invalid_fcu3_uart4_route_and_f401_port_four() {
        const WRONG_UART4: HardwareResource = HardwareResource::UartRxDma(UartRxDma::new(
            "uart4",
            SerialPortId::new(4),
            PinId::new(0, 1),
            DmaChannel::new(0, 3, 4),
        ));
        const FCU3: BoardDeclaration = BoardDeclaration {
            id: "ferrowasp_fcu3",
            target: Target::internal_high_speed(Mcu::Stm32F405, 168_000_000),
            monotonic: MonotonicDeclaration::SysTick {
                id: "Mono",
                clock_hz: 168_000_000,
            },
            hardware: &[WRONG_UART4],
        };
        let error = validate(&FCU3).unwrap_err().to_string();
        assert!(error.contains("UART4"));
        assert!(error.contains("DMA1 Stream 2 Channel 4"));
        assert!(error.contains("DMA1 Stream 3 Channel 4"));

        const F401_PORT4: BoardDeclaration = BoardDeclaration {
            hardware: &[HardwareResource::UartRxDma(UartRxDma::new(
                "uart4",
                SerialPortId::new(4),
                PinId::new(0, 1),
                DmaChannel::new(0, 2, 4),
            ))],
            ..BOARD
        };
        let error = validate(&F401_PORT4).unwrap_err().to_string();
        assert!(error.contains("supports DMA receive only on serial port 2"));
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
            components: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&PB4_BOARD).unwrap();
        let expanded = crate::component::expand(&PB4_BOARD, &app).unwrap();
        let resolved = resolve(&PB4_BOARD, &expanded).unwrap();
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
            components: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&TWO_OUTPUT_BOARD).unwrap();
        let expanded = crate::component::expand(&TWO_OUTPUT_BOARD, &app).unwrap();
        let resolved = resolve(&TWO_OUTPUT_BOARD, &expanded).unwrap();
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
            components: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&BUTTON_BOARD).unwrap();
        let expanded = crate::component::expand(&BUTTON_BOARD, &app).unwrap();
        let resolved = resolve(&BUTTON_BOARD, &expanded).unwrap();
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
            components: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let board = validate(&INPUT_BOARD).unwrap();
        let expanded = crate::component::expand(&INPUT_BOARD, &app).unwrap();
        let resolved = resolve(&INPUT_BOARD, &expanded).unwrap();
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
            components: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        let board = validate(&BOARD_WITH_UNUSED).unwrap();
        let expanded = crate::component::expand(&BOARD_WITH_UNUSED, &app).unwrap();
        let resolved = resolve(&BOARD_WITH_UNUSED, &expanded).unwrap();
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
    fn empty_application_renders_only_the_required_hal_alias() {
        let expanded = crate::component::expand(&BOARD, &AppDeclaration::EMPTY).unwrap();
        let resolved = resolve(&BOARD, &expanded).unwrap();
        let board = validate(&BOARD).unwrap();
        let rendered = render(&board, &resolved).unwrap();

        assert_eq!(rendered.prelude_exports, HAL_ALIAS_EXPORT);
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
