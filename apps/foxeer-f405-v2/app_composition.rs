//! Output-inhibited Foxeer F405 V2 generated candidate.
//!
//! This declaration is authoring data only. It deliberately does not generate
//! or contain an `#[rtic::app]` module.

use fugit::MillisDurationU32;
use stm32f4xx_hal::pac::Interrupt;

use crate::{
    backends::stm32f4::{
        dshot_actuator::DshotActuatorDeclaration,
        endpoints::imu::{SPI_DMA_ENDPOINT, SpiEndpointDeclaration},
        endpoints::serial::{SERIAL_DMA_ENDPOINT, SerialEndpointDeclaration},
        periodic_control::{PERIODIC_CONTROL_TIMER, PeriodicControlDeclaration},
    },
    input_catalog::{
        foxeer_board as board, foxeer_golden_services::GoldenServicesDeclaration,
        foxeer_platform_config as platform_config, selected_tasks as tasks,
    },
    rtic::{
        component::ComponentDeclaration,
        composition::{AppComposition, SharedResourceDeclaration, TaskDeclaration},
        safety_channel::SafetyChannelDeclaration,
        state::{
            FOXEER_CONTROL_STATE_V2, FOXEER_SAFETY_STATE_V1, TaskStateDeclaration, TaskStateField,
            TaskStateRecipe, TaskStateRole,
        },
        timing::{InitDelayDeclaration, MonotonicDeclaration},
    },
};

/// Latest healthy value of the third one-indexed SBUS channel.
pub const SBUS_CHANNEL3: SharedResourceDeclaration =
    SharedResourceDeclaration::u32("sbus_channel3", 0);

/// Exclusive valid-SBUS stream consumed by the control task.
pub const SBUS_TO_CONTROL: SafetyChannelDeclaration = SafetyChannelDeclaration::sbus_input(
    "sbus_to_control",
    4,
    "sbus_control_producer",
    "sbus_control_consumer",
);

/// Exclusive decoded-IMU stream consumed by the control task.
pub const IMU_TO_CONTROL: SafetyChannelDeclaration = SafetyChannelDeclaration::imu_samples(
    "imu_to_control",
    4,
    "imu_control_producer",
    "imu_control_consumer",
);

/// Exclusive golden-capacity motor requests from control to actuator safety.
pub const CONTROL_TO_ACTUATOR: SafetyChannelDeclaration = SafetyChannelDeclaration::motor_commands(
    "control_to_actuator",
    ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY,
    "motor_cmd_producer",
    "motor_cmd_consumer",
);

/// Observation-only IMU readiness/calibration/freshness evidence into safety.
pub const CONTROL_TO_SAFETY_HEALTH: SafetyChannelDeclaration =
    SafetyChannelDeclaration::prearm_health(
        "control_to_safety_health",
        4,
        "prearm_health_producer",
        "prearm_health_consumer",
    );

/// Fresh safety-owned authority observations consumed only by the actuator.
pub const SAFETY_TO_ACTUATOR_GUARD: SafetyChannelDeclaration =
    SafetyChannelDeclaration::actuator_guard(
        "safety_to_actuator_guard",
        ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY,
        "actuator_guard_producer",
        "actuator_guard_consumer",
    );

/// Pre-arm completion or abort reports from the sole physical adapter.
pub const ACTUATOR_TO_SAFETY: SafetyChannelDeclaration =
    SafetyChannelDeclaration::actuator_preparation(
        "actuator_to_safety",
        ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY,
        "actuator_completion_producer",
        "actuator_completion_consumer",
    );

/// Terminal physical-service faults serialized by one reporter task.
pub const ACTUATOR_FAULT_TO_SAFETY: SafetyChannelDeclaration =
    SafetyChannelDeclaration::actuator_preparation(
        "actuator_fault_to_safety",
        ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY,
        "actuator_fault_producer",
        "actuator_fault_consumer",
    );

/// Exact Foxeer physical bank and legacy ESC telemetry, independently disabled.
pub const PHYSICAL_ACTUATOR: DshotActuatorDeclaration = DshotActuatorDeclaration::foxeer_disabled(
    "physical_actuator",
    board::FOXEER_DSHOT.id,
    "esc_telemetry",
);

/// Exact ADC, SPI NOR, USB CDC, and watchdog service hardware selection.
pub const GOLDEN_SERVICES: GoldenServicesDeclaration = GoldenServicesDeclaration::foxeer(
    "golden_services",
    board::FOXEER_ADC1.id,
    board::FOXEER_SPI2_NOR.id,
    board::FOXEER_USB_CDC.id,
    board::TIM6.id,
);

/// Full-duplex physical connector backed by USART2.
pub const SERIAL1_ENDPOINT: SerialEndpointDeclaration = SERIAL_DMA_ENDPOINT
    .declare("serial1", "serial1")
    .interrupt_priority(13)
    .bridge_priority(12)
    .worker_priority(4)
    .rx_buffer_count(4)
    .rx_queue_depth(4)
    .tx_queue_depth(16);

/// Full-duplex physical connector backed by UART4.
pub const SERIAL2_ENDPOINT: SerialEndpointDeclaration = SERIAL_DMA_ENDPOINT
    .declare("serial2", "serial2")
    .interrupt_priority(6)
    .bridge_priority(4)
    .worker_priority(4)
    .rx_buffer_count(4)
    .rx_queue_depth(4)
    .tx_queue_depth(16);

/// Physical SPI1 endpoint with the currently supported IMU service adapter.
pub const SPI1_ENDPOINT: SpiEndpointDeclaration = SPI_DMA_ENDPOINT
    .declare("spi1", "spi1")
    .data_ready_priority(14)
    .owner_priority(13)
    .poll_priority(12)
    .parser_priority(11);

/// TIM4 periodic scheduler preserving the golden 800/400 Hz control cadence.
pub const CONTROL_TIMER: PeriodicControlDeclaration = PERIODIC_CONTROL_TIMER
    .declare("control", board::TIM4.id)
    .scheduler_hz(800)
    .control_hz(400)
    .interrupt_priority(14)
    .task(&tasks::foxeer_control::CONTRACT)
    .safety_critical()
    .with_local(&[
        tasks::foxeer_control::LOCAL
            .sbus
            .bind("sbus_control_consumer"),
        tasks::foxeer_control::LOCAL
            .imu
            .bind("imu_control_consumer"),
        tasks::foxeer_control::LOCAL
            .motor_commands
            .bind("motor_cmd_producer"),
        tasks::foxeer_control::LOCAL
            .prearm_health
            .bind("prearm_health_producer"),
        tasks::foxeer_control::LOCAL
            .flash_records
            .bind("flash_record_producer"),
    ])
    .with_shared(&[
        tasks::foxeer_control::SHARED
            .telemetry
            .bind("flight_service_telemetry"),
        tasks::foxeer_control::SHARED.tuning.bind("tuning_profile"),
        tasks::foxeer_control::SHARED
            .tuning_seq
            .bind("tuning_request_seq"),
        tasks::foxeer_control::SHARED
            .log_divisor
            .bind("flash_log_rate_divisor"),
        tasks::foxeer_control::SHARED.storage.bind("storage_status"),
    ])
    .with_spawns(&[tasks::foxeer_control::SPAWNS
        .actuator_wake
        .bind("actuator_output")]);

/// Persistent golden control state, owned exclusively by the TIM4 task.
pub const CONTROL_STATE_FIELDS: &[TaskStateField] = &[
    TaskStateField::new(
        "control_loop_cnt",
        TaskStateRole::ControlLoopCounter,
        TaskStateRecipe::U32(0),
    ),
    TaskStateField::new(
        "samples_per_control_loop",
        TaskStateRole::SamplesPerControlLoop,
        TaskStateRecipe::U32(2),
    ),
    TaskStateField::new(
        "flight_controller",
        TaskStateRole::FlightController,
        TaskStateRecipe::FoxeerFlightControllerV1,
    ),
    TaskStateField::new(
        "imu_rate_filter",
        TaskStateRole::ImuRateFilter,
        TaskStateRecipe::FoxeerImuRateLowPassFilterV1,
    ),
    TaskStateField::new(
        "imu_angle_integrator",
        TaskStateRole::ImuAngleIntegrator,
        TaskStateRecipe::GyroAngleIntegratorV1,
    ),
    TaskStateField::new(
        "gyro_axis_map",
        TaskStateRole::GyroAxisMap,
        TaskStateRecipe::BodyRateToControllerMapV1,
    ),
    TaskStateField::new(
        "gyro_bias_calibrator",
        TaskStateRole::GyroBiasCalibrator,
        TaskStateRecipe::FoxeerGyroBiasCalibratorV1,
    ),
    TaskStateField::new(
        "imu_last_sequence",
        TaskStateRole::ImuLastSequence,
        TaskStateRecipe::U32(0),
    ),
    TaskStateField::new(
        "imu_stale_ticks",
        TaskStateRole::ImuStaleTicks,
        TaskStateRecipe::U32(0),
    ),
    TaskStateField::new(
        "applied_tuning_seq",
        TaskStateRole::AppliedTuningSequence,
        TaskStateRecipe::U32(0),
    ),
    TaskStateField::new(
        "motor_cmd_seq",
        TaskStateRole::MotorCommandSequence,
        TaskStateRecipe::U32(0),
    ),
    TaskStateField::new(
        "rc_link_was_valid",
        TaskStateRole::RcLinkWasValid,
        TaskStateRecipe::Bool(false),
    ),
    TaskStateField::new(
        "latest_rc_input",
        TaskStateRole::LatestRcInput,
        TaskStateRecipe::EmptyRcInputSnapshotV1,
    ),
    TaskStateField::new(
        "rc_last_valid_frames",
        TaskStateRole::RcLastValidFrames,
        TaskStateRecipe::U32(0),
    ),
    TaskStateField::new(
        "rc_stale_ticks",
        TaskStateRole::RcStaleTicks,
        TaskStateRecipe::U32(0),
    ),
    TaskStateField::new(
        "control_was_armed",
        TaskStateRole::ControlWasArmed,
        TaskStateRecipe::Bool(false),
    ),
];

/// Versioned state schema instance for the generated `control_loop` task.
pub const CONTROL_STATE: TaskStateDeclaration =
    FOXEER_CONTROL_STATE_V2.declare("control_loop", CONTROL_STATE_FIELDS);

/// Output-inhibited safety state owned exclusively by the priority-16 task.
pub const SAFETY_STATE_FIELDS: &[TaskStateField] = &[TaskStateField::new(
    "foxeer_safety_state",
    TaskStateRole::SafetyMaster,
    TaskStateRecipe::OutputInhibitedFoxeerSafetyMasterV1,
)];

/// Versioned state-schema instance for the generated safety master.
pub const SAFETY_STATE: TaskStateDeclaration =
    FOXEER_SAFETY_STATE_V1.declare("safety_master", SAFETY_STATE_FIELDS);

/// Owns the golden RC-link state, arming state, and actuator permit.
pub const SAFETY_MASTER_TASK: TaskDeclaration =
    TaskDeclaration::software("safety_master", &tasks::foxeer_safety_master::CONTRACT)
        .priority(16)
        .safety_critical()
        .with_local(&[
            tasks::foxeer_safety_master::LOCAL
                .reader
                .bind("rc_sbus_reader"),
            tasks::foxeer_safety_master::LOCAL
                .discontinuities
                .bind("rc_sbus_rx_discontinuities"),
            tasks::foxeer_safety_master::LOCAL
                .foxeer_safety_state
                .bind("foxeer_safety_state"),
            tasks::foxeer_safety_master::LOCAL
                .control
                .bind("sbus_control_producer"),
            tasks::foxeer_safety_master::LOCAL
                .health
                .bind("prearm_health_consumer"),
            tasks::foxeer_safety_master::LOCAL
                .actuator_guards
                .bind("actuator_guard_producer"),
            tasks::foxeer_safety_master::LOCAL
                .actuator_completions
                .bind("actuator_completion_consumer"),
            tasks::foxeer_safety_master::LOCAL
                .actuator_faults
                .bind("actuator_fault_consumer"),
        ])
        .with_shared(&[
            tasks::foxeer_safety_master::SHARED
                .channel3
                .bind("sbus_channel3"),
            tasks::foxeer_safety_master::SHARED
                .telemetry
                .bind("flight_service_telemetry"),
            tasks::foxeer_safety_master::SHARED
                .tuning
                .bind("tuning_profile"),
        ])
        .with_config(&[tasks::foxeer_safety_master::CONFIG
            .poll_interval
            .set(MillisDurationU32::millis(10))])
        .with_spawns(&[tasks::foxeer_safety_master::SPAWNS
            .actuator_request
            .bind("actuator_output")]);

/// Transfers newly decoded endpoint samples into the control safety channel.
pub const IMU_CONTROL_BRIDGE_TASK: TaskDeclaration =
    TaskDeclaration::software("imu_control_bridge", &tasks::imu_control_bridge::CONTRACT)
        .priority(11)
        .safety_critical()
        .with_local(&[tasks::imu_control_bridge::LOCAL
            .control
            .bind("imu_control_producer")])
        .with_shared(&[tasks::imu_control_bridge::SHARED.sample.bind("imu_sample")])
        .with_config(&[tasks::imu_control_bridge::CONFIG
            .poll_interval
            .set(MillisDurationU32::millis(1))]);

/// Golden MSP DisplayPort RX/reply, overlay, and disarmed tuning service.
pub const MSP_OSD_TASK: TaskDeclaration =
    TaskDeclaration::software("msp_osd", &tasks::msp_osd::CONTRACT)
        .priority(GOLDEN_SERVICES.osd_priority)
        .with_local(&[
            tasks::msp_osd::LOCAL.reader.bind("msp_v1_osd_reader"),
            tasks::msp_osd::LOCAL
                .discontinuities
                .bind("msp_v1_osd_rx_discontinuities"),
            tasks::msp_osd::LOCAL.writer.bind("msp_v1_osd_writer"),
            tasks::msp_osd::LOCAL.osd.bind("osd_task_state"),
            tasks::msp_osd::LOCAL.output.bind("osd_tx_buffer"),
            tasks::msp_osd::LOCAL.refresh_tick.bind("osd_refresh_tick"),
            tasks::msp_osd::LOCAL.tx_healthy.bind("osd_tx_healthy"),
        ])
        .with_shared(&[
            tasks::msp_osd::SHARED
                .telemetry
                .bind("flight_service_telemetry"),
            tasks::msp_osd::SHARED.tuning.bind("tuning_profile"),
            tasks::msp_osd::SHARED.tuning_seq.bind("tuning_request_seq"),
        ])
        .with_config(&[tasks::msp_osd::CONFIG
            .interval
            .set(MillisDurationU32::millis(GOLDEN_SERVICES.osd_period_ms))]);

/// Starts one bounded ADC1 voltage/current conversion every 100 ms.
pub const ADC_OBSERVATION_POLL_TASK: TaskDeclaration = TaskDeclaration::software(
    "adc_observation_poll",
    &tasks::adc_observation_poll::CONTRACT,
)
.priority(GOLDEN_SERVICES.adc_poll_priority)
.with_shared(&[tasks::adc_observation_poll::SHARED
    .transfer
    .bind("adc1_transfer")])
.with_config(&[tasks::adc_observation_poll::CONFIG
    .interval
    .set(MillisDurationU32::millis(GOLDEN_SERVICES.adc_period_ms))]);

/// DMA2 Stream4 owner for freshness-tracked ADC1 observations.
pub const ADC_OBSERVATION_DMA_TASK: TaskDeclaration = TaskDeclaration::interrupt(
    "adc_observation_dma",
    &tasks::adc_observation_dma::CONTRACT,
    Interrupt::DMA2_STREAM4,
)
.priority(GOLDEN_SERVICES.adc_interrupt_priority)
.with_local(&[
    tasks::adc_observation_dma::LOCAL.buffer.bind("adc1_buffer"),
    tasks::adc_observation_dma::LOCAL
        .planner
        .bind("adc1_planner"),
    tasks::adc_observation_dma::LOCAL
        .cell_detector
        .bind("battery_cell_detector"),
])
.with_shared(&[
    tasks::adc_observation_dma::SHARED
        .transfer
        .bind("adc1_transfer"),
    tasks::adc_observation_dma::SHARED
        .telemetry
        .bind("flight_service_telemetry"),
])
.with_config(&[
    tasks::adc_observation_dma::CONFIG
        .vbat_divider_x10
        .set(GOLDEN_SERVICES.vbat_divider_x10 as u32),
    tasks::adc_observation_dma::CONFIG
        .current_scale
        .set(GOLDEN_SERVICES.current_scale),
    tasks::adc_observation_dma::CONFIG
        .current_offset_ma
        .set(GOLDEN_SERVICES.current_offset_ma as u32),
]);

/// Sole bounded SPI2 NOR owner for blackbox and copy-on-write configuration.
pub const GOLDEN_FLASH_TASK: TaskDeclaration =
    TaskDeclaration::software("golden_flash", &tasks::golden_flash::CONTRACT)
        .priority(GOLDEN_SERVICES.flash_priority)
        .with_local(&[
            tasks::golden_flash::LOCAL.flash.bind("flash_device"),
            tasks::golden_flash::LOCAL
                .records
                .bind("flash_record_consumer"),
            tasks::golden_flash::LOCAL
                .commands
                .bind("flash_command_consumer"),
            tasks::golden_flash::LOCAL
                .responses
                .bind("flash_response_producer"),
            tasks::golden_flash::LOCAL.state.bind("flash_manager_state"),
        ])
        .with_shared(&[
            tasks::golden_flash::SHARED
                .telemetry
                .bind("flight_service_telemetry"),
            tasks::golden_flash::SHARED.tuning.bind("tuning_profile"),
            tasks::golden_flash::SHARED
                .tuning_seq
                .bind("tuning_request_seq"),
            tasks::golden_flash::SHARED
                .log_divisor
                .bind("flash_log_rate_divisor"),
            tasks::golden_flash::SHARED.status.bind("storage_status"),
        ])
        .with_config(&[tasks::golden_flash::CONFIG
            .interval
            .set(MillisDurationU32::millis(GOLDEN_SERVICES.flash_period_ms))]);

/// OTG_FS CDC diagnostics and whitelisted disarmed storage/configuration commands.
pub const USB_CDC_TASK: TaskDeclaration =
    TaskDeclaration::interrupt("usb_cdc", &tasks::usb_cdc::CONTRACT, Interrupt::OTG_FS)
        .priority(GOLDEN_SERVICES.usb_priority)
        .with_local(&[
            tasks::usb_cdc::LOCAL.device.bind("usb_device"),
            tasks::usb_cdc::LOCAL.serial.bind("usb_serial"),
            tasks::usb_cdc::LOCAL.header_sent.bind("usb_header_sent"),
            tasks::usb_cdc::LOCAL.parser.bind("usb_command_parser"),
            tasks::usb_cdc::LOCAL
                .commands
                .bind("flash_command_producer"),
            tasks::usb_cdc::LOCAL
                .responses
                .bind("flash_response_consumer"),
            tasks::usb_cdc::LOCAL
                .pending_response
                .bind("usb_pending_response"),
        ])
        .with_shared(&[
            tasks::usb_cdc::SHARED
                .telemetry
                .bind("flight_service_telemetry"),
            tasks::usb_cdc::SHARED.storage.bind("storage_status"),
            tasks::usb_cdc::SHARED.status_due.bind("usb_status_due"),
        ]);

/// Golden two-second service heartbeat and USB diagnostic scheduler.
pub const HEARTBEAT_TASK: TaskDeclaration =
    TaskDeclaration::software("heartbeat", &tasks::heartbeat::CONTRACT)
        .priority(GOLDEN_SERVICES.heartbeat_priority)
        .with_shared(&[
            tasks::heartbeat::SHARED
                .telemetry
                .bind("flight_service_telemetry"),
            tasks::heartbeat::SHARED.storage.bind("storage_status"),
            tasks::heartbeat::SHARED
                .usb_status_due
                .bind("usb_status_due"),
        ])
        .with_config(&[tasks::heartbeat::CONFIG
            .interval
            .set(MillisDurationU32::millis(
                GOLDEN_SERVICES.heartbeat_period_ms,
            ))]);

/// TIM6 8 kHz independent I/O deadline watchdog.
pub const IO_WATCHDOG_TASK: TaskDeclaration = TaskDeclaration::interrupt(
    "io_watchdog",
    &tasks::io_watchdog::CONTRACT,
    Interrupt::TIM6_DAC,
)
.priority(GOLDEN_SERVICES.watchdog_priority)
.with_local(&[tasks::io_watchdog::LOCAL.watchdog.bind("io_watchdog")])
.with_shared(&[tasks::io_watchdog::SHARED.spi1_owner.bind("spi1_owner")]);

/// Sole safety-owned adapter from accepted requests to the physical bank.
pub const PHYSICAL_ACTUATOR_TASK: TaskDeclaration =
    TaskDeclaration::software("actuator_output", &tasks::physical_actuator::CONTRACT)
        .priority(15)
        .safety_critical()
        .with_local(&[
            tasks::physical_actuator::LOCAL
                .commands
                .bind("motor_cmd_consumer"),
            tasks::physical_actuator::LOCAL
                .guards
                .bind("actuator_guard_consumer"),
            tasks::physical_actuator::LOCAL
                .completions
                .bind("actuator_completion_producer"),
            tasks::physical_actuator::LOCAL
                .telemetry
                .bind("esc_telemetry_update_consumer"),
        ])
        .with_shared(&[tasks::physical_actuator::SHARED.bank.bind("dshot_motors")])
        .with_config(&[
            tasks::physical_actuator::CONFIG
                .output_enabled
                .set(PHYSICAL_ACTUATOR.output_enabled),
            tasks::physical_actuator::CONFIG
                .poll_interval
                .set(MillisDurationU32::millis(10)),
        ])
        .with_spawns(&[tasks::physical_actuator::SPAWNS
            .fault
            .bind("actuator_fault_reporter")]);

/// Serializes DShot/UART/manager terminal events onto one safety channel.
pub const ACTUATOR_FAULT_REPORTER_TASK: TaskDeclaration = TaskDeclaration::software(
    "actuator_fault_reporter",
    &tasks::actuator_fault_reporter::CONTRACT,
)
.priority(16)
.safety_critical()
.with_local(&[tasks::actuator_fault_reporter::LOCAL
    .reports
    .bind("actuator_fault_producer")]);

macro_rules! dshot_dma_task {
    ($name:literal, $interrupt:expr, $lane:literal) => {
        TaskDeclaration::interrupt($name, &tasks::dshot_dma_complete::CONTRACT, $interrupt)
            .priority(PHYSICAL_ACTUATOR.dma_priority)
            .safety_critical()
            .with_shared(&[tasks::dshot_dma_complete::SHARED.bank.bind("dshot_motors")])
            .with_config(&[tasks::dshot_dma_complete::CONFIG.lane.set($lane)])
            .with_spawns(&[tasks::dshot_dma_complete::SPAWNS
                .fault
                .bind("actuator_fault_reporter")])
    };
}

/// Physical output 1 / DMA2 Stream1 completion handler.
pub const DSHOT_MOTOR1_DMA_TASK: TaskDeclaration =
    dshot_dma_task!("dshot_motor1_dma_complete", Interrupt::DMA2_STREAM1, 1);
/// Physical output 2 / DMA2 Stream7 completion handler.
pub const DSHOT_MOTOR2_DMA_TASK: TaskDeclaration =
    dshot_dma_task!("dshot_motor2_dma_complete", Interrupt::DMA2_STREAM7, 2);
/// Physical output 3 / DMA2 Stream2 completion handler.
pub const DSHOT_MOTOR3_DMA_TASK: TaskDeclaration =
    dshot_dma_task!("dshot_motor3_dma_complete", Interrupt::DMA2_STREAM2, 3);
/// Physical output 4 / DMA2 Stream6 completion handler.
pub const DSHOT_MOTOR4_DMA_TASK: TaskDeclaration =
    dshot_dma_task!("dshot_motor4_dma_complete", Interrupt::DMA2_STREAM6, 4);

/// Bounded two-millisecond DShot frame and command-lease service.
pub const DSHOT_SERVICE_TASK: TaskDeclaration =
    TaskDeclaration::software("dshot_service", &tasks::dshot_service::CONTRACT)
        .priority(PHYSICAL_ACTUATOR.service_priority)
        .safety_critical()
        .with_local(&[
            tasks::dshot_service::LOCAL
                .requests
                .bind("esc_request_consumer"),
            tasks::dshot_service::LOCAL
                .acknowledgements
                .bind("esc_ack_producer"),
            tasks::dshot_service::LOCAL
                .pending_request
                .bind("esc_actuator_request"),
            tasks::dshot_service::LOCAL
                .request_submitted
                .bind("esc_actuator_request_submitted"),
            tasks::dshot_service::LOCAL
                .fault_reported
                .bind("dshot_fault_reported"),
        ])
        .with_shared(&[tasks::dshot_service::SHARED.bank.bind("dshot_motors")])
        .with_config(&[
            tasks::dshot_service::CONFIG
                .output_enabled
                .set(PHYSICAL_ACTUATOR.output_enabled),
            tasks::dshot_service::CONFIG
                .interval
                .set(MillisDurationU32::millis(2)),
        ])
        .with_spawns(&[tasks::dshot_service::SPAWNS
            .fault
            .bind("actuator_fault_reporter")]);

/// DMA2 Stream5 completion/error owner for legacy ESC telemetry.
pub const ESC_UART_RX_DMA_TASK: TaskDeclaration = TaskDeclaration::interrupt(
    "esc_uart_rx_dma",
    &tasks::esc_uart_rx_dma::CONTRACT,
    Interrupt::DMA2_STREAM5,
)
.priority(PHYSICAL_ACTUATOR.telemetry_interrupt_priority)
.with_shared(&[
    tasks::esc_uart_rx_dma::SHARED.uart.bind("uart1_rx"),
    tasks::esc_uart_rx_dma::SHARED
        .discontinuity
        .bind("esc_telemetry_discontinuity"),
]);

/// USART1 IDLE owner for legacy ESC telemetry.
pub const ESC_UART_RX_IDLE_TASK: TaskDeclaration = TaskDeclaration::interrupt(
    "esc_uart_rx_idle",
    &tasks::esc_uart_rx_idle::CONTRACT,
    Interrupt::USART1,
)
.priority(PHYSICAL_ACTUATOR.telemetry_interrupt_priority)
.with_shared(&[
    tasks::esc_uart_rx_idle::SHARED.uart.bind("uart1_rx"),
    tasks::esc_uart_rx_idle::SHARED
        .discontinuity
        .bind("esc_telemetry_discontinuity"),
]);

/// Parses legacy frames, preserves request identity, and publishes fresh observations.
pub const ESC_MANAGER_TASK: TaskDeclaration =
    TaskDeclaration::software("esc_manager", &tasks::esc_manager::CONTRACT)
        .priority(PHYSICAL_ACTUATOR.telemetry_manager_priority)
        .safety_critical()
        .with_local(&[
            tasks::esc_manager::LOCAL.parser.bind("esc_telemetry_uart"),
            tasks::esc_manager::LOCAL.manager.bind("esc_manager_state"),
            tasks::esc_manager::LOCAL
                .requests
                .bind("esc_request_producer"),
            tasks::esc_manager::LOCAL
                .acknowledgements
                .bind("esc_ack_consumer"),
            tasks::esc_manager::LOCAL
                .updates
                .bind("esc_telemetry_update_producer"),
            tasks::esc_manager::LOCAL
                .report_ticks
                .bind("esc_manager_report_ticks"),
        ])
        .with_shared(&[tasks::esc_manager::SHARED
            .discontinuity
            .bind("esc_telemetry_discontinuity")])
        .with_config(&[
            tasks::esc_manager::CONFIG
                .output_enabled
                .set(PHYSICAL_ACTUATOR.output_enabled),
            tasks::esc_manager::CONFIG
                .interval
                .set(MillisDurationU32::millis(2)),
        ])
        .with_spawns(&[tasks::esc_manager::SPAWNS
            .fault
            .bind("actuator_fault_reporter")]);

/// Complete example consumed by composition validation and, later, xtask.
pub const APP_COMPOSITION: AppComposition = AppComposition {
    board: &board::BOARD,
    platform_config: platform_config::platform_config,
    monotonic: MonotonicDeclaration::timer(board::TIM2.id, 1_000_000),
    init_delay: Some(InitDelayDeclaration::timer(board::TIM5.id, 1_000_000)),
    components: &[
        ComponentDeclaration::serial_endpoint(SERIAL1_ENDPOINT),
        ComponentDeclaration::serial_endpoint(SERIAL2_ENDPOINT),
        ComponentDeclaration::spi_endpoint(SPI1_ENDPOINT),
        ComponentDeclaration::periodic_control(CONTROL_TIMER),
        ComponentDeclaration::dshot_actuator(PHYSICAL_ACTUATOR),
        ComponentDeclaration::golden_services(GOLDEN_SERVICES),
    ],
    task_state: &[CONTROL_STATE, SAFETY_STATE],
    shared_resources: &[SBUS_CHANNEL3],
    safety_channels: &[
        SBUS_TO_CONTROL,
        IMU_TO_CONTROL,
        CONTROL_TO_ACTUATOR,
        CONTROL_TO_SAFETY_HEALTH,
        SAFETY_TO_ACTUATOR_GUARD,
        ACTUATOR_TO_SAFETY,
        ACTUATOR_FAULT_TO_SAFETY,
    ],
    tasks: &[
        SAFETY_MASTER_TASK,
        MSP_OSD_TASK,
        IMU_CONTROL_BRIDGE_TASK,
        PHYSICAL_ACTUATOR_TASK,
        ACTUATOR_FAULT_REPORTER_TASK,
        DSHOT_MOTOR1_DMA_TASK,
        DSHOT_MOTOR2_DMA_TASK,
        DSHOT_MOTOR3_DMA_TASK,
        DSHOT_MOTOR4_DMA_TASK,
        DSHOT_SERVICE_TASK,
        ESC_UART_RX_DMA_TASK,
        ESC_UART_RX_IDLE_TASK,
        ESC_MANAGER_TASK,
        ADC_OBSERVATION_POLL_TASK,
        ADC_OBSERVATION_DMA_TASK,
        GOLDEN_FLASH_TASK,
        USB_CDC_TASK,
        HEARTBEAT_TASK,
        IO_WATCHDOG_TASK,
    ],
    init_spawns: &[
        SAFETY_MASTER_TASK.init_spawn(),
        MSP_OSD_TASK.init_spawn(),
        IMU_CONTROL_BRIDGE_TASK.init_spawn(),
        DSHOT_SERVICE_TASK.init_spawn(),
        ESC_MANAGER_TASK.init_spawn(),
        ADC_OBSERVATION_POLL_TASK.init_spawn(),
        GOLDEN_FLASH_TASK.init_spawn(),
        HEARTBEAT_TASK.init_spawn(),
    ],
};
