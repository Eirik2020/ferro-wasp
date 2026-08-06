//! STM32F4-specific lowering for the resolved application graph.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use ferrowasp_io_core::serial::{SerialProfile, SerialProtocol};

use crate::{
    hardware_definitions::stm32f4::{
        dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
        gpio::{Drive, GpioMode, InterruptEdge, Level, Pull},
        hw_endpoint::serial_endpoint::{
            SerialEndpointDirection, SerialEndpointResourceOwnership, SerialEndpointResourceRole,
        },
        mcu::{ClockSource, Mcu},
        pins::{GpioPort, PinId},
        serial::SerialPeripheral,
    },
    rtic::{
        composition::SharedValue,
        resolve::{ResolvedApp, ResolvedSerialEndpoint},
    },
};

/// One static resource declared by RTIC's `#[init(local = [...])]` attribute.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedInitLocal {
    pub(crate) id: String,
    pub(crate) rust_type: String,
    pub(crate) initializer: String,
}

/// Concrete STM32F4 fragments consumed by the generic source renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedHardware {
    pub(crate) imports: String,
    pub(crate) timing: String,
    pub(crate) shared_fields: Vec<String>,
    pub(crate) shared_values: Vec<String>,
    pub(crate) local_fields: Vec<String>,
    pub(crate) local_values: Vec<String>,
    pub(crate) init_locals: Vec<RenderedInitLocal>,
    pub(crate) initialization: String,
}

pub(crate) fn lower(app: &ResolvedApp) -> Result<RenderedHardware> {
    if app.board.mcu.device != Mcu::Stm32f405 {
        bail!("the STM32F4 renderer currently supports only STM32F405");
    }

    let mut shared_fields = Vec::new();
    let mut shared_values = Vec::new();
    for resource in &app.shared_resources {
        let (rust_type, initial) = match resource.initial {
            SharedValue::Bool(value) => ("bool", value.to_string()),
            SharedValue::U32(value) => ("u32", render_u32(value)),
        };
        shared_fields.push(format!("{}: {rust_type},", resource.id));
        shared_values.push(format!("{}: {initial}", resource.id));
    }

    let mut local_fields = Vec::new();
    let mut local_values = Vec::new();
    for resource in &app.gpio_resources {
        let gpio = resource.hardware;
        let port = port_letter(gpio.pin.port)?;
        let rust_type = match gpio.mode {
            GpioMode::Input { .. } => format!("Pin<'{port}', {}, Input>", gpio.pin.pin),
            GpioMode::Output {
                drive: Drive::PushPull,
                ..
            } => format!("Pin<'{port}', {}, Output<PushPull>>", gpio.pin.pin),
        };
        local_fields.push(format!("{}: {rust_type},", gpio.id));
        local_values.push(gpio.id.to_owned());
    }

    let mut init_locals = Vec::new();
    for endpoint in &app.serial_endpoints {
        validate_supported_endpoint(endpoint)?;
        for resource in &endpoint.resources {
            match resource.ownership {
                SerialEndpointResourceOwnership::InitLocal => {
                    let (rust_type, initializer) = endpoint_init_local(resource.role)?;
                    init_locals.push(RenderedInitLocal {
                        id: resource.id.clone(),
                        rust_type: rust_type.to_owned(),
                        initializer: initializer.to_owned(),
                    });
                }
                SerialEndpointResourceOwnership::Local => {
                    local_fields.push(format!(
                        "{}: {},",
                        resource.id,
                        endpoint_resource_type(resource.role)?
                    ));
                    local_values.push(resource.id.clone());
                }
                SerialEndpointResourceOwnership::Shared => {
                    shared_fields.push(format!(
                        "{}: {},",
                        resource.id,
                        endpoint_resource_type(resource.role)?
                    ));
                    shared_values.push(format!("{}: {}", resource.id, resource.id));
                }
            }
        }
    }

    let initialization = render_initialization(app)?;
    let system_clock_hz = app.board.mcu.clock.system_frequency_hz;
    let timing = format!(
        "const SYSTEM_CLOCK_HZ: u32 = {};\n\nsystick_monotonic!(Mono, {});",
        render_u32(system_clock_hz),
        render_u32(app.monotonic.tick_hz())
    );

    Ok(RenderedHardware {
        imports: render_imports(app),
        timing,
        shared_fields,
        shared_values,
        local_fields,
        local_values,
        init_locals,
        initialization,
    })
}

fn render_imports(app: &ResolvedApp) -> String {
    let has_tx = app.serial_endpoints.iter().any(|endpoint| {
        matches!(
            endpoint.declaration.direction,
            SerialEndpointDirection::Bidirectional
        )
    });
    let tx_imports = if has_tx {
        "\n    memory::{UartOwnedTxChannel, UartOwnedTxCompletion, UartOwnedTxOwner, UartOwnedWriter},\n    serial::{UartTxDmaService, UartTxIrqOutcome, UartTxStartError},"
    } else {
        ""
    };
    let protocol_import = if app.serial_endpoints.iter().any(|endpoint| {
        matches!(
            endpoint.declaration.direction,
            SerialEndpointDirection::ReceiveOnly
        )
    }) {
        "SerialProtocol, "
    } else {
        ""
    };
    let fault_import = if has_tx { "SerialFault" } else { "" };
    format!(
        "use ferrowasp_io_core::serial::{{{protocol_import}{fault_import}}};\n\
use ferrowasp_stm32f4::{{\n    app_storage::{{\n        Uart4TxBuffer, UartRxBufferBank, UartRxFilledQueue, UartRxFreeQueue,\n        UartRxStorageResources,\n    }},\n    memory::{{\n        UartOwnedDiscontinuities, UartOwnedReader, UartOwnedRxChannel, UartOwnedRxProducer,\n    }},{tx_imports}\n    rtic::prelude::*,\n    serial::{{UartRxIrqOutcome, UartRxIrqService}},\n    uart_dma::{{\n        Uart4MspResources, Uart4RxIrq, Uart4TxDmaSide, UartRxParserSide,\n        UART4_TX_BUFFER_SIZE,\n    }},\n}};"
    )
}

fn endpoint_init_local(role: SerialEndpointResourceRole) -> Result<(&'static str, &'static str)> {
    match role {
        SerialEndpointResourceRole::RxBuffers => Ok((
            "UartRxBufferBank",
            "ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank()",
        )),
        SerialEndpointResourceRole::RxFreeQueue => {
            Ok(("UartRxFreeQueue", "UartRxFreeQueue::new()"))
        }
        SerialEndpointResourceRole::RxFilledQueue => {
            Ok(("UartRxFilledQueue", "UartRxFilledQueue::new()"))
        }
        SerialEndpointResourceRole::RxChannel => {
            Ok(("UartOwnedRxChannel", "UartOwnedRxChannel::new()"))
        }
        SerialEndpointResourceRole::TxBuffer => Ok(("Uart4TxBuffer", "[0; UART4_TX_BUFFER_SIZE]")),
        SerialEndpointResourceRole::TxChannel => {
            Ok(("UartOwnedTxChannel", "UartOwnedTxChannel::new()"))
        }
        unsupported => bail!("endpoint role {unsupported:?} cannot be initialization-local"),
    }
}

fn endpoint_resource_type(role: SerialEndpointResourceRole) -> Result<&'static str> {
    match role {
        SerialEndpointResourceRole::RxService => Ok("Uart4RxIrq"),
        SerialEndpointResourceRole::RxParser => Ok("UartRxParserSide"),
        SerialEndpointResourceRole::RxProducer => Ok("UartOwnedRxProducer<'static>"),
        SerialEndpointResourceRole::RxReader => Ok("UartOwnedReader<'static>"),
        SerialEndpointResourceRole::RxDiscontinuities => Ok("UartOwnedDiscontinuities<'static>"),
        SerialEndpointResourceRole::TxDma => Ok("Uart4TxDmaSide"),
        SerialEndpointResourceRole::TxWriter => Ok("UartOwnedWriter<'static>"),
        SerialEndpointResourceRole::TxOwner => Ok("UartOwnedTxOwner<'static>"),
        SerialEndpointResourceRole::TxCompletion => Ok("UartOwnedTxCompletion<'static>"),
        unsupported => bail!("endpoint role {unsupported:?} cannot be an RTIC field"),
    }
}

fn validate_supported_endpoint(endpoint: &ResolvedSerialEndpoint) -> Result<()> {
    if endpoint.hardware.port.peripheral != SerialPeripheral::Uart4 {
        bail!(
            "serial endpoint `{}` uses {:?}; the initial renderer supports UART4 only",
            endpoint.declaration.id,
            endpoint.hardware.port.peripheral
        );
    }
    let rx = endpoint
        .hardware
        .port
        .rx
        .context("validated UART4 endpoint has no RX route")?;
    require_route(
        endpoint.declaration.id,
        "RX",
        rx.pin,
        rx.dma,
        PinId::new(GpioPort::A, 1),
        DmaRoute::new(
            DmaController::Dma1,
            DmaStream::Stream2,
            DmaChannel::Channel4,
        ),
    )?;
    if matches!(
        endpoint.declaration.direction,
        SerialEndpointDirection::Bidirectional
    ) {
        if endpoint.declaration.profile != SerialProfile::msp() {
            bail!(
                "bidirectional UART4 endpoint `{}` must use the standard MSP profile",
                endpoint.declaration.id
            );
        }
        let tx = endpoint
            .hardware
            .port
            .tx
            .context("validated bidirectional UART4 endpoint has no TX route")?;
        require_route(
            endpoint.declaration.id,
            "TX",
            tx.pin,
            tx.dma,
            PinId::new(GpioPort::A, 0),
            DmaRoute::new(
                DmaController::Dma1,
                DmaStream::Stream4,
                DmaChannel::Channel4,
            ),
        )?;
    }
    Ok(())
}

fn require_route(
    endpoint: &str,
    signal: &str,
    actual_pin: PinId,
    actual_dma: Option<DmaRoute>,
    expected_pin: PinId,
    expected_dma: DmaRoute,
) -> Result<()> {
    if actual_pin != expected_pin || actual_dma != Some(expected_dma) {
        bail!(
            "serial endpoint `{endpoint}` {signal} route {:?}/{actual_dma:?} is unsupported; expected {:?}/{expected_dma:?}",
            actual_pin,
            expected_pin
        );
    }
    Ok(())
}

fn render_initialization(app: &ResolvedApp) -> Result<String> {
    let mut lines = Vec::new();
    let clock = match app.board.mcu.clock.source {
        ClockSource::Hsi => "ferrowasp_stm32f4::clocks::freeze_hsi(\n    cx.device.RCC.constrain(),\n    SYSTEM_CLOCK_HZ,\n    false,\n)".to_owned(),
        ClockSource::Hse { frequency_hz } => format!(
            "ferrowasp_stm32f4::clocks::freeze_hse(\n    cx.device.RCC.constrain(),\n    {},\n    SYSTEM_CLOCK_HZ,\n    false,\n)",
            render_u32(frequency_hz)
        ),
    };
    lines.push(format!("let mut rcc = {clock};"));
    lines.push("Mono::start(cx.core.SYST, SYSTEM_CLOCK_HZ);".to_owned());

    let mut ports = BTreeSet::new();
    for resource in &app.gpio_resources {
        ports.insert(resource.hardware.pin.port);
    }
    for endpoint in &app.serial_endpoints {
        if let Some(rx) = endpoint.hardware.port.rx {
            ports.insert(rx.pin.port);
        }
        if let Some(tx) = endpoint.hardware.port.tx {
            ports.insert(tx.pin.port);
        }
    }
    for port in ports {
        let letter = port_letter(port)?;
        let lower = letter.to_ascii_lowercase();
        lines.push(format!(
            "let gpio{lower} = cx.device.GPIO{letter}.split(&mut rcc);"
        ));
    }

    let has_exti = app.gpio_resources.iter().any(|resource| {
        matches!(
            resource.hardware.mode,
            GpioMode::Input { interrupt: Some(_) }
        )
    });
    if has_exti {
        lines.push("let mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);".to_owned());
        lines.push("let mut exti = cx.device.EXTI;".to_owned());
    }

    for resource in &app.gpio_resources {
        lines.push(render_gpio_initialization(resource.hardware)?);
    }
    if !app.serial_endpoints.is_empty() {
        lines.push("let dma1 = StreamsTuple::new(cx.device.DMA1, &mut rcc);".to_owned());
    }
    for endpoint in &app.serial_endpoints {
        lines.push(render_endpoint_initialization(endpoint)?);
    }
    Ok(lines.join("\n\n"))
}

fn render_gpio_initialization(
    gpio: &crate::hardware_definitions::stm32f4::gpio::GpioHardwareDeclaration,
) -> Result<String> {
    let port = port_letter(gpio.pin.port)?.to_ascii_lowercase();
    let pin = gpio.pin.pin;
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
            let level = match initial_level {
                Level::Low => "Low",
                Level::High => "High",
            };
            Ok(format!(
                "let mut {id} = gpio{port}.p{port}{pin}.into_push_pull_output_in_state(PinState::{level});\n{id}.set_internal_resistor(Pull::{pull});\n{id}.set_speed(Speed::Low);",
                id = gpio.id
            ))
        }
        GpioMode::Input { interrupt } => {
            let id = gpio.id;
            let mut rendered =
                format!("let {id} = Input::new(gpio{port}.p{port}{pin}, Pull::{pull});");
            if let Some(interrupt) = interrupt {
                let edge = match interrupt.edge {
                    InterruptEdge::Rising => "Rising",
                    InterruptEdge::Falling => "Falling",
                    InterruptEdge::Both => "RisingFalling",
                };
                rendered.push_str(&format!(
                    "\nlet {id} = ferrowasp_stm32f4::exti::init_input(\n    {id},\n    &mut syscfg,\n    &mut exti,\n    Edge::{edge},\n);"
                ));
            }
            Ok(rendered)
        }
    }
}

fn render_endpoint_initialization(endpoint: &ResolvedSerialEndpoint) -> Result<String> {
    validate_supported_endpoint(endpoint)?;
    let id = endpoint.declaration.id;
    let rx_buffers = endpoint.resource_id(SerialEndpointResourceRole::RxBuffers)?;
    let rx_free_queue = endpoint.resource_id(SerialEndpointResourceRole::RxFreeQueue)?;
    let rx_filled_queue = endpoint.resource_id(SerialEndpointResourceRole::RxFilledQueue)?;
    let rx_channel = endpoint.resource_id(SerialEndpointResourceRole::RxChannel)?;
    let rx_service = endpoint.resource_id(SerialEndpointResourceRole::RxService)?;
    let rx_parser = endpoint.resource_id(SerialEndpointResourceRole::RxParser)?;
    let rx_producer = endpoint.resource_id(SerialEndpointResourceRole::RxProducer)?;
    let rx_reader = endpoint.resource_id(SerialEndpointResourceRole::RxReader)?;
    let rx_discontinuities = endpoint.resource_id(SerialEndpointResourceRole::RxDiscontinuities)?;

    let profile = serial_protocol_variant(endpoint.declaration.profile.protocol)?;
    let mut rendered = if matches!(
        endpoint.declaration.direction,
        SerialEndpointDirection::Bidirectional
    ) {
        let tx_buffer = endpoint.resource_id(SerialEndpointResourceRole::TxBuffer)?;
        format!(
            "let {id}_parts = ferrowasp_stm32f4::uart_dma::init_uart4_msp_osd(\n    Uart4MspResources {{\n        tx_pin: gpioa.pa0,\n        rx_pin: gpioa.pa1,\n        uart: cx.device.UART4,\n        rx_dma: dma1.2,\n        tx_dma: dma1.4,\n    }},\n    &mut rcc,\n    UartRxStorageResources {{\n        buffers: cx.local.{rx_buffers},\n        free_queue: cx.local.{rx_free_queue},\n        filled_queue: cx.local.{rx_filled_queue},\n    }},\n    cx.local.{tx_buffer},\n);"
        )
    } else {
        format!(
            "let {id}_parts = ferrowasp_stm32f4::uart_dma::init_uart4_rx_only(\n    ferrowasp_stm32f4::uart_dma::Uart4RxOnlyResources {{\n        rx_pin: gpioa.pa1,\n        uart: cx.device.UART4,\n        rx_dma: dma1.2,\n    }},\n    &mut rcc,\n    SerialProtocol::{profile},\n    UartRxStorageResources {{\n        buffers: cx.local.{rx_buffers},\n        free_queue: cx.local.{rx_free_queue},\n        filled_queue: cx.local.{rx_filled_queue},\n    }},\n);"
        )
    };

    let rx_parts = if matches!(
        endpoint.declaration.direction,
        SerialEndpointDirection::Bidirectional
    ) {
        "rx_irq"
    } else {
        "irq"
    };
    rendered.push_str(&format!(
        "\nlet {rx_service} = {id}_parts.{rx_parts};\nlet {rx_parser} = {id}_parts.parser;\nlet ({rx_producer}, {rx_reader}, {rx_discontinuities}) =\n    cx.local.{rx_channel}.split();\n// RX parser and producer remain separate until the endpoint bridge task is added."
    ));

    if matches!(
        endpoint.declaration.direction,
        SerialEndpointDirection::Bidirectional
    ) {
        let tx_dma = endpoint.resource_id(SerialEndpointResourceRole::TxDma)?;
        let tx_channel = endpoint.resource_id(SerialEndpointResourceRole::TxChannel)?;
        let tx_writer = endpoint.resource_id(SerialEndpointResourceRole::TxWriter)?;
        let tx_owner = endpoint.resource_id(SerialEndpointResourceRole::TxOwner)?;
        let tx_completion = endpoint.resource_id(SerialEndpointResourceRole::TxCompletion)?;
        rendered.push_str(&format!(
            "\nlet {tx_dma} = {id}_parts.tx_dma;\nlet ({tx_writer}, {tx_owner}, {tx_completion}) =\n    cx.local.{tx_channel}.split();"
        ));
    }
    Ok(rendered)
}

fn serial_protocol_variant(protocol: SerialProtocol) -> Result<&'static str> {
    match protocol {
        SerialProtocol::Disabled => bail!("disabled serial profile reached STM32F4 lowering"),
        SerialProtocol::Raw => Ok("Raw"),
        SerialProtocol::Sbus => Ok("Sbus"),
        SerialProtocol::Crsf => Ok("Crsf"),
        SerialProtocol::Mavlink => Ok("Mavlink"),
        SerialProtocol::Msp => Ok("Msp"),
        SerialProtocol::EscTelemetry => Ok("EscTelemetry"),
    }
}

fn port_letter(port: GpioPort) -> Result<char> {
    match port {
        GpioPort::A => Ok('A'),
        GpioPort::B => Ok('B'),
        GpioPort::C => Ok('C'),
        GpioPort::D => Ok('D'),
        GpioPort::E => Ok('E'),
        GpioPort::F => Ok('F'),
        GpioPort::G => Ok('G'),
        GpioPort::H => Ok('H'),
        GpioPort::I => Ok('I'),
        GpioPort::J | GpioPort::K => {
            bail!("STM32F405 lowering does not support GPIO{port:?}")
        }
    }
}

fn render_u32(value: u32) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rtic::resolve, target::app_composition::APP_COMPOSITION};

    #[test]
    fn current_board_lowers_clock_gpio_and_uart4() {
        let app = resolve::resolve(&APP_COMPOSITION).unwrap();
        let rendered = lower(&app).unwrap();

        assert!(rendered.timing.contains("168_000_000"));
        assert!(rendered.initialization.contains("gpioa.pa5"));
        assert!(rendered.initialization.contains("gpioc.pc13"));
        assert!(rendered.initialization.contains("init_uart4_msp_osd"));
        assert!(
            rendered
                .shared_fields
                .iter()
                .any(|field| field.contains("osd_uart_tx_dma"))
        );
        assert!(
            rendered
                .local_fields
                .iter()
                .any(|field| field == "osd_uart_tx_owner: UartOwnedTxOwner<'static>,")
        );
    }
}
