//! STM32F4-specific lowering for the resolved application graph.

use std::collections::BTreeSet;

use crate::{
    backends::stm32f4::{
        dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
        dshot::DshotTimerChannel,
        dshot_actuator,
        endpoints::imu::{ImuEndpointResourceOwnership, ImuEndpointResourceRole},
        endpoints::serial::{SerialEndpointResourceOwnership, SerialEndpointResourceRole},
        gpio::{Drive, GpioMode, InterruptEdge, Level, Pull},
        mcu::{ClockSource, Mcu},
        periodic_control::PeriodicControlResourceRole,
        pins::{GpioPort, PinId},
        serial::SerialPeripheral,
        spi::SpiPeripheral,
        timer::TimerPeripheral,
    },
    input_catalog::foxeer_golden_services as golden_services,
    rtic::{
        composition::SharedValue,
        platform_config::{PlatformConfig, SerialPort, SerialService, SpiPort, SpiService},
        resolve::{
            ResolvedApp, ResolvedGoldenServices, ResolvedImuEndpoint, ResolvedPeriodicControl,
            ResolvedSerialEndpoint,
        },
        state::TaskStateRecipe,
        timing::MonotonicDeclaration,
    },
};
use anyhow::{Context, Result, bail};

/// One static resource declared by RTIC's `#[init(local = [...])]` attribute.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedInitLocal {
    pub(crate) section: String,
    pub(crate) id: String,
    pub(crate) rust_type: String,
    pub(crate) initializer: String,
}

/// Concrete STM32F4 fragments consumed by the generic source renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedHardware {
    pub(crate) imports: String,
    pub(crate) globals: String,
    pub(crate) platform_config: String,
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
        let consumers = app
            .tasks
            .iter()
            .filter(|task| {
                task.local
                    .iter()
                    .chain(&task.shared)
                    .any(|binding| binding.target == resource.id)
            })
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let guard = section_guard(&format!(
            "Application resource `{}`; tasks: {consumers}",
            resource.id
        ));
        shared_fields.push(guard.clone());
        shared_values.push(guard);
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
        let guard = section_guard(&format!(
            "Task `{}` board GPIO resources",
            resource.owner_task
        ));
        local_fields.push(guard.clone());
        local_values.push(guard);
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

    let mut state_owners = BTreeSet::new();
    for state in &app.task_state {
        if state_owners.insert(state.owner_task) {
            let guard = section_guard(&format!(
                "Application-owned state for task `{}`",
                state.owner_task
            ));
            local_fields.push(guard.clone());
            local_values.push(guard);
        }
        local_fields.push(format!("{}: {},", state.id, task_state_type(state.recipe)));
        local_values.push(state.id.to_owned());
    }

    let mut init_locals = Vec::new();
    for endpoint in &app.serial_endpoints {
        validate_supported_endpoint(endpoint)?;
        let section = format!("Serial endpoint `{}` resources", endpoint.declaration.id);
        if endpoint
            .resources
            .iter()
            .any(|resource| resource.ownership == SerialEndpointResourceOwnership::Local)
        {
            let guard = section_guard(&section);
            local_fields.push(guard.clone());
            local_values.push(guard);
        }
        if endpoint
            .resources
            .iter()
            .any(|resource| resource.ownership == SerialEndpointResourceOwnership::Shared)
        {
            let guard = section_guard(&section);
            shared_fields.push(guard.clone());
            shared_values.push(guard);
        }
        for resource in &endpoint.resources {
            match resource.ownership {
                SerialEndpointResourceOwnership::InitLocal => {
                    let (rust_type, initializer) = endpoint_init_local(resource.role)?;
                    init_locals.push(RenderedInitLocal {
                        section: section.clone(),
                        id: resource.id.clone(),
                        rust_type: rust_type.to_owned(),
                        initializer: initializer.to_owned(),
                    });
                }
                SerialEndpointResourceOwnership::Local => {
                    if matches!(
                        resource.role,
                        SerialEndpointResourceRole::RxReader
                            | SerialEndpointResourceRole::RxDiscontinuities
                            | SerialEndpointResourceRole::TxWriter
                    ) {
                        continue;
                    }
                    local_fields.push(format!(
                        "{}: {},",
                        resource.id,
                        endpoint_resource_type(endpoint, resource.role)?
                    ));
                    local_values.push(resource.id.clone());
                }
                SerialEndpointResourceOwnership::Shared => {
                    shared_fields.push(format!(
                        "{}: {},",
                        resource.id,
                        endpoint_resource_type(endpoint, resource.role)?
                    ));
                    shared_values.push(resource.id.clone());
                }
            }
        }
    }
    for assignment in app.platform.serial.into_iter().flatten() {
        let service = assignment.service;
        let guard = section_guard(&format!(
            "Serial service `{:?}` boot-routed resources",
            service
        ));
        local_fields.push(guard.clone());
        local_values.push(guard);
        local_fields.push(format!(
            "{}: UartOwnedReader<'static>,",
            service.reader_resource()
        ));
        local_values.push(service.reader_resource().to_owned());
        local_fields.push(format!(
            "{}: UartOwnedDiscontinuities<'static>,",
            service.discontinuities_resource()
        ));
        local_values.push(service.discontinuities_resource().to_owned());
        if let Some(writer) = service.writer_resource() {
            local_fields.push(format!("{writer}: UartOwnedWriter<'static>,"));
            local_values.push(writer.to_owned());
        }
    }
    for endpoint in &app.imu_endpoints {
        validate_supported_imu_endpoint(endpoint)?;
        let section = format!("SPI endpoint `{}` resources", endpoint.declaration.id);
        if endpoint
            .resources
            .iter()
            .any(|resource| resource.ownership == ImuEndpointResourceOwnership::Local)
        {
            let guard = section_guard(&section);
            local_fields.push(guard.clone());
            local_values.push(guard);
        }
        if endpoint
            .resources
            .iter()
            .any(|resource| resource.ownership == ImuEndpointResourceOwnership::Shared)
        {
            let guard = section_guard(&section);
            shared_fields.push(guard.clone());
            shared_values.push(guard);
        }
        for resource in &endpoint.resources {
            match resource.ownership {
                ImuEndpointResourceOwnership::InitLocal => {
                    let (rust_type, initializer) = imu_init_local(resource.role)?;
                    init_locals.push(RenderedInitLocal {
                        section: section.clone(),
                        id: resource.id.clone(),
                        rust_type: rust_type.to_owned(),
                        initializer: initializer.to_owned(),
                    });
                }
                ImuEndpointResourceOwnership::Local => {
                    local_fields.push(format!(
                        "{}: {},",
                        resource.id,
                        imu_resource_type(resource.role)?
                    ));
                    local_values.push(resource.id.clone());
                }
                ImuEndpointResourceOwnership::Shared => {
                    shared_fields.push(format!(
                        "{}: {},",
                        resource.id,
                        imu_resource_type(resource.role)?
                    ));
                    shared_values.push(resource.id.clone());
                }
            }
        }
    }
    for control in &app.periodic_controls {
        let guard = section_guard(&format!(
            "Periodic control `{}` task-local resources",
            control.declaration.id
        ));
        local_fields.push(guard.clone());
        local_values.push(guard);
        for resource in &control.resources {
            let rust_type = match resource.role {
                PeriodicControlResourceRole::Scheduler => {
                    periodic_control_scheduler_type(control.hardware.peripheral)
                }
                PeriodicControlResourceRole::Phase => "u32".to_owned(),
            };
            local_fields.push(format!("{}: {rust_type},", resource.id));
            local_values.push(resource.id.clone());
        }
    }

    if !app.dshot_actuators.is_empty() {
        if app.dshot_actuators.len() != 1 {
            bail!("the STM32F4 renderer supports exactly one physical DShot actuator");
        }
        let section = "Physical DShot actuator resources".to_owned();
        let guard = section_guard(&section);
        shared_fields.push(guard.clone());
        shared_values.push(guard.clone());
        local_fields.push(guard.clone());
        local_values.push(guard);

        for (id, rust_type) in [
            (dshot_actuator::DSHOT_BANK_RESOURCE, "DshotMotorBank"),
            (dshot_actuator::ESC_UART_IRQ_RESOURCE, "Uart1RxIrq"),
            (dshot_actuator::ESC_DISCONTINUITY_RESOURCE, "bool"),
        ] {
            shared_fields.push(format!("{id}: {rust_type},"));
            shared_values.push(id.to_owned());
        }
        for (id, rust_type) in [
            (dshot_actuator::ESC_UART_PARSER_RESOURCE, "UartRxParserSide"),
            (
                dshot_actuator::ESC_REQUEST_PRODUCER_RESOURCE,
                "EscRequestProducer",
            ),
            (
                dshot_actuator::ESC_REQUEST_CONSUMER_RESOURCE,
                "EscRequestConsumer",
            ),
            (dshot_actuator::ESC_ACK_PRODUCER_RESOURCE, "EscAckProducer"),
            (dshot_actuator::ESC_ACK_CONSUMER_RESOURCE, "EscAckConsumer"),
            (
                dshot_actuator::ESC_UPDATE_PRODUCER_RESOURCE,
                "EscTelemetryUpdateProducer",
            ),
            (
                dshot_actuator::ESC_UPDATE_CONSUMER_RESOURCE,
                "EscTelemetryUpdateConsumer",
            ),
            (dshot_actuator::ESC_MANAGER_STATE_RESOURCE, "EscManager"),
            (dshot_actuator::ESC_MANAGER_REPORT_TICKS_RESOURCE, "u16"),
            (
                dshot_actuator::DSHOT_PENDING_REQUEST_RESOURCE,
                "Option<EscActuatorRequest>",
            ),
            (dshot_actuator::DSHOT_REQUEST_SUBMITTED_RESOURCE, "bool"),
            (dshot_actuator::DSHOT_FAULT_REPORTED_RESOURCE, "bool"),
        ] {
            local_fields.push(format!("{id}: {rust_type},"));
            local_values.push(id.to_owned());
        }
        for (id, rust_type, initializer) in [
            (
                "dshot_dma_storage",
                "DshotDmaStorage",
                "DshotDmaStorage::new()",
            ),
            (
                "esc_uart_rx_buffers",
                "UartRxBufferBank",
                "ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank()",
            ),
            (
                "esc_uart_free_queue",
                "UartRxFreeQueue",
                "UartRxFreeQueue::new()",
            ),
            (
                "esc_uart_filled_queue",
                "UartRxFilledQueue",
                "UartRxFilledQueue::new()",
            ),
            (
                "esc_request_queue",
                "EscRequestQueue",
                "EscRequestQueue::new()",
            ),
            ("esc_ack_queue", "EscAckQueue", "EscAckQueue::new()"),
            (
                "esc_update_queue",
                "EscTelemetryUpdateQueue",
                "EscTelemetryUpdateQueue::new()",
            ),
        ] {
            init_locals.push(RenderedInitLocal {
                section: section.clone(),
                id: id.to_owned(),
                rust_type: rust_type.to_owned(),
                initializer: initializer.to_owned(),
            });
        }
    }

    if !app.golden_services.is_empty() {
        if app.golden_services.len() != 1 {
            bail!("the STM32F4 renderer supports exactly one golden service suite");
        }
        let guard = section_guard("Golden ADC observation resources");
        shared_fields.push(guard.clone());
        shared_values.push(guard.clone());
        local_fields.push(guard.clone());
        local_values.push(guard);

        for (id, rust_type) in [
            (golden_services::ADC_TRANSFER, "Adc1ObservationTransfer"),
            (golden_services::SERVICE_TELEMETRY, "FlightServiceTelemetry"),
            (golden_services::TUNING_PROFILE, "dt::TuningProfile"),
            (golden_services::TUNING_REQUEST_SEQ, "u32"),
            (golden_services::LOG_RATE_DIVISOR, "u32"),
            (golden_services::STORAGE_STATUS, "StorageStatus"),
            (golden_services::USB_STATUS_DUE, "bool"),
        ] {
            shared_fields.push(format!("{id}: {rust_type},"));
            shared_values.push(id.to_owned());
        }
        for (id, rust_type) in [
            (golden_services::ADC_BUFFER, "Option<&'static mut [u16; 3]>"),
            (golden_services::ADC_PLANNER, "AdcDmaIrqPlanner"),
            (
                golden_services::BATTERY_CELL_DETECTOR,
                "BatteryCellDetector",
            ),
            (golden_services::OSD_TASK, "OsdTask"),
            (
                golden_services::OSD_BUFFER,
                "[u8; ferrowasp_mspv1::OSD_TX_BUFFER_LEN]",
            ),
            (golden_services::OSD_REFRESH_TICK, "u8"),
            (golden_services::OSD_TX_HEALTHY, "bool"),
            (golden_services::FLASH_DEVICE, "Spi2Flash"),
            (golden_services::FLASH_RECORD_PRODUCER, "RecordProducer"),
            (golden_services::FLASH_RECORD_CONSUMER, "RecordConsumer"),
            (golden_services::FLASH_COMMAND_PRODUCER, "CommandProducer"),
            (golden_services::FLASH_COMMAND_CONSUMER, "CommandConsumer"),
            (golden_services::FLASH_RESPONSE_PRODUCER, "ResponseProducer"),
            (golden_services::FLASH_RESPONSE_CONSUMER, "ResponseConsumer"),
            (golden_services::FLASH_MANAGER_STATE, "GoldenFlashState"),
            (golden_services::USB_DEVICE, "UsbCdcDevice"),
            (golden_services::USB_SERIAL, "BufferedUsbCdcSerial"),
            (golden_services::USB_HEADER_SENT, "bool"),
            (golden_services::USB_COMMAND_PARSER, "CommandParser"),
            (
                golden_services::USB_PENDING_RESPONSE,
                "Option<ResponseFrame>",
            ),
            (golden_services::IO_WATCHDOG, "IoWatchdog"),
        ] {
            local_fields.push(format!("{id}: {rust_type},"));
            local_values.push(id.to_owned());
        }
        init_locals.push(RenderedInitLocal {
            section: "Golden ADC observation resources".to_owned(),
            id: "adc1_buffers".to_owned(),
            rust_type: "AdcBufferBank".to_owned(),
            initializer: "ferrowasp_stm32f4::app_storage::new_adc_buffer_bank()".to_owned(),
        });
        for (id, rust_type, initializer) in [
            ("flash_record_queue", "RecordQueue", "RecordQueue::new()"),
            ("flash_command_queue", "CommandQueue", "CommandQueue::new()"),
            (
                "flash_response_queue",
                "ResponseQueue",
                "ResponseQueue::new()",
            ),
        ] {
            init_locals.push(RenderedInitLocal {
                section: "Golden SPI2 NOR and USB CDC resources".to_owned(),
                id: id.to_owned(),
                rust_type: rust_type.to_owned(),
                initializer: initializer.to_owned(),
            });
        }
    }

    let initialization = render_initialization(app)?;
    let system_clock_hz = app.board.mcu.clock.system_frequency_hz;
    let monotonic = match app.monotonic {
        MonotonicDeclaration::SysTick { tick_hz } => {
            format!("systick_monotonic!(Mono, {});", render_u32(tick_hz))
        }
        MonotonicDeclaration::Timer {
            hardware_id,
            tick_hz,
        } => match timer_peripheral(app, hardware_id)? {
            TimerPeripheral::Tim2 => {
                format!("stm32_tim2_monotonic!(Mono, {});", render_u32(tick_hz))
            }
            unsupported => {
                bail!("the STM32F4 renderer does not support {unsupported:?} as the RTIC monotonic")
            }
        },
    };
    let timing = format!(
        "const SYSTEM_CLOCK_HZ: u32 = {};\n\n{monotonic}",
        render_u32(system_clock_hz),
    );

    Ok(RenderedHardware {
        imports: render_imports(app),
        globals: render_globals(app)?,
        platform_config: render_platform_config(app.platform),
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
    let has_tx = !app.serial_endpoints.is_empty();
    let tx_imports = if has_tx {
        "\n    memory::{UartOwnedTxChannel, UartOwnedTxCompletion, UartOwnedTxOwner, UartOwnedWriter},\n    serial::{UartTxDmaService, UartTxIrqOutcome, UartTxStartError},"
    } else {
        ""
    };
    let fault_import = if has_tx { "SerialFault" } else { "" };
    let mut imports = format!(
        "pub(crate) use core::fmt::Write as FmtWrite;\n\
pub(crate) use embedded_io_async::{{Read, Write}};\n\
pub(crate) use ferrowasp_core::frames::FrameRotation;\n\
pub(crate) use ferrowasp_io_core::{{\n    platform_config::{{ImuInstallationId, RuntimePlatformConfig, SerialService, SpiService}},\n    serial::{{{fault_import}}},\n    spi::SpiDeviceError,\n}};\n\
pub(crate) use ferrowasp_stm32f4::{{\n    app_storage::{{\n        UartRxBufferBank, UartRxFilledQueue, UartRxFreeQueue, UartRxStorageResources,\n        UartTxBuffer,\n    }},\n    memory::{{\n        UartOwnedDiscontinuities, UartOwnedReader, UartOwnedRxChannel,\n    }},{tx_imports}\n    rtic::prelude::*,\n    serial::{{UartOwnedRxBridgeOutcome, UartOwnedRxBridgeService, UartRxIrqOutcome, UartRxIrqService}},\n    uart_dma::{{\n        UART_TX_BUFFER_SIZE, Uart2RxIrq, Uart2TxDmaSide, Uart4EndpointResources,\n        Uart4RxIrq, Uart4TxDmaSide, UartOwnedRxBridge, Usart2EndpointResources,\n    }},\n}};"
    );
    if !app.imu_endpoints.is_empty() {
        imports.push_str(
            "\npub(crate) use ferrowasp_drivers::mpu6500::ImuData;\n\
pub(crate) use ferrowasp_stm32f4::{\n\
    app_storage::{SpiDmaBufferBank, SpiDmaStorageResources, SpiFilledQueue, SpiFreeQueue},\n\
    memory, spi_dma,\n\
    spi_imu_endpoint::{\n\
        IMU_KIND_ICM42688P, IMU_KIND_MPU6500, IMU_KIND_NONE, Spi1ImuDevice, Spi1ImuEndpointOwner,\n\
        Spi1ImuMailbox, Spi1ImuParser, SpiImuOwnerOutcome,\n\
        SpiImuRxOutcome, SpiImuTimeoutOutcome, new_spi1_imu_mailbox,\n\
    },\n\
};\n\
pub(crate) use ferrowasp_stm32f4::rtic::hal::spi;",
        );
    }
    if !app.periodic_controls.is_empty() {
        imports.push_str(
            "\npub(crate) use ferrowasp_stm32f4::scheduler::{acknowledge_control_tick, init_control_scheduler};",
        );
    }
    if !app.dshot_actuators.is_empty() {
        imports.push_str(
            "\npub(crate) use ferrowasp_stm32f4::{\n\
    dshot::{\n\
        DshotDmaStorage, DshotInterruptEvent, DshotMotor, DshotMotorBank, DshotServiceEvent,\n\
        DshotTelemetryRequestError,\n\
    },\n\
    uart_dma::{\n\
        Uart1RxIrq, UartRxParserSide, UartRxReadStatus, Usart1EscTelemetryResources,\n\
    },\n\
};\n\
pub(crate) use ferrowasp_tasks::esc_manager::{\n\
    DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_IDLE_THROTTLE_COMMAND,\n\
    DSHOT_PREARM_STOP_HOLD_MS, EscIdleQualification, EscIdleQualificationFailure,\n\
    EscIdleQualificationStatus,\n\
    EscAckConsumer, EscAckOutcome, EscAckProducer, EscAckQueue, EscActuatorAck,\n\
    EscActuatorRequest, EscManager, EscManagerConfig, EscOutput, EscRequestConsumer,\n\
    EscRequestProducer, EscRequestQueue, EscTelemetryUpdateConsumer,\n\
    EscTelemetryUpdateProducer, EscTelemetryUpdateQueue,\n\
};",
        );
    }
    if !app.golden_services.is_empty() {
        imports.push_str(
            r#"
pub(crate) use ferrowasp_stm32f4::{
    adc::{AdcDmaDeliveryError, AdcDmaIrqPlanner, take_completed_adc1_sample_for},
    app_storage::{AdcBufferBank, AdcStorageResources},
    usb_serial::{
        BufferedUsbCdcSerial, UsbCdcDevice, UsbCdcIdentity, UsbDeviceState,
    },
    watchdog::acknowledge_watchdog_tick,
};
pub(crate) use ferrowasp_tasks::{
    flash_storage::{
        CommandConsumer, CommandParser, CommandProducer, CommandQueue, GoldenFlashOperation,
        GoldenFlashState, RecordConsumer, RecordEnqueueOutcome, RecordProducer, RecordQueue, ResponseConsumer,
        ResponseFrame, ResponseProducer, ResponseQueue, StorageCommand, StorageLayout,
        emit_page_hex_lines, format_log_info_response, load_config, scan_log,
    },
    osd::{BatteryCellDetector, OsdStickRates, OsdTask},
    service_telemetry::{FlightServiceTelemetry, StorageStatus},
    usb_debug::{self, ImuKind, StatusSnapshot},
};
pub(crate) type Adc1ObservationTransfer = ferrowasp_stm32f4::adc::Adc1ObservationTransferFor<
    ferrowasp_stm32f4::rtic::hal::dma::Stream4<ferrowasp_stm32f4::rtic::hal::pac::DMA2>,
    0,
>;
pub(crate) type Spi2Flash = ferrowasp_drivers::spi_nor::SpiNor<
    ferrowasp_stm32f4::rtic::hal::spi::Spi<ferrowasp_stm32f4::rtic::hal::pac::SPI2>,
    ferrowasp_stm32f4::rtic::hal::gpio::Pin<
        'B',
        12,
        ferrowasp_stm32f4::rtic::hal::gpio::Output<
            ferrowasp_stm32f4::rtic::hal::gpio::PushPull,
        >,
    >,
>;
pub(crate) type IoWatchdog = ferrowasp_stm32f4::rtic::hal::timer::CounterHz<
    ferrowasp_stm32f4::rtic::hal::pac::TIM6,
>;"#,
        );
    }
    if !app.task_state.is_empty() {
        imports.push_str("\npub(crate) use ferrowasp_tasks::drone_toolbox as dt;");
    }
    imports
}

fn render_globals(app: &ResolvedApp) -> Result<String> {
    if app.imu_endpoints.is_empty() {
        return Ok(String::new());
    }
    if app.imu_endpoints.len() != 1 {
        bail!("the initial STM32F4 renderer supports one SPI1 IMU endpoint");
    }
    let endpoint = &app.imu_endpoints[0];
    validate_supported_imu_endpoint(endpoint)?;
    let mailbox = format!("{}_MAILBOX", endpoint.declaration.id.to_ascii_uppercase());

    Ok(format!(
        "{}\nstatic {mailbox}: Spi1ImuMailbox = new_spi1_imu_mailbox();",
        section_guard(&format!(
            "SPI endpoint `{}` static resources",
            endpoint.declaration.id
        ))
    ))
}

fn render_platform_config(config: PlatformConfig) -> String {
    let serial1 = config
        .serial
        .into_iter()
        .flatten()
        .find(|assignment| assignment.port == SerialPort::Serial1)
        .map_or("None", |assignment| match assignment.service {
            SerialService::RcSbus => "Some(SerialService::RcSbus)",
            SerialService::MspV1Osd => "Some(SerialService::MspV1Osd)",
        });
    let serial2 = config
        .serial
        .into_iter()
        .flatten()
        .find(|assignment| assignment.port == SerialPort::Serial2)
        .map_or("None", |assignment| match assignment.service {
            SerialService::RcSbus => "Some(SerialService::RcSbus)",
            SerialService::MspV1Osd => "Some(SerialService::MspV1Osd)",
        });
    let spi1 = config
        .spi
        .into_iter()
        .flatten()
        .find(|assignment| assignment.port == SpiPort::Spi1)
        .map_or_else(
            || "None".to_owned(),
            |assignment| match assignment.service {
                SpiService::Imu(id) => format!(
                    "Some(SpiService::Imu(ImuInstallationId::new({})))",
                    id.get()
                ),
            },
        );

    format!(
        r#"pub(crate) fn load_platform_config() -> RuntimePlatformConfig {{
    RuntimePlatformConfig {{
        serial1: {serial1},
        serial2: {serial2},
        spi1: {spi1},
    }}
}}"#,
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
        SerialEndpointResourceRole::TxBuffer => Ok(("UartTxBuffer", "[0; UART_TX_BUFFER_SIZE]")),
        SerialEndpointResourceRole::TxChannel => {
            Ok(("UartOwnedTxChannel", "UartOwnedTxChannel::new()"))
        }
        unsupported => bail!("endpoint role {unsupported:?} cannot be initialization-local"),
    }
}

fn endpoint_resource_type(
    endpoint: &ResolvedSerialEndpoint,
    role: SerialEndpointResourceRole,
) -> Result<&'static str> {
    match role {
        SerialEndpointResourceRole::RxService => match endpoint.hardware.port.peripheral {
            SerialPeripheral::Usart2 => Ok("Uart2RxIrq"),
            SerialPeripheral::Uart4 => Ok("Uart4RxIrq"),
            unsupported => bail!("unsupported serial RX service for {unsupported:?}"),
        },
        SerialEndpointResourceRole::RxParser => Ok(
            "UartOwnedRxBridge<'static, { memory::UART_RX_BUFFER_BYTES }, { memory::OWNED_UART_RX_QUEUE_DEPTH }>",
        ),
        SerialEndpointResourceRole::RxReader => Ok("UartOwnedReader<'static>"),
        SerialEndpointResourceRole::RxDiscontinuities => Ok("UartOwnedDiscontinuities<'static>"),
        SerialEndpointResourceRole::TxDma => match endpoint.hardware.port.peripheral {
            SerialPeripheral::Usart2 => Ok("Uart2TxDmaSide"),
            SerialPeripheral::Uart4 => Ok("Uart4TxDmaSide"),
            unsupported => bail!("unsupported serial TX service for {unsupported:?}"),
        },
        SerialEndpointResourceRole::TxWriter => Ok("UartOwnedWriter<'static>"),
        SerialEndpointResourceRole::TxOwner => Ok("UartOwnedTxOwner<'static>"),
        SerialEndpointResourceRole::TxCompletion => Ok("UartOwnedTxCompletion<'static>"),
        unsupported => bail!("endpoint role {unsupported:?} cannot be an RTIC field"),
    }
}

fn imu_init_local(role: ImuEndpointResourceRole) -> Result<(&'static str, &'static str)> {
    match role {
        ImuEndpointResourceRole::Buffers => Ok((
            "SpiDmaBufferBank",
            "ferrowasp_stm32f4::app_storage::new_spi_dma_buffer_bank()",
        )),
        ImuEndpointResourceRole::FreeQueue => Ok(("SpiFreeQueue", "SpiFreeQueue::new()")),
        ImuEndpointResourceRole::FilledQueue => Ok(("SpiFilledQueue", "SpiFilledQueue::new()")),
        unsupported => bail!("IMU endpoint role {unsupported:?} cannot be initialization-local"),
    }
}

fn imu_resource_type(role: ImuEndpointResourceRole) -> Result<&'static str> {
    match role {
        ImuEndpointResourceRole::Owner => Ok("Spi1ImuEndpointOwner"),
        ImuEndpointResourceRole::Parser => Ok("Spi1ImuParser"),
        ImuEndpointResourceRole::Device => Ok("Spi1ImuDevice"),
        ImuEndpointResourceRole::DataReady => Ok("Pin<'C', 4, Input>"),
        ImuEndpointResourceRole::Kind => Ok("u8"),
        ImuEndpointResourceRole::Sample => Ok("ImuData"),
        ImuEndpointResourceRole::UnavailableLogged => Ok("bool"),
        unsupported => bail!("IMU endpoint role {unsupported:?} cannot be an RTIC field"),
    }
}

fn validate_supported_imu_endpoint(endpoint: &ResolvedImuEndpoint) -> Result<()> {
    let expected_rx = DmaRoute::new(
        DmaController::Dma2,
        DmaStream::Stream0,
        DmaChannel::Channel3,
    );
    let expected_tx = DmaRoute::new(
        DmaController::Dma2,
        DmaStream::Stream3,
        DmaChannel::Channel3,
    );
    let bus = endpoint.spi.bus;
    if bus.peripheral != SpiPeripheral::Spi1
        || bus.pins.sck != PinId::new(GpioPort::A, 5)
        || bus.pins.miso != PinId::new(GpioPort::A, 6)
        || bus.pins.mosi != PinId::new(GpioPort::A, 7)
        || bus.dma.rx != expected_rx
        || bus.dma.tx != expected_tx
    {
        bail!(
            "IMU endpoint `{}` uses an unsupported SPI route; expected Foxeer SPI1 PA5/PA6/PA7 with DMA2 streams 0/3 channel 3",
            endpoint.declaration.id
        );
    }
    if endpoint.hardware.chip_select != PinId::new(GpioPort::A, 4)
        || endpoint.hardware.data_ready != PinId::new(GpioPort::C, 4)
        || endpoint.hardware.data_ready_edge != InterruptEdge::Rising
    {
        bail!(
            "IMU endpoint `{}` must use Foxeer CS PA4 and DRDY PC4 rising",
            endpoint.declaration.id
        );
    }
    Ok(())
}

fn validate_supported_endpoint(endpoint: &ResolvedSerialEndpoint) -> Result<()> {
    let rx = endpoint
        .hardware
        .port
        .rx
        .context("validated serial endpoint has no RX route")?;
    let tx = endpoint
        .hardware
        .port
        .tx
        .context("validated serial endpoint has no TX route")?;
    match endpoint.hardware.port.peripheral {
        SerialPeripheral::Usart2 => {
            require_route(
                endpoint.declaration.id,
                "RX",
                rx.pin,
                rx.dma,
                PinId::new(GpioPort::A, 3),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream5,
                    DmaChannel::Channel4,
                ),
            )?;
            require_route(
                endpoint.declaration.id,
                "TX",
                tx.pin,
                tx.dma,
                PinId::new(GpioPort::A, 2),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream6,
                    DmaChannel::Channel4,
                ),
            )?;
        }
        SerialPeripheral::Uart4 => {
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
        unsupported => bail!(
            "serial endpoint `{}` uses unsupported peripheral {unsupported:?}",
            endpoint.declaration.id
        ),
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
    lines.push(format!(
        "{}\nlet platform_config = load_platform_config();",
        section_guard("Common initialization")
    ));
    let clock = match app.board.mcu.clock.source {
        ClockSource::Hsi => "ferrowasp_stm32f4::clocks::freeze_hsi(\n    cx.device.RCC.constrain(),\n    SYSTEM_CLOCK_HZ,\n    false,\n)".to_owned(),
        ClockSource::Hse { frequency_hz } => format!(
            "ferrowasp_stm32f4::clocks::freeze_hse(\n    cx.device.RCC.constrain(),\n    {},\n    SYSTEM_CLOCK_HZ,\n    false,\n)",
            render_u32(frequency_hz)
        ),
    };
    lines.push(format!("let mut rcc = {clock};"));
    lines.push(match app.monotonic {
        MonotonicDeclaration::SysTick { .. } => {
            "Mono::start(cx.core.SYST, SYSTEM_CLOCK_HZ);".to_owned()
        }
        MonotonicDeclaration::Timer { hardware_id, .. } => {
            match timer_peripheral(app, hardware_id)? {
                TimerPeripheral::Tim2 => "Mono::start(rcc.clocks.timclk1().raw());".to_owned(),
                unsupported => bail!(
                    "the STM32F4 renderer does not support {unsupported:?} as the RTIC monotonic"
                ),
            }
        }
    });
    if let Some(delay) = app.init_delay {
        let peripheral = match timer_peripheral(app, delay.hardware_id)? {
            TimerPeripheral::Tim5 => "TIM5",
            unsupported => bail!(
                "the STM32F4 renderer does not support {unsupported:?} as the initialization delay"
            ),
        };
        lines.push(format!(
            "let mut init_delay = cx.device.{peripheral}.delay::<{}>(&mut rcc);",
            render_u32(delay.tick_hz)
        ));
    }
    for control in &app.periodic_controls {
        lines.push(format!(
            "{}\n{}",
            section_guard(&format!(
                "Periodic control `{}` initialization",
                control.declaration.id
            )),
            render_periodic_control_initialization(control)?
        ));
    }
    if !app.task_state.is_empty() {
        lines.push(render_task_state_initialization(app));
    }

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
    for endpoint in &app.imu_endpoints {
        ports.insert(endpoint.spi.bus.pins.sck.port);
        ports.insert(endpoint.spi.bus.pins.miso.port);
        ports.insert(endpoint.spi.bus.pins.mosi.port);
        ports.insert(endpoint.hardware.chip_select.port);
        ports.insert(endpoint.hardware.data_ready.port);
    }
    for actuator in &app.dshot_actuators {
        for lane in actuator.hardware.lanes {
            ports.insert(lane.pin.port);
        }
        if let Some(rx) = actuator.telemetry.port.rx {
            ports.insert(rx.pin.port);
        }
    }
    for services in &app.golden_services {
        ports.insert(services.adc.voltage_pin.port);
        ports.insert(services.adc.current_pin.port);
        ports.insert(services.flash.chip_select.port);
        ports.insert(services.flash.sck.port);
        ports.insert(services.flash.miso.port);
        ports.insert(services.flash.mosi.port);
        ports.insert(services.usb.dm.port);
        ports.insert(services.usb.dp.port);
    }
    for port in ports {
        let letter = port_letter(port)?;
        let lower = letter.to_ascii_lowercase();
        lines.push(format!(
            "let gpio{lower} = cx.device.GPIO{letter}.split(&mut rcc);"
        ));
    }
    let has_exti = !app.imu_endpoints.is_empty()
        || app.gpio_resources.iter().any(|resource| {
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
        lines.push(format!(
            "{}\n{}",
            section_guard(&format!(
                "Task `{}` board GPIO initialization",
                resource.owner_task
            )),
            render_gpio_initialization(resource.hardware)?
        ));
    }
    if !app.serial_endpoints.is_empty() {
        lines.push("let dma1 = StreamsTuple::new(cx.device.DMA1, &mut rcc);".to_owned());
    }
    if !app.imu_endpoints.is_empty()
        || !app.dshot_actuators.is_empty()
        || !app.golden_services.is_empty()
    {
        lines.push("let dma2 = StreamsTuple::new(cx.device.DMA2, &mut rcc);".to_owned());
    }
    for endpoint in &app.serial_endpoints {
        lines.push(format!(
            "{}\n{}",
            section_guard(&format!(
                "Serial endpoint `{}` initialization",
                endpoint.declaration.id
            )),
            render_endpoint_initialization(endpoint)?
        ));
    }
    if !app.serial_endpoints.is_empty() {
        lines.push(render_serial_service_routing(app)?);
    }
    if !app.dshot_actuators.is_empty() {
        lines.push(render_dshot_actuator_initialization(app)?);
    }
    for endpoint in &app.imu_endpoints {
        lines.push(format!(
            "{}\n{}",
            section_guard(&format!(
                "SPI endpoint `{}` initialization",
                endpoint.declaration.id
            )),
            render_imu_endpoint_initialization(app, endpoint)?
        ));
    }
    for services in &app.golden_services {
        lines.push(format!(
            "{}\n{}",
            section_guard("Golden ADC observation initialization"),
            render_adc_observation_initialization(services)?
        ));
    }
    Ok(lines.join("\n\n"))
}

fn render_adc_observation_initialization(services: &ResolvedGoldenServices) -> Result<String> {
    if services.adc.dma
        != DmaRoute::new(
            DmaController::Dma2,
            DmaStream::Stream4,
            DmaChannel::Channel0,
        )
    {
        bail!("golden ADC lowering requires DMA2 Stream4 Channel0");
    }
    Ok(format!(
        r#"let (adc1_primary_buffer, adc1_spare_buffer) = AdcStorageResources {{
    buffers: cx.local.adc1_buffers,
}}
.split();
let adc1_parts = ferrowasp_stm32f4::adc::init_adc1_observation_for::<_, 0>(
    cx.device.ADC1,
    gpioc.pc0,
    gpioc.pc1,
    dma2.4,
    &mut rcc,
    adc1_primary_buffer,
    adc1_spare_buffer,
);
let adc1_transfer = adc1_parts.transfer;
let adc1_buffer = Some(adc1_parts.spare_buffer);
let adc1_planner = AdcDmaIrqPlanner::new();
let battery_cell_detector = ferrowasp_tasks::osd::BatteryCellDetector::new(
    {},
    {},
    {},
);
let flight_service_telemetry = FlightServiceTelemetry::new();
let tuning_profile = initial_tuning_profile();
let tuning_request_seq = INITIAL_TUNING_REQUEST_SEQ;
let osd_task_state = OsdTask::new();
let osd_tx_buffer = [0; ferrowasp_mspv1::OSD_TX_BUFFER_LEN];
let osd_refresh_tick = 0;
let osd_tx_healthy = true;

let mut flash_cs = gpiob.pb12.into_push_pull_output();
let _ = embedded_hal::digital::OutputPin::set_high(&mut flash_cs);
let flash_mode = ferrowasp_stm32f4::rtic::hal::spi::Mode {{
    polarity: ferrowasp_stm32f4::rtic::hal::spi::Polarity::IdleLow,
    phase: ferrowasp_stm32f4::rtic::hal::spi::Phase::CaptureOnFirstTransition,
}};
let flash_bus = ferrowasp_stm32f4::rtic::hal::spi::Spi::new(
    cx.device.SPI2,
    (
        Some(gpiob.pb13.into_alternate()),
        Some(gpioc.pc2.into_alternate()),
        Some(gpioc.pc3.into_alternate()),
    ),
    flash_mode,
    10_000_000.Hz(),
    &mut rcc,
);
let mut flash_device = ferrowasp_drivers::spi_nor::SpiNor::new(flash_bus, flash_cs);
let storage_status = match flash_device.read_jedec_id() {{
    Ok(id) => {{
        let capacity_bytes = id.capacity_bytes().unwrap_or(0);
        StorageStatus {{
            ready: id.plausible() && StorageLayout::new(capacity_bytes).is_some(),
            jedec: [id.manufacturer, id.memory_type, id.capacity_code],
            capacity_bytes,
            ..StorageStatus::default()
        }}
    }}
    Err(_) => {{
        defmt::warn!("SPI2 flash JEDEC probe failed; storage disabled");
        StorageStatus::default()
    }}
}};
let (flash_record_producer, flash_record_consumer) = cx.local.flash_record_queue.split();
let (flash_command_producer, flash_command_consumer) = cx.local.flash_command_queue.split();
let (flash_response_producer, flash_response_consumer) = cx.local.flash_response_queue.split();
let flash_manager_state = GoldenFlashState::new();
let flash_log_rate_divisor = INITIAL_FLASH_LOG_RATE_DIVISOR;

let (usb_device, usb_serial) = ferrowasp_stm32f4::usb_serial::init_usb_cdc_serial(
    (
        cx.device.OTG_FS_GLOBAL,
        cx.device.OTG_FS_DEVICE,
        cx.device.OTG_FS_PWRCLK,
    ),
    (gpioa.pa11, gpioa.pa12),
    &rcc.clocks,
    UsbCdcIdentity {{
        manufacturer: {:?},
        product: {:?},
        serial_number: {:?},
    }},
)
.expect("validated OTG_FS resources must initialize once");
let usb_header_sent = false;
let usb_command_parser = CommandParser::new();
let usb_pending_response = None;
let usb_status_due = false;
let io_watchdog = ferrowasp_stm32f4::watchdog::init_io_watchdog(
    cx.device.TIM6,
    &mut rcc,
)
.expect("validated TIM6 8 kHz watchdog must start");"#,
        services.declaration.battery_max_cell_mv,
        services.declaration.battery_detect_cell_mv,
        services.declaration.battery_max_cells,
        services.usb.identity.manufacturer,
        services.usb.identity.product,
        services.usb.identity.serial_number,
    ))
}

fn render_serial_service_routing(app: &ResolvedApp) -> Result<String> {
    let has_serial1 = app
        .serial_endpoints
        .iter()
        .any(|endpoint| endpoint.declaration.id == "serial1");
    let has_serial2 = app
        .serial_endpoints
        .iter()
        .any(|endpoint| endpoint.declaration.id == "serial2");
    let has_sbus = app
        .platform
        .serial
        .into_iter()
        .flatten()
        .any(|assignment| assignment.service == SerialService::RcSbus);
    let has_msp = app
        .platform
        .serial
        .into_iter()
        .flatten()
        .any(|assignment| assignment.service == SerialService::MspV1Osd);

    if !(has_serial1 && has_serial2 && has_sbus && has_msp) {
        bail!("the current runtime serial router requires serial1, serial2, RcSbus, and MspV1Osd");
    }

    Ok(format!(
        r#"{}
let (
    rc_sbus_reader,
    rc_sbus_rx_discontinuities,
    msp_v1_osd_reader,
    msp_v1_osd_rx_discontinuities,
    msp_v1_osd_writer,
) =
    match (platform_config.serial1, platform_config.serial2) {{
        (Some(SerialService::RcSbus), Some(SerialService::MspV1Osd)) =>
            (
                serial1_rx_reader,
                serial1_rx_discontinuities,
                serial2_rx_reader,
                serial2_rx_discontinuities,
                serial2_tx_writer,
            ),
        (Some(SerialService::MspV1Osd), Some(SerialService::RcSbus)) =>
            (
                serial2_rx_reader,
                serial2_rx_discontinuities,
                serial1_rx_reader,
                serial1_rx_discontinuities,
                serial1_tx_writer,
            ),
        _ => panic!("boot platform config must assign RcSbus and MspV1Osd to distinct serial endpoints"),
    }};"#,
        section_guard("Boot-time serial service routing")
    ))
}

fn render_dshot_actuator_initialization(app: &ResolvedApp) -> Result<String> {
    let [actuator] = app.dshot_actuators.as_slice() else {
        bail!("the STM32F4 renderer requires exactly one DShot actuator")
    };
    let lanes = actuator.hardware.lanes;
    let expected = [
        (
            DshotTimerChannel::Tim1Ch1,
            PinId::new(GpioPort::A, 8),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream1,
                DmaChannel::Channel6,
            ),
        ),
        (
            DshotTimerChannel::Tim8Ch4,
            PinId::new(GpioPort::C, 9),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream7,
                DmaChannel::Channel7,
            ),
        ),
        (
            DshotTimerChannel::Tim8Ch3,
            PinId::new(GpioPort::C, 8),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream2,
                DmaChannel::Channel0,
            ),
        ),
        (
            DshotTimerChannel::Tim1Ch3N,
            PinId::new(GpioPort::B, 15),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream6,
                DmaChannel::Channel6,
            ),
        ),
    ];
    for (index, (channel, pin, dma)) in expected.into_iter().enumerate() {
        let lane = lanes[index];
        if lane.timer_channel != channel || lane.pin != pin || lane.dma != dma {
            bail!(
                "DShot physical output {} drifted from the reviewed Foxeer route",
                index + 1
            );
        }
    }
    let telemetry_rx = actuator
        .telemetry
        .port
        .rx
        .context("validated ESC telemetry RX route disappeared")?;
    require_route(
        actuator.telemetry.id,
        "RX",
        telemetry_rx.pin,
        telemetry_rx.dma,
        PinId::new(GpioPort::A, 10),
        DmaRoute::new(
            DmaController::Dma2,
            DmaStream::Stream5,
            DmaChannel::Channel4,
        ),
    )?;

    Ok(format!(
        r#"{}
ferrowasp_stm32f4::dshot::assert_foxeer_four_motor_dma_routes_compile();
let esc_uart_parts = ferrowasp_stm32f4::uart_dma::init_usart1_esc_telemetry(
    Usart1EscTelemetryResources {{
        rx_pin: gpioa.pa10,
        usart: cx.device.USART1,
        rx_dma: dma2.5,
    }},
    &mut rcc,
    UartRxStorageResources {{
        buffers: cx.local.esc_uart_rx_buffers,
        free_queue: cx.local.esc_uart_free_queue,
        filled_queue: cx.local.esc_uart_filled_queue,
    }},
);
let uart1_rx = esc_uart_parts.irq;
let esc_telemetry_uart = esc_uart_parts.parser;
let (esc_request_producer, esc_request_consumer) = cx.local.esc_request_queue.split();
let (esc_ack_producer, esc_ack_consumer) = cx.local.esc_ack_queue.split();
let (esc_telemetry_update_producer, esc_telemetry_update_consumer) =
    cx.local.esc_update_queue.split();
let esc_manager_state = EscManager::new(EscManagerConfig::legacy_uart(), 0);
let esc_manager_report_ticks = 0;
let esc_actuator_request = None;
let esc_actuator_request_submitted = false;
let dshot_fault_reported = false;
let esc_telemetry_discontinuity = false;
let dshot_motors = DshotMotorBank::new_foxeer(
    gpioa.pa8,
    gpioc.pc9,
    gpioc.pc8,
    gpiob.pb15,
    ferrowasp_stm32f4::rtic::hal::timer::Timer::new(cx.device.TIM1, &mut rcc),
    ferrowasp_stm32f4::rtic::hal::timer::Timer::new(cx.device.TIM8, &mut rcc),
    dma2.1,
    dma2.7,
    dma2.2,
    dma2.6,
    &rcc.clocks,
    cx.local.dshot_dma_storage,
)
.expect("reviewed Foxeer DShot600 timing must be valid");"#,
        section_guard("Physical DShot actuator initialization")
    ))
}

fn render_gpio_initialization(
    gpio: &crate::backends::stm32f4::gpio::GpioHardwareDeclaration,
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
    let rx_bridge = endpoint.resource_id(SerialEndpointResourceRole::RxParser)?;
    let rx_reader = endpoint.resource_id(SerialEndpointResourceRole::RxReader)?;
    let rx_discontinuities = endpoint.resource_id(SerialEndpointResourceRole::RxDiscontinuities)?;

    let config_field = match id {
        "serial1" => "serial1",
        "serial2" => "serial2",
        unsupported => bail!(
            "serial endpoint `{unsupported}` must use a logical connector ID such as `serial1` or `serial2`"
        ),
    };
    let service = format!(
        "let {id}_service = platform_config.{config_field}\n    .expect(\"boot platform config must assign {id}\");"
    );

    let initialization = match endpoint.hardware.port.peripheral {
        SerialPeripheral::Uart4 => {
            let tx_buffer = endpoint.resource_id(SerialEndpointResourceRole::TxBuffer)?;
            format!(
                "let {id}_parts = ferrowasp_stm32f4::uart_dma::init_uart4_endpoint(\n    Uart4EndpointResources {{\n        tx_pin: gpioa.pa0,\n        rx_pin: gpioa.pa1,\n        uart: cx.device.UART4,\n        rx_dma: dma1.2,\n        tx_dma: dma1.4,\n    }},\n    &mut rcc,\n    {id}_service.profile().protocol,\n    UartRxStorageResources {{\n        buffers: cx.local.{rx_buffers},\n        free_queue: cx.local.{rx_free_queue},\n        filled_queue: cx.local.{rx_filled_queue},\n    }},\n    cx.local.{tx_buffer},\n);"
            )
        }
        SerialPeripheral::Usart2 => {
            let tx_buffer = endpoint.resource_id(SerialEndpointResourceRole::TxBuffer)?;
            format!(
                "let {id}_parts = ferrowasp_stm32f4::uart_dma::init_usart2_endpoint(\n    Usart2EndpointResources {{\n        tx_pin: gpioa.pa2,\n        rx_pin: gpioa.pa3,\n        usart: cx.device.USART2,\n        rx_dma: dma1.5,\n        tx_dma: dma1.6,\n    }},\n    &mut rcc,\n    {id}_service.profile().protocol,\n    UartRxStorageResources {{\n        buffers: cx.local.{rx_buffers},\n        free_queue: cx.local.{rx_free_queue},\n        filled_queue: cx.local.{rx_filled_queue},\n    }},\n    cx.local.{tx_buffer},\n);"
            )
        }
        peripheral => bail!("unsupported serial initialization for {peripheral:?}"),
    };
    let mut rendered = format!("{service}\n{initialization}");

    rendered.push_str(&format!(
        "\nlet {rx_service} = {id}_parts.rx_irq;\nlet {id}_rx_parser = {id}_parts.parser;\nlet ({id}_rx_producer, {rx_reader}, {rx_discontinuities}) =\n    cx.local.{rx_channel}.split();\nlet {rx_bridge} = UartOwnedRxBridge::new({id}_rx_parser, {id}_rx_producer);"
    ));

    let tx_dma = endpoint.resource_id(SerialEndpointResourceRole::TxDma)?;
    let tx_channel = endpoint.resource_id(SerialEndpointResourceRole::TxChannel)?;
    let tx_writer = endpoint.resource_id(SerialEndpointResourceRole::TxWriter)?;
    let tx_owner = endpoint.resource_id(SerialEndpointResourceRole::TxOwner)?;
    let tx_completion = endpoint.resource_id(SerialEndpointResourceRole::TxCompletion)?;
    rendered.push_str(&format!(
        "\nlet {tx_dma} = {id}_parts.tx_dma;\nlet ({tx_writer}, {tx_owner}, {tx_completion}) =\n    cx.local.{tx_channel}.split();"
    ));
    Ok(rendered)
}

fn render_imu_endpoint_initialization(
    app: &ResolvedApp,
    endpoint: &ResolvedImuEndpoint,
) -> Result<String> {
    validate_supported_imu_endpoint(endpoint)?;
    let delay = app
        .init_delay
        .context("SPI IMU initialization requires a general initialization delay timer")?;
    match timer_peripheral(app, delay.hardware_id)? {
        TimerPeripheral::Tim5 => {}
        unsupported => bail!(
            "the STM32F4 renderer does not support {unsupported:?} as the initialization delay"
        ),
    }
    let delay_id = "init_delay";
    let id = endpoint.declaration.id;
    let buffers = endpoint.resource_id(ImuEndpointResourceRole::Buffers)?;
    let free_queue = endpoint.resource_id(ImuEndpointResourceRole::FreeQueue)?;
    let filled_queue = endpoint.resource_id(ImuEndpointResourceRole::FilledQueue)?;
    let owner = endpoint.resource_id(ImuEndpointResourceRole::Owner)?;
    let parser = endpoint.resource_id(ImuEndpointResourceRole::Parser)?;
    let device = endpoint.resource_id(ImuEndpointResourceRole::Device)?;
    let data_ready = endpoint.resource_id(ImuEndpointResourceRole::DataReady)?;
    let kind = endpoint.resource_id(ImuEndpointResourceRole::Kind)?;
    let sample = endpoint.resource_id(ImuEndpointResourceRole::Sample)?;
    let unavailable_logged = endpoint.resource_id(ImuEndpointResourceRole::UnavailableLogged)?;
    let mailbox = format!("{}_MAILBOX", id.to_ascii_uppercase());
    let owner_task = format!("{id}_owner_service");

    let service_selection = format!(
        "match platform_config.spi1 {{\n    Some(SpiService::Imu(id)) if id == ImuInstallationId::new({}) => {{}},\n    Some(SpiService::Imu(id)) => panic!(\"boot platform config selected IMU installation {{}} but spi1 initialized installation {}\", id.get()),\n    None => panic!(\"boot platform config must assign spi1\"),\n}}",
        endpoint.hardware.id.get(),
        endpoint.hardware.id.get(),
    );

    let initialization = format!(
        r#"let mut {id}_cs = spi_dma::init_spi1_imu_cs(gpioa.pa4);
let {id}_mode = spi::Mode {{
    polarity: spi::Polarity::IdleHigh,
    phase: spi::Phase::CaptureOnSecondTransition,
}};
let mut {id}_bus = spi_dma::init_spi1_bus(
    cx.device.SPI1,
    gpioa.pa5,
    gpioa.pa6,
    gpioa.pa7,
    {id}_mode,
    1_000_000,
    &mut rcc,
);
embedded_hal::delay::DelayNs::delay_ms(&mut {delay_id}, 10);
let {kind} = match ferrowasp_drivers::icm42688p::read_who_am_i(
    &mut {id}_bus,
    &mut {id}_cs,
) {{
    Ok(ferrowasp_drivers::mpu6500::WHO_AM_I_EXPECTED)
        if ferrowasp_drivers::mpu6500::init(
            &mut {id}_bus,
            &mut {id}_cs,
            &mut {delay_id},
        ).is_ok() => IMU_KIND_MPU6500,
    Ok(ferrowasp_drivers::icm42688p::WHO_AM_I_EXPECTED)
        if ferrowasp_drivers::icm42688p::init(
            &mut {id}_bus,
            &mut {id}_cs,
            &mut {delay_id},
        ).is_ok() => IMU_KIND_ICM42688P,
    Ok(identity) => {{
        defmt::warn!("unsupported SPI1 IMU identity: {{=u8:02x}}", identity);
        IMU_KIND_NONE
    }}
    Err(_) => {{
        defmt::warn!("SPI1 IMU probe failed");
        IMU_KIND_NONE
    }}
}};
let {id}_dma = spi_dma::init_spi_dma::<_, _, _, 3, 3>(
    {id}_bus,
    dma2.0,
    dma2.3,
    SpiDmaStorageResources {{
        buffers: cx.local.{buffers},
        free_queue: cx.local.{free_queue},
        filled_queue: cx.local.{filled_queue},
    }}.into_backend(),
);
let spi_dma::SpiDmaParts {{
    irq: {id}_irq,
    poller: {id}_poller,
    parser: {id}_parser_side,
    recovery_rx_buffer: {id}_recovery_rx_buffer,
}} = {id}_dma;
let {owner} = Spi1ImuEndpointOwner::new(
    spi_dma::SpiDmaOwner::new(
        {id}_irq,
        {id}_poller,
        {id}_recovery_rx_buffer,
        {id}_cs,
    ),
    &{mailbox},
);
let {parser} = Spi1ImuParser::new({id}_parser_side);
let {data_ready} = ferrowasp_stm32f4::exti::init_input(
    gpioa_placeholder_for_pc4,
    &mut syscfg,
    &mut exti,
    Edge::Rising,
);
let {device} = Spi1ImuDevice::new(
    &{mailbox},
    || {{ let _ = {owner_task}::spawn(); }},
);
let {sample} = ImuData::default();
let {unavailable_logged} = false;"#,
    );
    Ok(format!("{service_selection}\n{initialization}")
        .replace("gpioa_placeholder_for_pc4", "gpioc.pc4"))
}

fn render_periodic_control_initialization(control: &ResolvedPeriodicControl) -> Result<String> {
    use PeriodicControlResourceRole::{Phase, Scheduler};

    let peripheral = timer_pac_identifier(control.hardware.peripheral);
    let scheduler = control.resource_id(Scheduler)?;
    let phase = control.resource_id(Phase)?;
    Ok(format!(
        "let {scheduler} = init_control_scheduler(\n    cx.device.{peripheral},\n    &mut rcc,\n    {}.Hz(),\n)\n.expect(\"validated periodic control timer must start\");\nlet {phase} = 0;",
        render_u32(control.declaration.scheduler_hz)
    ))
}

fn task_state_type(recipe: TaskStateRecipe) -> &'static str {
    recipe.state_type().rust_type()
}

fn render_task_state_initialization(app: &ResolvedApp) -> String {
    let mut lines = Vec::new();
    let mut owners = BTreeSet::new();
    for state in &app.task_state {
        if owners.insert(state.owner_task) {
            lines.push(section_guard(&format!(
                "Application-owned state for task `{}` initialization",
                state.owner_task
            )));
        }
        lines.push(format!(
            "let {} = {};",
            state.id,
            task_state_initializer(state.recipe)
        ));
    }
    lines.join("\n")
}

fn task_state_initializer(recipe: TaskStateRecipe) -> String {
    match recipe {
        TaskStateRecipe::U32(initial) => render_u32(initial),
        TaskStateRecipe::Bool(initial) => initial.to_string(),
        TaskStateRecipe::FoxeerFlightControllerV1 => r#"dt::FlightController::new(
    dt::FlightControllerConfig {
        max_throttle: 2_000.0,
        rescale_throttles: true,
        clamp_negative_to_zero: true,
    },
    dt::RateController::new(
        dt::RateControllerGains {
            roll: dt::PidGains { p: 2.5, i: 0.0, d: 0.0 },
            pitch: dt::PidGains { p: 2.5, i: 0.0, d: 0.0 },
            yaw: dt::PidGains { p: 2.0, i: 0.0, d: 0.0 },
        },
        2_000.0,
    ),
)"#
        .to_owned(),
        TaskStateRecipe::FoxeerImuRateLowPassFilterV1 => {
            "dt::ImuRateLowPassFilter::new(0.55)".to_owned()
        }
        TaskStateRecipe::GyroAngleIntegratorV1 => "dt::GyroAngleIntegrator::new()".to_owned(),
        TaskStateRecipe::BodyRateToControllerMapV1 => {
            "ferrowasp_core::frames::BODY_RATE_TO_RATE_CONTROLLER_MAP".to_owned()
        }
        TaskStateRecipe::FoxeerGyroBiasCalibratorV1 => {
            "dt::GyroBiasCalibrator::new(800, 1_000)".to_owned()
        }
        TaskStateRecipe::EmptyRcInputSnapshotV1 => "RcInputSnapshot::new()".to_owned(),
        TaskStateRecipe::OutputInhibitedFoxeerSafetyMasterV1 => {
            "ferrowasp_tasks::foxeer_safety::FoxeerSafetyMaster::output_inhibited()".to_owned()
        }
    }
}

fn periodic_control_scheduler_type(peripheral: TimerPeripheral) -> String {
    format!(
        "ferrowasp_stm32f4::rtic::hal::timer::CounterHz<ferrowasp_stm32f4::rtic::hal::pac::{}>",
        timer_pac_identifier(peripheral)
    )
}

const fn timer_pac_identifier(peripheral: TimerPeripheral) -> &'static str {
    match peripheral {
        TimerPeripheral::Tim2 => "TIM2",
        TimerPeripheral::Tim4 => "TIM4",
        TimerPeripheral::Tim5 => "TIM5",
        TimerPeripheral::Tim6 => "TIM6",
    }
}

fn timer_peripheral(app: &ResolvedApp, hardware_id: &str) -> Result<TimerPeripheral> {
    app.board
        .timer(hardware_id)
        .map(|timer| timer.peripheral)
        .with_context(|| format!("board timer `{hardware_id}` disappeared after validation"))
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

fn section_guard(label: &str) -> String {
    format!("// ===== {label} =====")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{input_catalog::foxeer_f405_v2::app_composition::APP_COMPOSITION, rtic::resolve};

    #[test]
    fn current_board_lowers_foxeer_sbus_uart4_and_spi1_imu() {
        let app = resolve::resolve(&APP_COMPOSITION).unwrap();
        let rendered = lower(&app).unwrap();

        assert!(rendered.timing.contains("168_000_000"));
        assert!(rendered.timing.contains("stm32_tim2_monotonic"));
        assert!(
            rendered
                .initialization
                .contains("Mono::start(rcc.clocks.timclk1().raw())")
        );
        assert!(
            rendered
                .initialization
                .contains("let mut init_delay = cx.device.TIM5.delay::<1_000_000>")
        );
        assert!(!rendered.initialization.contains("spi1_delay"));
        assert!(!rendered.initialization.contains("cx.device.TIM2"));
        assert!(
            rendered
                .initialization
                .contains("init_control_scheduler(\n    cx.device.TIM4")
        );
        assert!(rendered.initialization.contains("800.Hz()"));
        assert!(
            rendered
                .initialization
                .contains("let flight_controller = dt::FlightController::new(")
        );
        assert!(rendered.initialization.contains("p: 2.5"));
        assert!(
            rendered
                .initialization
                .contains("dt::ImuRateLowPassFilter::new(0.55)")
        );
        assert!(
            rendered.initialization.contains(
                "gyro_axis_map = ferrowasp_core::frames::BODY_RATE_TO_RATE_CONTROLLER_MAP"
            )
        );
        assert!(
            rendered
                .initialization
                .contains("dt::GyroBiasCalibrator::new(800, 1_000)")
        );
        assert!(
            rendered
                .local_fields
                .iter()
                .any(|field| field == "flight_controller: dt::FlightController,")
        );
        assert!(
            rendered
                .local_fields
                .iter()
                .any(|field| field.contains("control_scheduler:") && field.contains("TIM4"))
        );
        assert!(
            rendered
                .local_fields
                .iter()
                .any(|field| field == "control_phase: u32,")
        );
        assert!(rendered.initialization.contains("gpioa.pa5"));
        assert!(rendered.initialization.contains("gpioc.pc4"));
        assert!(rendered.initialization.contains("init_usart2_endpoint"));
        assert!(rendered.initialization.contains("init_uart4_endpoint"));
        assert!(
            rendered
                .initialization
                .contains("Boot-time serial service routing")
        );
        assert!(rendered.initialization.contains("init_spi_dma"));
        assert!(rendered.initialization.contains("read_who_am_i"));
        assert!(
            rendered
                .shared_fields
                .iter()
                .any(|field| field.contains("serial2_tx_dma"))
        );
        assert!(
            rendered
                .local_fields
                .iter()
                .any(|field| field == "serial2_tx_owner: UartOwnedTxOwner<'static>,")
        );
        assert!(
            rendered
                .local_fields
                .iter()
                .any(|field| field == "rc_sbus_reader: UartOwnedReader<'static>,")
        );
    }
}
