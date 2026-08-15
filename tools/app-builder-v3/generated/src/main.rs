// GENERATED FILE — DO NOT EDIT DIRECTLY
// Generated from xtask/src/target/board.rs, app_composition.rs, and platform_config.rs.

#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
// Endpoint exports may intentionally have no consumer in a partial composition.
#![allow(dead_code)]

mod platform_config;
mod prelude;

#[rtic::app(
    device = ferrowasp_stm32f4::rtic::hal::pac,
    peripherals = true,
    dispatchers = [EXTI0, EXTI1, EXTI2, EXTI3, CAN1_TX, CAN2_TX, CAN1_RX0, CAN1_RX1]
)]
mod app {
    use crate::prelude::*;

    // ===== SPI endpoint `spi1` static resources =====
    static SPI1_MAILBOX: Spi1ImuMailbox = new_spi1_imu_mailbox();

    const SYSTEM_CLOCK_HZ: u32 = 168_000_000;

    stm32_tim2_monotonic!(Mono, 1_000_000);

    #[shared]
    struct Shared {
        // ===== Application resource `sbus_channel3`; tasks: safety_master =====
        sbus_channel3: u32,
        // ===== Serial endpoint `serial1` resources =====
        serial1_rx: Uart2RxIrq,
        serial1_tx_dma: Uart2TxDmaSide,
        // ===== Serial endpoint `serial2` resources =====
        serial2_rx: Uart4RxIrq,
        serial2_tx_dma: Uart4TxDmaSide,
        // ===== SPI endpoint `spi1` resources =====
        spi1_owner: Spi1ImuEndpointOwner,
        spi1_kind: u8,
        imu_sample: ImuData,
        // ===== Physical DShot actuator resources =====
        dshot_motors: DshotMotorBank,
        uart1_rx: Uart1RxIrq,
        esc_telemetry_discontinuity: bool,
        // ===== Golden ADC observation resources =====
        adc1_transfer: Adc1ObservationTransfer,
        flight_service_telemetry: FlightServiceTelemetry,
        tuning_profile: dt::TuningProfile,
        tuning_request_seq: u32,
        flash_log_rate_divisor: u32,
        storage_status: StorageStatus,
        usb_status_due: bool,
    }

    #[local]
    struct Local {
        // ===== Application-owned state for task `control_loop` =====
        control_loop_cnt: u32,
        samples_per_control_loop: u32,
        flight_controller: dt::FlightController,
        imu_rate_filter: dt::ImuRateLowPassFilter,
        imu_angle_integrator: dt::GyroAngleIntegrator,
        gyro_axis_map: FrameRotation,
        gyro_bias_calibrator: dt::GyroBiasCalibrator,
        imu_last_sequence: u32,
        imu_stale_ticks: u32,
        applied_tuning_seq: u32,
        motor_cmd_seq: u32,
        rc_link_was_valid: bool,
        latest_rc_input: RcInputSnapshot,
        rc_last_valid_frames: u32,
        rc_stale_ticks: u32,
        control_was_armed: bool,
        // ===== Application-owned state for task `safety_master` =====
        foxeer_safety_state: ferrowasp_tasks::foxeer_safety::FoxeerSafetyMaster,
        // ===== Serial endpoint `serial1` resources =====
        serial1_rx_bridge: UartOwnedRxBridge<
            'static,
            { memory::UART_RX_BUFFER_BYTES },
            { memory::OWNED_UART_RX_QUEUE_DEPTH },
        >,
        serial1_tx_owner: UartOwnedTxOwner<'static>,
        serial1_tx_completion: UartOwnedTxCompletion<'static>,
        // ===== Serial endpoint `serial2` resources =====
        serial2_rx_bridge: UartOwnedRxBridge<
            'static,
            { memory::UART_RX_BUFFER_BYTES },
            { memory::OWNED_UART_RX_QUEUE_DEPTH },
        >,
        serial2_tx_owner: UartOwnedTxOwner<'static>,
        serial2_tx_completion: UartOwnedTxCompletion<'static>,
        // ===== Serial service `RcSbus` boot-routed resources =====
        rc_sbus_reader: UartOwnedReader<'static>,
        rc_sbus_rx_discontinuities: UartOwnedDiscontinuities<'static>,
        // ===== Serial service `MspV1Osd` boot-routed resources =====
        msp_v1_osd_reader: UartOwnedReader<'static>,
        msp_v1_osd_rx_discontinuities: UartOwnedDiscontinuities<'static>,
        msp_v1_osd_writer: UartOwnedWriter<'static>,
        // ===== SPI endpoint `spi1` resources =====
        spi1_parser: Spi1ImuParser,
        spi1_device: Spi1ImuDevice,
        spi1_data_ready: Pin<'C', 4, Input>,
        spi1_unavailable_logged: bool,
        // ===== Periodic control `control` task-local resources =====
        control_scheduler:
            ferrowasp_stm32f4::rtic::hal::timer::CounterHz<ferrowasp_stm32f4::rtic::hal::pac::TIM4>,
        control_phase: u32,
        // ===== Physical DShot actuator resources =====
        esc_telemetry_uart: UartRxParserSide,
        esc_request_producer: EscRequestProducer,
        esc_request_consumer: EscRequestConsumer,
        esc_ack_producer: EscAckProducer,
        esc_ack_consumer: EscAckConsumer,
        esc_telemetry_update_producer: EscTelemetryUpdateProducer,
        esc_telemetry_update_consumer: EscTelemetryUpdateConsumer,
        esc_manager_state: EscManager,
        esc_manager_report_ticks: u16,
        esc_actuator_request: Option<EscActuatorRequest>,
        esc_actuator_request_submitted: bool,
        dshot_fault_reported: bool,
        // ===== Golden ADC observation resources =====
        adc1_buffer: Option<&'static mut [u16; 3]>,
        adc1_planner: AdcDmaIrqPlanner,
        battery_cell_detector: BatteryCellDetector,
        osd_task_state: OsdTask,
        osd_tx_buffer: [u8; ferrowasp_mspv1::OSD_TX_BUFFER_LEN],
        osd_refresh_tick: u8,
        osd_tx_healthy: bool,
        flash_device: Spi2Flash,
        flash_record_producer: RecordProducer,
        flash_record_consumer: RecordConsumer,
        flash_command_producer: CommandProducer,
        flash_command_consumer: CommandConsumer,
        flash_response_producer: ResponseProducer,
        flash_response_consumer: ResponseConsumer,
        flash_manager_state: GoldenFlashState,
        usb_device: UsbCdcDevice,
        usb_serial: BufferedUsbCdcSerial,
        usb_header_sent: bool,
        usb_command_parser: CommandParser,
        usb_pending_response: Option<ResponseFrame>,
        io_watchdog: IoWatchdog,
        // ===== Safety channel `sbus_to_control` resources =====
        sbus_control_producer: SafetyProducer<'static, RcInputSnapshot>,
        sbus_control_consumer: SafetyConsumer<'static, RcInputSnapshot>,
        // ===== Safety channel `imu_to_control` resources =====
        imu_control_producer: SafetyProducer<'static, ImuData>,
        imu_control_consumer: SafetyConsumer<'static, ImuData>,
        // ===== Safety channel `control_to_actuator` resources =====
        motor_cmd_producer: SafetyProducer<'static, MotorCmd>,
        motor_cmd_consumer: SafetyConsumer<'static, MotorCmd>,
        // ===== Safety channel `control_to_safety_health` resources =====
        prearm_health_producer: SafetyProducer<'static, PreArmHealthReport>,
        prearm_health_consumer: SafetyConsumer<'static, PreArmHealthReport>,
        // ===== Safety channel `safety_to_actuator_guard` resources =====
        actuator_guard_producer: SafetyProducer<'static, ActuatorGuardReport>,
        actuator_guard_consumer: SafetyConsumer<'static, ActuatorGuardReport>,
        // ===== Safety channel `actuator_to_safety` resources =====
        actuator_completion_producer: SafetyProducer<'static, ActuatorPreparationReport>,
        actuator_completion_consumer: SafetyConsumer<'static, ActuatorPreparationReport>,
        // ===== Safety channel `actuator_fault_to_safety` resources =====
        actuator_fault_producer: SafetyProducer<'static, ActuatorPreparationReport>,
        actuator_fault_consumer: SafetyConsumer<'static, ActuatorPreparationReport>,
    }

    #[init(local = [
        // ===== Serial endpoint `serial1` resources =====
        serial1_rx_buffers: UartRxBufferBank = ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank(),
        serial1_rx_free_queue: UartRxFreeQueue = UartRxFreeQueue::new(),
        serial1_rx_filled_queue: UartRxFilledQueue = UartRxFilledQueue::new(),
        serial1_rx_channel: UartOwnedRxChannel = UartOwnedRxChannel::new(),
        serial1_tx_buffer: UartTxBuffer = [0; UART_TX_BUFFER_SIZE],
        serial1_tx_channel: UartOwnedTxChannel = UartOwnedTxChannel::new(),
        // ===== Serial endpoint `serial2` resources =====
        serial2_rx_buffers: UartRxBufferBank = ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank(),
        serial2_rx_free_queue: UartRxFreeQueue = UartRxFreeQueue::new(),
        serial2_rx_filled_queue: UartRxFilledQueue = UartRxFilledQueue::new(),
        serial2_rx_channel: UartOwnedRxChannel = UartOwnedRxChannel::new(),
        serial2_tx_buffer: UartTxBuffer = [0; UART_TX_BUFFER_SIZE],
        serial2_tx_channel: UartOwnedTxChannel = UartOwnedTxChannel::new(),
        // ===== SPI endpoint `spi1` resources =====
        spi1_dma_buffers: SpiDmaBufferBank = ferrowasp_stm32f4::app_storage::new_spi_dma_buffer_bank(),
        spi1_free_queue: SpiFreeQueue = SpiFreeQueue::new(),
        spi1_filled_queue: SpiFilledQueue = SpiFilledQueue::new(),
        // ===== Physical DShot actuator resources =====
        dshot_dma_storage: DshotDmaStorage = DshotDmaStorage::new(),
        esc_uart_rx_buffers: UartRxBufferBank = ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank(),
        esc_uart_free_queue: UartRxFreeQueue = UartRxFreeQueue::new(),
        esc_uart_filled_queue: UartRxFilledQueue = UartRxFilledQueue::new(),
        esc_request_queue: EscRequestQueue = EscRequestQueue::new(),
        esc_ack_queue: EscAckQueue = EscAckQueue::new(),
        esc_update_queue: EscTelemetryUpdateQueue = EscTelemetryUpdateQueue::new(),
        // ===== Golden ADC observation resources =====
        adc1_buffers: AdcBufferBank = ferrowasp_stm32f4::app_storage::new_adc_buffer_bank(),
        // ===== Golden SPI2 NOR and USB CDC resources =====
        flash_record_queue: RecordQueue = RecordQueue::new(),
        flash_command_queue: CommandQueue = CommandQueue::new(),
        flash_response_queue: ResponseQueue = ResponseQueue::new(),
        // ===== Safety channel `sbus_to_control` resources =====
        sbus_to_control: SafetyChannel<RcInputSnapshot, 5> = SafetyChannel::new(),
        // ===== Safety channel `imu_to_control` resources =====
        imu_to_control: SafetyChannel<ImuData, 5> = SafetyChannel::new(),
        // ===== Safety channel `control_to_actuator` resources =====
        control_to_actuator: SafetyChannel<MotorCmd, 4> = SafetyChannel::new(),
        // ===== Safety channel `control_to_safety_health` resources =====
        control_to_safety_health: SafetyChannel<PreArmHealthReport, 5> = SafetyChannel::new(),
        // ===== Safety channel `safety_to_actuator_guard` resources =====
        safety_to_actuator_guard: SafetyChannel<ActuatorGuardReport, 4> = SafetyChannel::new(),
        // ===== Safety channel `actuator_to_safety` resources =====
        actuator_to_safety: SafetyChannel<ActuatorPreparationReport, 4> = SafetyChannel::new(),
        // ===== Safety channel `actuator_fault_to_safety` resources =====
        actuator_fault_to_safety: SafetyChannel<ActuatorPreparationReport, 4> = SafetyChannel::new(),
    ])]
    fn init(cx: init::Context) -> (Shared, Local) {
        // ===== Common initialization =====
        let platform_config = load_platform_config();

        let mut rcc = ferrowasp_stm32f4::clocks::freeze_hse(
            cx.device.RCC.constrain(),
            8_000_000,
            SYSTEM_CLOCK_HZ,
            false,
        );

        Mono::start(rcc.clocks.timclk1().raw());

        let mut init_delay = cx.device.TIM5.delay::<1_000_000>(&mut rcc);

        // ===== Periodic control `control` initialization =====
        let control_scheduler = init_control_scheduler(cx.device.TIM4, &mut rcc, 800.Hz())
            .expect("validated periodic control timer must start");
        let control_phase = 0;

        // ===== Application-owned state for task `control_loop` initialization =====
        let control_loop_cnt = 0;
        let samples_per_control_loop = 2;
        let flight_controller = dt::FlightController::new(
            dt::FlightControllerConfig {
                max_throttle: 2_000.0,
                rescale_throttles: true,
                clamp_negative_to_zero: true,
            },
            dt::RateController::new(
                dt::RateControllerGains {
                    roll: dt::PidGains {
                        p: 2.5,
                        i: 0.0,
                        d: 0.0,
                    },
                    pitch: dt::PidGains {
                        p: 2.5,
                        i: 0.0,
                        d: 0.0,
                    },
                    yaw: dt::PidGains {
                        p: 2.0,
                        i: 0.0,
                        d: 0.0,
                    },
                },
                2_000.0,
            ),
        );
        let imu_rate_filter = dt::ImuRateLowPassFilter::new(0.55);
        let imu_angle_integrator = dt::GyroAngleIntegrator::new();
        let gyro_axis_map = ferrowasp_core::frames::BODY_RATE_TO_RATE_CONTROLLER_MAP;
        let gyro_bias_calibrator = dt::GyroBiasCalibrator::new(800, 1_000);
        let imu_last_sequence = 0;
        let imu_stale_ticks = 0;
        let applied_tuning_seq = 0;
        let motor_cmd_seq = 0;
        let rc_link_was_valid = false;
        let latest_rc_input = RcInputSnapshot::new();
        let rc_last_valid_frames = 0;
        let rc_stale_ticks = 0;
        let control_was_armed = false;
        // ===== Application-owned state for task `safety_master` initialization =====
        let foxeer_safety_state =
            ferrowasp_tasks::foxeer_safety::FoxeerSafetyMaster::output_inhibited();

        let gpioa = cx.device.GPIOA.split(&mut rcc);

        let gpiob = cx.device.GPIOB.split(&mut rcc);

        let gpioc = cx.device.GPIOC.split(&mut rcc);

        let mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);

        let mut exti = cx.device.EXTI;

        let dma1 = StreamsTuple::new(cx.device.DMA1, &mut rcc);

        let dma2 = StreamsTuple::new(cx.device.DMA2, &mut rcc);

        // ===== Serial endpoint `serial1` initialization =====
        let serial1_service = platform_config
            .serial1
            .expect("boot platform config must assign serial1");
        let serial1_parts = ferrowasp_stm32f4::uart_dma::init_usart2_endpoint(
            Usart2EndpointResources {
                tx_pin: gpioa.pa2,
                rx_pin: gpioa.pa3,
                usart: cx.device.USART2,
                rx_dma: dma1.5,
                tx_dma: dma1.6,
            },
            &mut rcc,
            serial1_service.profile().protocol,
            UartRxStorageResources {
                buffers: cx.local.serial1_rx_buffers,
                free_queue: cx.local.serial1_rx_free_queue,
                filled_queue: cx.local.serial1_rx_filled_queue,
            },
            cx.local.serial1_tx_buffer,
        );
        let serial1_rx = serial1_parts.rx_irq;
        let serial1_rx_parser = serial1_parts.parser;
        let (serial1_rx_producer, serial1_rx_reader, serial1_rx_discontinuities) =
            cx.local.serial1_rx_channel.split();
        let serial1_rx_bridge = UartOwnedRxBridge::new(serial1_rx_parser, serial1_rx_producer);
        let serial1_tx_dma = serial1_parts.tx_dma;
        let (serial1_tx_writer, serial1_tx_owner, serial1_tx_completion) =
            cx.local.serial1_tx_channel.split();

        // ===== Serial endpoint `serial2` initialization =====
        let serial2_service = platform_config
            .serial2
            .expect("boot platform config must assign serial2");
        let serial2_parts = ferrowasp_stm32f4::uart_dma::init_uart4_endpoint(
            Uart4EndpointResources {
                tx_pin: gpioa.pa0,
                rx_pin: gpioa.pa1,
                uart: cx.device.UART4,
                rx_dma: dma1.2,
                tx_dma: dma1.4,
            },
            &mut rcc,
            serial2_service.profile().protocol,
            UartRxStorageResources {
                buffers: cx.local.serial2_rx_buffers,
                free_queue: cx.local.serial2_rx_free_queue,
                filled_queue: cx.local.serial2_rx_filled_queue,
            },
            cx.local.serial2_tx_buffer,
        );
        let serial2_rx = serial2_parts.rx_irq;
        let serial2_rx_parser = serial2_parts.parser;
        let (serial2_rx_producer, serial2_rx_reader, serial2_rx_discontinuities) =
            cx.local.serial2_rx_channel.split();
        let serial2_rx_bridge = UartOwnedRxBridge::new(serial2_rx_parser, serial2_rx_producer);
        let serial2_tx_dma = serial2_parts.tx_dma;
        let (serial2_tx_writer, serial2_tx_owner, serial2_tx_completion) =
            cx.local.serial2_tx_channel.split();

        // ===== Boot-time serial service routing =====
        let (
            rc_sbus_reader,
            rc_sbus_rx_discontinuities,
            msp_v1_osd_reader,
            msp_v1_osd_rx_discontinuities,
            msp_v1_osd_writer,
        ) = match (platform_config.serial1, platform_config.serial2) {
            (Some(SerialService::RcSbus), Some(SerialService::MspV1Osd)) => (
                serial1_rx_reader,
                serial1_rx_discontinuities,
                serial2_rx_reader,
                serial2_rx_discontinuities,
                serial2_tx_writer,
            ),
            (Some(SerialService::MspV1Osd), Some(SerialService::RcSbus)) => (
                serial2_rx_reader,
                serial2_rx_discontinuities,
                serial1_rx_reader,
                serial1_rx_discontinuities,
                serial1_tx_writer,
            ),
            _ => panic!(
                "boot platform config must assign RcSbus and MspV1Osd to distinct serial endpoints"
            ),
        };

        // ===== Physical DShot actuator initialization =====
        ferrowasp_stm32f4::dshot::assert_foxeer_four_motor_dma_routes_compile();
        let esc_uart_parts = ferrowasp_stm32f4::uart_dma::init_usart1_esc_telemetry(
            Usart1EscTelemetryResources {
                rx_pin: gpioa.pa10,
                usart: cx.device.USART1,
                rx_dma: dma2.5,
            },
            &mut rcc,
            UartRxStorageResources {
                buffers: cx.local.esc_uart_rx_buffers,
                free_queue: cx.local.esc_uart_free_queue,
                filled_queue: cx.local.esc_uart_filled_queue,
            },
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
        .expect("reviewed Foxeer DShot600 timing must be valid");

        // ===== SPI endpoint `spi1` initialization =====
        match platform_config.spi1 {
            Some(SpiService::Imu(id)) if id == ImuInstallationId::new(1) => {}
            Some(SpiService::Imu(id)) => panic!(
                "boot platform config selected IMU installation {} but spi1 initialized installation 1",
                id.get()
            ),
            None => panic!("boot platform config must assign spi1"),
        }
        let mut spi1_cs = spi_dma::init_spi1_imu_cs(gpioa.pa4);
        let spi1_mode = spi::Mode {
            polarity: spi::Polarity::IdleHigh,
            phase: spi::Phase::CaptureOnSecondTransition,
        };
        let mut spi1_bus = spi_dma::init_spi1_bus(
            cx.device.SPI1,
            gpioa.pa5,
            gpioa.pa6,
            gpioa.pa7,
            spi1_mode,
            1_000_000,
            &mut rcc,
        );
        embedded_hal::delay::DelayNs::delay_ms(&mut init_delay, 10);
        let spi1_kind =
            match ferrowasp_drivers::icm42688p::read_who_am_i(&mut spi1_bus, &mut spi1_cs) {
                Ok(ferrowasp_drivers::mpu6500::WHO_AM_I_EXPECTED)
                    if ferrowasp_drivers::mpu6500::init(
                        &mut spi1_bus,
                        &mut spi1_cs,
                        &mut init_delay,
                    )
                    .is_ok() =>
                {
                    IMU_KIND_MPU6500
                }
                Ok(ferrowasp_drivers::icm42688p::WHO_AM_I_EXPECTED)
                    if ferrowasp_drivers::icm42688p::init(
                        &mut spi1_bus,
                        &mut spi1_cs,
                        &mut init_delay,
                    )
                    .is_ok() =>
                {
                    IMU_KIND_ICM42688P
                }
                Ok(identity) => {
                    defmt::warn!("unsupported SPI1 IMU identity: {=u8:02x}", identity);
                    IMU_KIND_NONE
                }
                Err(_) => {
                    defmt::warn!("SPI1 IMU probe failed");
                    IMU_KIND_NONE
                }
            };
        let spi1_dma = spi_dma::init_spi_dma::<_, _, _, 3, 3>(
            spi1_bus,
            dma2.0,
            dma2.3,
            SpiDmaStorageResources {
                buffers: cx.local.spi1_dma_buffers,
                free_queue: cx.local.spi1_free_queue,
                filled_queue: cx.local.spi1_filled_queue,
            }
            .into_backend(),
        );
        let spi_dma::SpiDmaParts {
            irq: spi1_irq,
            poller: spi1_poller,
            parser: spi1_parser_side,
            recovery_rx_buffer: spi1_recovery_rx_buffer,
        } = spi1_dma;
        let spi1_owner = Spi1ImuEndpointOwner::new(
            spi_dma::SpiDmaOwner::new(spi1_irq, spi1_poller, spi1_recovery_rx_buffer, spi1_cs),
            &SPI1_MAILBOX,
        );
        let spi1_parser = Spi1ImuParser::new(spi1_parser_side);
        let spi1_data_ready =
            ferrowasp_stm32f4::exti::init_input(gpioc.pc4, &mut syscfg, &mut exti, Edge::Rising);
        let spi1_device = Spi1ImuDevice::new(&SPI1_MAILBOX, || {
            let _ = spi1_owner_service::spawn();
        });
        let imu_sample = ImuData::default();
        let spi1_unavailable_logged = false;

        // ===== Golden ADC observation initialization =====
        let (adc1_primary_buffer, adc1_spare_buffer) = AdcStorageResources {
            buffers: cx.local.adc1_buffers,
        }
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
        let battery_cell_detector = ferrowasp_tasks::osd::BatteryCellDetector::new(4300, 3000, 8);
        let flight_service_telemetry = FlightServiceTelemetry::new();
        let tuning_profile = dt::TuningProfile::default_foxeer_f405_v2();
        let tuning_request_seq = 1;
        let osd_task_state = OsdTask::new();
        let osd_tx_buffer = [0; ferrowasp_mspv1::OSD_TX_BUFFER_LEN];
        let osd_refresh_tick = 0;
        let osd_tx_healthy = true;

        let mut flash_cs = gpiob.pb12.into_push_pull_output();
        let _ = embedded_hal::digital::OutputPin::set_high(&mut flash_cs);
        let flash_mode = ferrowasp_stm32f4::rtic::hal::spi::Mode {
            polarity: ferrowasp_stm32f4::rtic::hal::spi::Polarity::IdleLow,
            phase: ferrowasp_stm32f4::rtic::hal::spi::Phase::CaptureOnFirstTransition,
        };
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
        let storage_status = match flash_device.read_jedec_id() {
            Ok(id) => {
                let capacity_bytes = id.capacity_bytes().unwrap_or(0);
                StorageStatus {
                    ready: id.plausible() && StorageLayout::new(capacity_bytes).is_some(),
                    jedec: [id.manufacturer, id.memory_type, id.capacity_code],
                    capacity_bytes,
                    ..StorageStatus::default()
                }
            }
            Err(_) => {
                defmt::warn!("SPI2 flash JEDEC probe failed; storage disabled");
                StorageStatus::default()
            }
        };
        let (flash_record_producer, flash_record_consumer) = cx.local.flash_record_queue.split();
        let (flash_command_producer, flash_command_consumer) = cx.local.flash_command_queue.split();
        let (flash_response_producer, flash_response_consumer) =
            cx.local.flash_response_queue.split();
        let flash_manager_state = GoldenFlashState::new();
        let flash_log_rate_divisor = 1;

        let (usb_device, usb_serial) = ferrowasp_stm32f4::usb_serial::init_usb_cdc_serial(
            (
                cx.device.OTG_FS_GLOBAL,
                cx.device.OTG_FS_DEVICE,
                cx.device.OTG_FS_PWRCLK,
            ),
            (gpioa.pa11, gpioa.pa12),
            &rcc.clocks,
            UsbCdcIdentity {
                manufacturer: "FerroWasp",
                product: "FerroWasp Foxeer Debug",
                serial_number: "FW-FOX-F405V2",
            },
        )
        .expect("validated OTG_FS resources must initialize once");
        let usb_header_sent = false;
        let usb_command_parser = CommandParser::new();
        let usb_pending_response = None;
        let usb_status_due = false;
        let io_watchdog = ferrowasp_stm32f4::watchdog::init_io_watchdog(cx.device.TIM6, &mut rcc)
            .expect("validated TIM6 8 kHz watchdog must start");

        // ===== Safety channel `sbus_to_control` initialization =====
        let (sbus_control_producer, sbus_control_consumer) = cx.local.sbus_to_control.split();

        // ===== Safety channel `imu_to_control` initialization =====
        let (imu_control_producer, imu_control_consumer) = cx.local.imu_to_control.split();

        // ===== Safety channel `control_to_actuator` initialization =====
        let (motor_cmd_producer, motor_cmd_consumer) = cx.local.control_to_actuator.split();

        // ===== Safety channel `control_to_safety_health` initialization =====
        let (prearm_health_producer, prearm_health_consumer) =
            cx.local.control_to_safety_health.split();

        // ===== Safety channel `safety_to_actuator_guard` initialization =====
        let (actuator_guard_producer, actuator_guard_consumer) =
            cx.local.safety_to_actuator_guard.split();

        // ===== Safety channel `actuator_to_safety` initialization =====
        let (actuator_completion_producer, actuator_completion_consumer) =
            cx.local.actuator_to_safety.split();

        // ===== Safety channel `actuator_fault_to_safety` initialization =====
        let (actuator_fault_producer, actuator_fault_consumer) =
            cx.local.actuator_fault_to_safety.split();

        // ===== Initial task spawns =====
        safety_master::spawn().expect("init must spawn declared task safety_master");
        msp_osd::spawn().expect("init must spawn declared task msp_osd");
        imu_control_bridge::spawn().expect("init must spawn declared task imu_control_bridge");
        dshot_service::spawn().expect("init must spawn declared task dshot_service");
        esc_manager::spawn().expect("init must spawn declared task esc_manager");
        adc_observation_poll::spawn().expect("init must spawn declared task adc_observation_poll");
        golden_flash::spawn().expect("init must spawn declared task golden_flash");
        heartbeat::spawn().expect("init must spawn declared task heartbeat");
        serial1_tx_worker::spawn().expect("init must spawn declared task serial1_tx_worker");
        serial2_tx_worker::spawn().expect("init must spawn declared task serial2_tx_worker");

        // ===== RTIC resource handoff =====
        (
            Shared {
                // ===== Application resource `sbus_channel3`; tasks: safety_master =====
                sbus_channel3: 0,
                // ===== Serial endpoint `serial1` resources =====
                serial1_rx,
                serial1_tx_dma,
                // ===== Serial endpoint `serial2` resources =====
                serial2_rx,
                serial2_tx_dma,
                // ===== SPI endpoint `spi1` resources =====
                spi1_owner,
                spi1_kind,
                imu_sample,
                // ===== Physical DShot actuator resources =====
                dshot_motors,
                uart1_rx,
                esc_telemetry_discontinuity,
                // ===== Golden ADC observation resources =====
                adc1_transfer,
                flight_service_telemetry,
                tuning_profile,
                tuning_request_seq,
                flash_log_rate_divisor,
                storage_status,
                usb_status_due,
            },
            Local {
                // ===== Application-owned state for task `control_loop` =====
                control_loop_cnt,
                samples_per_control_loop,
                flight_controller,
                imu_rate_filter,
                imu_angle_integrator,
                gyro_axis_map,
                gyro_bias_calibrator,
                imu_last_sequence,
                imu_stale_ticks,
                applied_tuning_seq,
                motor_cmd_seq,
                rc_link_was_valid,
                latest_rc_input,
                rc_last_valid_frames,
                rc_stale_ticks,
                control_was_armed,
                // ===== Application-owned state for task `safety_master` =====
                foxeer_safety_state,
                // ===== Serial endpoint `serial1` resources =====
                serial1_rx_bridge,
                serial1_tx_owner,
                serial1_tx_completion,
                // ===== Serial endpoint `serial2` resources =====
                serial2_rx_bridge,
                serial2_tx_owner,
                serial2_tx_completion,
                // ===== Serial service `RcSbus` boot-routed resources =====
                rc_sbus_reader,
                rc_sbus_rx_discontinuities,
                // ===== Serial service `MspV1Osd` boot-routed resources =====
                msp_v1_osd_reader,
                msp_v1_osd_rx_discontinuities,
                msp_v1_osd_writer,
                // ===== SPI endpoint `spi1` resources =====
                spi1_parser,
                spi1_device,
                spi1_data_ready,
                spi1_unavailable_logged,
                // ===== Periodic control `control` task-local resources =====
                control_scheduler,
                control_phase,
                // ===== Physical DShot actuator resources =====
                esc_telemetry_uart,
                esc_request_producer,
                esc_request_consumer,
                esc_ack_producer,
                esc_ack_consumer,
                esc_telemetry_update_producer,
                esc_telemetry_update_consumer,
                esc_manager_state,
                esc_manager_report_ticks,
                esc_actuator_request,
                esc_actuator_request_submitted,
                dshot_fault_reported,
                // ===== Golden ADC observation resources =====
                adc1_buffer,
                adc1_planner,
                battery_cell_detector,
                osd_task_state,
                osd_tx_buffer,
                osd_refresh_tick,
                osd_tx_healthy,
                flash_device,
                flash_record_producer,
                flash_record_consumer,
                flash_command_producer,
                flash_command_consumer,
                flash_response_producer,
                flash_response_consumer,
                flash_manager_state,
                usb_device,
                usb_serial,
                usb_header_sent,
                usb_command_parser,
                usb_pending_response,
                io_watchdog,
                // ===== Safety channel `sbus_to_control` resources =====
                sbus_control_producer,
                sbus_control_consumer,
                // ===== Safety channel `imu_to_control` resources =====
                imu_control_producer,
                imu_control_consumer,
                // ===== Safety channel `control_to_actuator` resources =====
                motor_cmd_producer,
                motor_cmd_consumer,
                // ===== Safety channel `control_to_safety_health` resources =====
                prearm_health_producer,
                prearm_health_consumer,
                // ===== Safety channel `safety_to_actuator_guard` resources =====
                actuator_guard_producer,
                actuator_guard_consumer,
                // ===== Safety channel `actuator_to_safety` resources =====
                actuator_completion_producer,
                actuator_completion_consumer,
                // ===== Safety channel `actuator_fault_to_safety` resources =====
                actuator_fault_producer,
                actuator_fault_consumer,
            },
        )
    }

    /// Owns golden RC-link recovery and all arming/permit decisions.
    #[task(
        priority = 16,
        local = [rc_sbus_reader, rc_sbus_rx_discontinuities, foxeer_safety_state, sbus_control_producer, prearm_health_consumer, actuator_guard_producer, actuator_completion_consumer, actuator_fault_consumer],
        shared = [sbus_channel3, flight_service_telemetry, tuning_profile]
    )]
    async fn safety_master(mut cx: safety_master::Context) {
        loop {
            let mut bytes = [0; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES];
            let read =
                Mono::timeout_after(10.millis().into(), cx.local.rc_sbus_reader.read(&mut bytes))
                    .await;
            let now_us = Mono::now().duration_since_epoch().to_micros() as u32;

            let mut apply = |output: ferrowasp_tasks::foxeer_safety::SafetyMasterOutput| {
                if let Some(snapshot) = output.control_snapshot {
                    let tuning = cx.shared.tuning_profile.lock(|profile| *profile);
                    let mapped = dt::remap_rc_channels_with_profile(
                        snapshot.channels[0],
                        snapshot.channels[1],
                        snapshot.channels[3],
                        snapshot.channels[2],
                        tuning.sanitized().rc_rates,
                    );
                    cx.shared.flight_service_telemetry.lock(|telemetry| {
                        telemetry.rc_link_valid = true;
                        telemetry.rc_rates_dps = [
                            mapped.roll_dps as i16,
                            mapped.pitch_dps as i16,
                            mapped.yaw_dps as i16,
                        ];
                        telemetry.rc_throttle = mapped.throttle;
                    });
                    if cx.local.sbus_control_producer.try_send(snapshot).is_err() {
                        defmt::warn!("safety-to-control RC channel full; sample rejected");
                    }
                }
                if let Some(value) = output.channel3 {
                    cx.shared
                        .sbus_channel3
                        .lock(|channel| *channel = u32::from(value));
                }
                if let Some(request) = output.actuator
                    && actuator_output::spawn(request).is_err()
                {
                    defmt::warn!("safety actuator request rejected");
                }
                if output.event.is_some() {
                    defmt::info!("safety-master state transition");
                }
            };

            while let Some(report) = cx.local.prearm_health_consumer.try_receive() {
                apply(
                    cx.local
                        .foxeer_safety_state
                        .observe_control_health(report, now_us),
                );
            }

            while let Some(report) = cx.local.actuator_completion_consumer.try_receive() {
                apply(cx.local.foxeer_safety_state.actuator_report(report, now_us));
            }
            while let Some(report) = cx.local.actuator_fault_consumer.try_receive() {
                apply(cx.local.foxeer_safety_state.actuator_report(report, now_us));
            }

            if let Some(discontinuity) = cx.local.rc_sbus_rx_discontinuities.take_new() {
                let reason = if matches!(
                    discontinuity.cause,
                    ferrowasp_io_core::serial::Discontinuity::DmaError
                ) {
                    RcLinkInvalidation::DmaError
                } else {
                    RcLinkInvalidation::TransportDiscontinuity
                };
                apply(cx.local.foxeer_safety_state.transport_fault(reason));
            }

            match read {
                Ok(Ok(read_len)) => cx.local.foxeer_safety_state.consume_bytes(
                    &bytes[..read_len],
                    now_us,
                    &mut apply,
                ),
                Ok(Err(_)) => {
                    apply(
                        cx.local
                            .foxeer_safety_state
                            .transport_fault(RcLinkInvalidation::TransportDiscontinuity),
                    );
                    defmt::warn!("SBUS reader stopped after a transport fault");
                    return;
                }
                Err(_) => apply(cx.local.foxeer_safety_state.poll(now_us)),
            }

            drop(apply);
            let link = cx.local.foxeer_safety_state.link_status(now_us);
            let armed = matches!(
                cx.local.foxeer_safety_state.arm_state(),
                ferrowasp_core::safety::ArmingState::Armed
            );
            cx.shared.flight_service_telemetry.lock(|telemetry| {
                telemetry.rc_link_valid = link.valid;
                telemetry.armed = armed;
                if !link.valid {
                    telemetry.rc_rates_dps = [0; 3];
                    telemetry.rc_throttle = 0;
                }
            });

            let guard = cx.local.foxeer_safety_state.actuator_guard_report(now_us);
            if guard.authority != ActuatorAuthority::Inhibited
                && cx.local.actuator_guard_producer.try_send(guard).is_err()
            {
                let _ = cx
                    .local
                    .foxeer_safety_state
                    .actuator_report(ActuatorPreparationReport::Faulted, now_us);
                defmt::warn!("safety-to-actuator authority channel full");
            }
        }
    }

    /// Services MSP DisplayPort replies, telemetry, and the disarmed tuning menu.
    #[task(
        priority = 3,
        local = [msp_v1_osd_reader, msp_v1_osd_rx_discontinuities, msp_v1_osd_writer, osd_task_state, osd_tx_buffer, osd_refresh_tick, osd_tx_healthy],
        shared = [flight_service_telemetry, tuning_profile, tuning_request_seq]
    )]
    async fn msp_osd(mut cx: msp_osd::Context) {
        loop {
            let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
            let service_snapshot = cx
                .shared
                .flight_service_telemetry
                .lock(|telemetry| *telemetry);
            let telemetry = service_snapshot.msp_snapshot(now_ms);

            let mut bytes = [0_u8; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES];
            match Mono::timeout_after(
                10.millis().into(),
                cx.local.msp_v1_osd_reader.read(&mut bytes),
            )
            .await
            {
                Ok(Ok(read_len)) => {
                    // At most one reply is transmitted per service iteration. Remaining bytes are
                    // still consumed by the parser and may advance an incomplete packet.
                    let mut replied = false;
                    for byte in &bytes[..read_len] {
                        if let Some(frame_len) = cx.local.osd_task_state.ingest_byte(
                            *byte,
                            &telemetry,
                            cx.local.osd_tx_buffer,
                        ) && !replied
                        {
                            replied = true;
                            if *cx.local.osd_tx_healthy
                                && cx
                                    .local
                                    .msp_v1_osd_writer
                                    .write_all(&cx.local.osd_tx_buffer[..frame_len])
                                    .await
                                    .is_err()
                            {
                                *cx.local.osd_tx_healthy = false;
                                defmt::warn!("MSP DisplayPort reply transport fault");
                            }
                        }
                    }
                }
                Ok(Err(_)) => {
                    *cx.local.osd_tx_healthy = false;
                    defmt::warn!("MSP DisplayPort RX transport fault");
                }
                Err(_) => {}
            }

            if cx.local.msp_v1_osd_rx_discontinuities.take_new().is_some() {
                defmt::warn!("MSP DisplayPort RX discontinuity");
            }

            let menu_active = cx.shared.tuning_profile.lock(|tuning| {
                let before = *tuning;
                let active = cx.local.osd_task_state.update_menu(
                    service_snapshot.armed,
                    OsdStickRates {
                        roll: service_snapshot.rc_rates_dps[0],
                        pitch: service_snapshot.rc_rates_dps[1],
                        yaw: service_snapshot.rc_rates_dps[2],
                    },
                    service_snapshot.rc_throttle,
                    tuning,
                );
                if before != *tuning {
                    cx.shared
                        .tuning_request_seq
                        .lock(|sequence| *sequence = sequence.wrapping_add(1).max(1));
                }
                active
            });

            *cx.local.osd_refresh_tick = cx.local.osd_refresh_tick.wrapping_add(1);
            if *cx.local.osd_refresh_tick >= 10 {
                *cx.local.osd_refresh_tick = 0;

                if *cx.local.osd_tx_healthy
                    && let Some(frame_len) = cx
                        .local
                        .osd_task_state
                        .heartbeat_frame(cx.local.osd_tx_buffer)
                    && cx
                        .local
                        .msp_v1_osd_writer
                        .write_all(&cx.local.osd_tx_buffer[..frame_len])
                        .await
                        .is_err()
                {
                    *cx.local.osd_tx_healthy = false;
                    defmt::warn!("MSP DisplayPort heartbeat transport fault");
                }

                let frame_len = if menu_active {
                    let tuning = cx.shared.tuning_profile.lock(|profile| *profile);
                    cx.local
                        .osd_task_state
                        .next_menu_frame(&tuning, cx.local.osd_tx_buffer)
                } else {
                    cx.local
                        .osd_task_state
                        .next_overlay_frame(&telemetry, cx.local.osd_tx_buffer)
                };
                if *cx.local.osd_tx_healthy
                    && let Some(frame_len) = frame_len
                    && cx
                        .local
                        .msp_v1_osd_writer
                        .write_all(&cx.local.osd_tx_buffer[..frame_len])
                        .await
                        .is_err()
                {
                    *cx.local.osd_tx_healthy = false;
                    defmt::warn!("MSP DisplayPort overlay transport fault");
                }
            }
        }
    }

    /// Forwards each newly decoded IMU sample into the control safety channel.
    #[task(priority = 11, local = [imu_control_producer], shared = [imu_sample])]
    async fn imu_control_bridge(mut cx: imu_control_bridge::Context) {
        let mut forwarded_sequence = 0_u32;
        loop {
            let sample = cx.shared.imu_sample.lock(|sample| ImuData {
                acc: sample.acc,
                gyro: sample.gyro,
                gyro_raw: sample.gyro_raw,
                temp: sample.temp,
                sequence: sample.sequence,
            });
            if sample.sequence != forwarded_sequence {
                forwarded_sequence = sample.sequence;
                if cx.local.imu_control_producer.try_send(sample).is_err() {
                    defmt::warn!("IMU-to-control safety channel full; rejected newest sample");
                }
            }

            Mono::delay(1.millis().into()).await;
        }
    }

    /// Sole adapter allowed to translate fresh safety-approved requests into DShot commands.
    #[task(
        priority = 15,
        local = [motor_cmd_consumer, actuator_guard_consumer, actuator_completion_producer, esc_telemetry_update_consumer],
        shared = [dshot_motors]
    )]
    async fn actuator_output(
        mut cx: actuator_output::Context,
        request: ferrowasp_core::safety::ActuatorCmd,
    ) {
        let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
        if !false {
            cx.shared.dshot_motors.lock(|bank| bank.command_stop());
            let _ =
                ferrowasp_tasks::actuator::handle_inhibited_actuator_wake(request, now_ms, || {
                    cx.local.motor_cmd_consumer.try_receive()
                });
            if matches!(request, ActuatorCmd::EnterIdle) {
                let report = ActuatorPreparationReport::Aborted(ArmingAbortReason::PermitRevoked);
                let _ = cx.local.actuator_completion_producer.try_send(report);
            }
            return;
        }

        match request {
            ActuatorCmd::Disarm | ActuatorCmd::Calibrate => {
                for _ in 0..ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY {
                    if cx.local.motor_cmd_consumer.try_receive().is_none() {
                        break;
                    }
                }
                cx.shared.dshot_motors.lock(|bank| bank.command_stop());
            }
            ActuatorCmd::ApplyLatestThrottle => {
                let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
                let now_ms = now_us / 1_000;
                let command = ferrowasp_tasks::actuator::take_physical_motor_command(
                    now_us,
                    now_ms,
                    || cx.local.actuator_guard_consumer.try_receive(),
                    || cx.local.motor_cmd_consumer.try_receive(),
                );
                match command {
                    Ok(values) => {
                        let applied = cx.shared.dshot_motors.lock(|bank| {
                            bank.command_throttles(values, now_ms, MOTOR_CMD_MAX_AGE_MS)
                        });
                        if applied.is_err() {
                            cx.shared.dshot_motors.lock(|bank| bank.command_stop());
                            let _ =
                                actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                        }
                    }
                    Err(_) => {
                        cx.shared.dshot_motors.lock(|bank| bank.command_stop());
                        let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                    }
                }
            }
            ActuatorCmd::EnterIdle => {
                cx.shared.dshot_motors.lock(|bank| bank.command_stop());
                let stop_iterations = DSHOT_PREARM_STOP_HOLD_MS / 10;
                let mut abort_reason = None;
                for _ in 0..stop_iterations {
                    let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
                    if ferrowasp_tasks::actuator::take_latest_guard(
                        now_us,
                        ActuatorAuthority::Preparing,
                        || cx.local.actuator_guard_consumer.try_receive(),
                    )
                    .is_err()
                    {
                        abort_reason = Some(ArmingAbortReason::PermitRevoked);
                        break;
                    }
                    cx.shared.dshot_motors.lock(|bank| bank.command_stop());
                    Mono::delay(10.millis().into()).await;
                }

                for _ in 0..ferrowasp_tasks::esc_manager::ESC_TELEMETRY_UPDATE_QUEUE_CAPACITY {
                    if cx.local.esc_telemetry_update_consumer.dequeue().is_none() {
                        break;
                    }
                }
                let started_ms = Mono::now().duration_since_epoch().to_millis() as u32;
                let mut qualification =
                    EscIdleQualification::new(DSHOT_IDLE_QUALIFICATION_CONFIG, started_ms);
                while abort_reason.is_none() {
                    let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
                    let now_ms = now_us / 1_000;
                    if ferrowasp_tasks::actuator::take_latest_guard(
                        now_us,
                        ActuatorAuthority::Preparing,
                        || cx.local.actuator_guard_consumer.try_receive(),
                    )
                    .is_err()
                    {
                        abort_reason = Some(ArmingAbortReason::PermitRevoked);
                        break;
                    }
                    let idle = [DSHOT_IDLE_THROTTLE_COMMAND; 4];
                    if cx
                        .shared
                        .dshot_motors
                        .lock(|bank| bank.command_throttles(idle, now_ms, MOTOR_CMD_MAX_AGE_MS))
                        .is_err()
                    {
                        abort_reason = Some(ArmingAbortReason::CompletionDeliveryFailed);
                        break;
                    }

                    let mut status = EscIdleQualificationStatus::Pending;
                    for _ in 0..ferrowasp_tasks::esc_manager::ESC_TELEMETRY_UPDATE_QUEUE_CAPACITY {
                        let Some(update) = cx.local.esc_telemetry_update_consumer.dequeue() else {
                            break;
                        };
                        status = qualification.observe(update, now_ms);
                    }
                    if matches!(status, EscIdleQualificationStatus::Pending) {
                        status = qualification.status(now_ms);
                    }
                    match status {
                        EscIdleQualificationStatus::Pending => {}
                        EscIdleQualificationStatus::Qualified => break,
                        EscIdleQualificationStatus::Failed(
                            EscIdleQualificationFailure::Overspeed { .. },
                        ) => abort_reason = Some(ArmingAbortReason::EscIdleRpmOutOfRange),
                        EscIdleQualificationStatus::Failed(
                            EscIdleQualificationFailure::Timeout { .. },
                        ) => abort_reason = Some(ArmingAbortReason::EscIdleTelemetryTimeout),
                        EscIdleQualificationStatus::Failed(
                            EscIdleQualificationFailure::InvalidConfig,
                        ) => abort_reason = Some(ArmingAbortReason::EscIdleQualificationInvalid),
                    }
                    if abort_reason.is_none()
                        && !matches!(status, EscIdleQualificationStatus::Qualified)
                    {
                        Mono::delay(10.millis().into()).await;
                    } else {
                        break;
                    }
                }

                let report = match abort_reason {
                    Some(reason) => {
                        cx.shared.dshot_motors.lock(|bank| bank.command_stop());
                        ActuatorPreparationReport::Aborted(reason)
                    }
                    None => ActuatorPreparationReport::Qualified,
                };
                if cx
                    .local
                    .actuator_completion_producer
                    .try_send(report)
                    .is_err()
                {
                    cx.shared.dshot_motors.lock(|bank| bank.command_stop());
                    let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                }
            }
        }
    }

    /// Publishes one physical-service fault through its sole bounded producer.
    #[task(priority = 16, local = [actuator_fault_producer])]
    async fn actuator_fault_reporter(
        cx: actuator_fault_reporter::Context,
        report: ferrowasp_core::safety::ActuatorPreparationReport,
    ) {
        if cx.local.actuator_fault_producer.try_send(report).is_err() {
            defmt::warn!("actuator fault channel full; output remains fail-closed");
        }
    }

    /// Services one exact physical-lane completion interrupt.
    #[task(binds = DMA2_STREAM1, priority = 16, shared = [dshot_motors])]
    fn dshot_motor1_dma_complete(mut cx: dshot_motor1_dma_complete::Context) {
        let motor = match 1 {
            1 => DshotMotor::Motor1,
            2 => DshotMotor::Motor2,
            3 => DshotMotor::Motor3,
            4 => DshotMotor::Motor4,
            _ => {
                let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                return;
            }
        };
        let event = cx
            .shared
            .dshot_motors
            .lock(|bank| bank.on_dma_interrupt(motor));
        if !matches!(event, DshotInterruptEvent::Completed)
            && actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted).is_err()
        {
            defmt::warn!("DShot DMA fault report rejected");
        }
    }

    /// Services one exact physical-lane completion interrupt.
    #[task(binds = DMA2_STREAM7, priority = 16, shared = [dshot_motors])]
    fn dshot_motor2_dma_complete(mut cx: dshot_motor2_dma_complete::Context) {
        let motor = match 2 {
            1 => DshotMotor::Motor1,
            2 => DshotMotor::Motor2,
            3 => DshotMotor::Motor3,
            4 => DshotMotor::Motor4,
            _ => {
                let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                return;
            }
        };
        let event = cx
            .shared
            .dshot_motors
            .lock(|bank| bank.on_dma_interrupt(motor));
        if !matches!(event, DshotInterruptEvent::Completed)
            && actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted).is_err()
        {
            defmt::warn!("DShot DMA fault report rejected");
        }
    }

    /// Services one exact physical-lane completion interrupt.
    #[task(binds = DMA2_STREAM2, priority = 16, shared = [dshot_motors])]
    fn dshot_motor3_dma_complete(mut cx: dshot_motor3_dma_complete::Context) {
        let motor = match 3 {
            1 => DshotMotor::Motor1,
            2 => DshotMotor::Motor2,
            3 => DshotMotor::Motor3,
            4 => DshotMotor::Motor4,
            _ => {
                let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                return;
            }
        };
        let event = cx
            .shared
            .dshot_motors
            .lock(|bank| bank.on_dma_interrupt(motor));
        if !matches!(event, DshotInterruptEvent::Completed)
            && actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted).is_err()
        {
            defmt::warn!("DShot DMA fault report rejected");
        }
    }

    /// Services one exact physical-lane completion interrupt.
    #[task(binds = DMA2_STREAM6, priority = 16, shared = [dshot_motors])]
    fn dshot_motor4_dma_complete(mut cx: dshot_motor4_dma_complete::Context) {
        let motor = match 4 {
            1 => DshotMotor::Motor1,
            2 => DshotMotor::Motor2,
            3 => DshotMotor::Motor3,
            4 => DshotMotor::Motor4,
            _ => {
                let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                return;
            }
        };
        let event = cx
            .shared
            .dshot_motors
            .lock(|bank| bank.on_dma_interrupt(motor));
        if !matches!(event, DshotInterruptEvent::Completed)
            && actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted).is_err()
        {
            defmt::warn!("DShot DMA fault report rejected");
        }
    }

    /// Services DShot frames, command leases, and telemetry request acknowledgement.
    #[task(
        priority = 13,
        local = [esc_request_consumer, esc_ack_producer, esc_actuator_request, esc_actuator_request_submitted, dshot_fault_reported],
        shared = [dshot_motors]
    )]
    async fn dshot_service(mut cx: dshot_service::Context) {
        loop {
            if !false {
                // Deliberately do not call `service`: even stop frames remain disabled.
                Mono::delay(2.millis().into()).await;
                continue;
            }
            let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
            if cx.local.esc_actuator_request.is_none() {
                *cx.local.esc_actuator_request = cx.local.esc_request_consumer.dequeue();
                *cx.local.esc_actuator_request_submitted = false;
            }

            let (event, sent) = cx.shared.dshot_motors.lock(|bank| {
                if let Some(request) = *cx.local.esc_actuator_request
                    && !*cx.local.esc_actuator_request_submitted
                {
                    let motor = match request.output {
                        EscOutput::Output1 => DshotMotor::Motor1,
                        EscOutput::Output2 => DshotMotor::Motor2,
                        EscOutput::Output3 => DshotMotor::Motor3,
                        EscOutput::Output4 => DshotMotor::Motor4,
                    };
                    match bank.request_telemetry(motor) {
                        Ok(()) => *cx.local.esc_actuator_request_submitted = true,
                        Err(DshotTelemetryRequestError::Busy) => {}
                        Err(DshotTelemetryRequestError::Faulted) => {
                            *cx.local.esc_actuator_request = None;
                        }
                    }
                }
                let event = bank.service(now_ms);
                (event, bank.take_telemetry_request_sent())
            });

            if let (Some(request), Some(sent_motor)) = (*cx.local.esc_actuator_request, sent) {
                let expected = match request.output {
                    EscOutput::Output1 => DshotMotor::Motor1,
                    EscOutput::Output2 => DshotMotor::Motor2,
                    EscOutput::Output3 => DshotMotor::Motor3,
                    EscOutput::Output4 => DshotMotor::Motor4,
                };
                if sent_motor == expected {
                    let _ = cx.local.esc_ack_producer.enqueue(EscActuatorAck {
                        request,
                        started_at_ms: now_ms,
                    });
                } else {
                    let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                }
                *cx.local.esc_actuator_request = None;
                *cx.local.esc_actuator_request_submitted = false;
            }

            let terminal = matches!(event, DshotServiceEvent::LeaseExpired)
                || matches!(event, DshotServiceEvent::Faulted) && !*cx.local.dshot_fault_reported;
            if terminal {
                if matches!(event, DshotServiceEvent::Faulted) {
                    *cx.local.dshot_fault_reported = true;
                }
                if actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted).is_err() {
                    defmt::warn!("DShot terminal fault report rejected");
                }
            }
            Mono::delay(2.millis().into()).await;
        }
    }

    /// Services the exact USART1 RX DMA stream.
    #[task(
        binds = DMA2_STREAM5,
        priority = 5,
        shared = [uart1_rx, esc_telemetry_discontinuity]
    )]
    fn esc_uart_rx_dma(mut cx: esc_uart_rx_dma::Context) {
        let outcome = cx.shared.uart1_rx.lock(|uart| uart.service_dma_irq());
        if matches!(
            outcome,
            UartRxIrqOutcome::DmaError | UartRxIrqOutcome::DeliveryError(_)
        ) {
            cx.shared
                .esc_telemetry_discontinuity
                .lock(|flag| *flag = true);
            defmt::warn!("USART1 ESC telemetry RX DMA discontinuity");
        }
    }

    /// Services USART1 IDLE chunk completion.
    #[task(
        binds = USART1,
        priority = 5,
        shared = [uart1_rx, esc_telemetry_discontinuity]
    )]
    fn esc_uart_rx_idle(mut cx: esc_uart_rx_idle::Context) {
        let outcome = cx.shared.uart1_rx.lock(|uart| uart.service_idle_irq());
        if matches!(
            outcome,
            UartRxIrqOutcome::DmaError | UartRxIrqOutcome::DeliveryError(_)
        ) {
            cx.shared
                .esc_telemetry_discontinuity
                .lock(|flag| *flag = true);
            defmt::warn!("USART1 ESC telemetry RX IDLE discontinuity");
        }
    }

    /// Parses bounded legacy telemetry and maintains exact request association.
    #[task(
        priority = 4,
        local = [esc_telemetry_uart, esc_manager_state, esc_request_producer, esc_ack_consumer, esc_telemetry_update_producer, esc_manager_report_ticks],
        shared = [esc_telemetry_discontinuity]
    )]
    async fn esc_manager(mut cx: esc_manager::Context) {
        loop {
            let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
            let had_discontinuity = cx.shared.esc_telemetry_discontinuity.lock(|flag| {
                let value = *flag;
                *flag = false;
                value
            });
            if had_discontinuity {
                cx.local.esc_manager_state.record_wire_discontinuity();
            }

            for _ in 0..ferrowasp_tasks::esc_manager::ESC_ACK_QUEUE_CAPACITY {
                let Some(ack) = cx.local.esc_ack_consumer.dequeue() else {
                    break;
                };
                if let EscAckOutcome::Sample(update) =
                    cx.local.esc_manager_state.on_actuator_ack(ack)
                {
                    let _ = cx.local.esc_telemetry_update_producer.enqueue(update);
                }
            }

            let mut bytes = [0; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES];
            for _ in 0..ferrowasp_stm32f4::memory::UART_RX_BUFFERS_PER_PORT {
                match cx
                    .local
                    .esc_telemetry_uart
                    .read_chunk_with_status(&mut bytes)
                {
                    UartRxReadStatus::NoChunk => break,
                    UartRxReadStatus::RecycleError => {
                        cx.local.esc_manager_state.record_wire_discontinuity();
                        let _ = actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted);
                        break;
                    }
                    UartRxReadStatus::Chunk {
                        len,
                        uart_error_seen,
                    } => {
                        if uart_error_seen {
                            cx.local.esc_manager_state.record_wire_discontinuity();
                        }
                        for byte in &bytes[..len] {
                            if let Some(update) =
                                cx.local.esc_manager_state.push_wire_byte(*byte, now_ms)
                            {
                                let _ = cx.local.esc_telemetry_update_producer.enqueue(update);
                            }
                        }
                    }
                }
            }

            cx.local.esc_manager_state.refresh_wire_stats();
            if cx.local.esc_manager_state.poll_timeout(now_ms).is_some()
                && actuator_fault_reporter::spawn(ActuatorPreparationReport::Faulted).is_err()
            {
                defmt::warn!("ESC manager timeout fault report rejected");
            }
            if false
                && let Some(request) = cx.local.esc_manager_state.next_request(now_ms)
                && cx.local.esc_request_producer.enqueue(request).is_ok()
            {
                let _ = cx
                    .local
                    .esc_manager_state
                    .mark_request_queued(request, now_ms);
            }

            *cx.local.esc_manager_report_ticks = cx.local.esc_manager_report_ticks.wrapping_add(1);
            Mono::delay(2.millis().into()).await;
        }
    }

    /// Starts one ADC1 voltage/current observation at each bounded interval.
    #[task(priority = 1, shared = [adc1_transfer])]
    async fn adc_observation_poll(mut cx: adc_observation_poll::Context) {
        loop {
            cx.shared
                .adc1_transfer
                .lock(|transfer| transfer.start(|adc| adc.start_conversion()));
            Mono::delay(100.millis().into()).await;
        }
    }

    /// Accepts one complete ADC1 DMA sample and publishes freshness-tracked telemetry.
    #[task(
        binds = DMA2_STREAM4,
        priority = 1,
        local = [adc1_buffer, adc1_planner, battery_cell_detector],
        shared = [adc1_transfer, flight_service_telemetry]
    )]
    fn adc_observation_dma(mut cx: adc_observation_dma::Context) {
        let sample = cx.shared.adc1_transfer.lock(|transfer| {
            take_completed_adc1_sample_for(transfer, cx.local.adc1_buffer, cx.local.adc1_planner)
        });
        let sample = match sample {
            Ok(Some(sample)) => sample,
            Ok(None) => return,
            Err(
                AdcDmaDeliveryError::DmaFault
                | AdcDmaDeliveryError::NoSpareBuffer
                | AdcDmaDeliveryError::TransferNotReady,
            ) => {
                cx.shared
                    .flight_service_telemetry
                    .lock(|telemetry| telemetry.battery.record_fault());
                defmt::warn!("ADC1 observation rejected after DMA delivery fault");
                return;
            }
        };

        let pack_mv = u32::from(sample.voltage_mv).saturating_mul(110) / 10;
        let cell_count = cx.local.battery_cell_detector.update(pack_mv);
        let cell_voltage_centivolts =
            ferrowasp_tasks::osd::pack_millivolts_to_cell_centivolts(pack_mv, cell_count);
        let current_centiamps = if cell_count == 0 {
            0
        } else {
            ferrowasp_tasks::osd::current_sample_to_centiamps_with_offset(
                u32::from(sample.current_mv),
                70,
                0 as i32,
            )
        };
        let voltage_decivolts = ((pack_mv + 50) / 100).min(u32::from(u8::MAX)) as u8;
        let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;

        cx.shared.flight_service_telemetry.lock(|telemetry| {
            telemetry.battery.observe(
                voltage_decivolts,
                cell_count,
                cell_voltage_centivolts,
                current_centiamps,
                sample.voltage_mv,
                sample.current_mv,
                now_ms,
            );
        });

        // Return the completed buffer before the next polling conversion.
        *cx.local.adc1_buffer = Some(sample.buffer);
    }

    /// Owns bounded append-only blackbox and copy-on-write configuration storage.
    #[task(
        priority = 1,
        local = [flash_device, flash_record_consumer, flash_command_consumer, flash_response_producer, flash_manager_state],
        shared = [flight_service_telemetry, tuning_profile, tuning_request_seq, flash_log_rate_divisor, storage_status]
    )]
    async fn golden_flash(mut cx: golden_flash::Context) {
        loop {
            let storage_status = cx.shared.storage_status.lock(|status| *status);
            let mut queue_response = |text: &str| {
                ResponseFrame::from_text(text)
                    .is_some_and(|frame| cx.local.flash_response_producer.enqueue(frame).is_ok())
            };

            if !storage_status.ready {
                if cx.local.flash_command_consumer.dequeue().is_some() {
                    let _ = queue_response("ERR storage unavailable\r\n");
                }
                Mono::delay(1.millis().into()).await;
                continue;
            }
            let Some(layout) = StorageLayout::new(storage_status.capacity_bytes) else {
                cx.shared.storage_status.lock(|status| {
                    status.ready = false;
                    status.faults = status.faults.saturating_add(1);
                });
                Mono::delay(1.millis().into()).await;
                continue;
            };

            if !cx.local.flash_manager_state.initialized {
                let recovered_log = scan_log(layout, |address, page| {
                    cx.local.flash_device.read(address, page)
                });
                let recovered_config = load_config(
                    layout,
                    ferrowasp_tasks::flash_storage::StoredConfig::foxeer_f405_v2_default(),
                    |address, page| cx.local.flash_device.read(address, page),
                );
                match (recovered_log, recovered_config) {
                    (Ok((next_page, next_flight, writable)), Ok((config, sequence, slot))) => {
                        cx.local.flash_manager_state.layout = Some(layout);
                        cx.local.flash_manager_state.next_page = next_page;
                        cx.local.flash_manager_state.next_flight = next_flight;
                        cx.local.flash_manager_state.log_writable = writable;
                        cx.local.flash_manager_state.config = config;
                        cx.local.flash_manager_state.config_sequence = sequence;
                        cx.local.flash_manager_state.config_active_slot = slot;
                        cx.local.flash_manager_state.initialized = true;
                        cx.shared
                            .tuning_profile
                            .lock(|profile| *profile = config.tuning);
                        cx.shared.flash_log_rate_divisor.lock(|divisor| {
                            *divisor = u32::from(config.log_rate_divisor).clamp(1, 16)
                        });
                        cx.shared
                            .tuning_request_seq
                            .lock(|seq| *seq = seq.wrapping_add(1).max(1));
                        cx.shared.storage_status.lock(|status| {
                            status.next_page = next_page;
                            status.next_flight = next_flight;
                        });
                    }
                    _ => {
                        cx.shared.storage_status.lock(|status| {
                            status.ready = false;
                            status.faults = status.faults.saturating_add(1);
                        });
                        defmt::warn!("SPI2 flash recovery failed; storage disabled");
                    }
                }
                Mono::delay(1.millis().into()).await;
                continue;
            }

            let status = match cx.local.flash_device.read_status() {
                Ok(status) => status,
                Err(_) => {
                    cx.shared.storage_status.lock(|status| {
                        status.ready = false;
                        status.faults = status.faults.saturating_add(1);
                    });
                    defmt::warn!("SPI2 flash status read failed; storage disabled");
                    Mono::delay(1.millis().into()).await;
                    continue;
                }
            };
            if status.busy() {
                Mono::delay(1.millis().into()).await;
                continue;
            }

            let armed = cx
                .shared
                .flight_service_telemetry
                .lock(|telemetry| telemetry.armed);
            if armed && cx.local.flash_manager_state.maintenance_busy() {
                cx.local.flash_manager_state.operation = GoldenFlashOperation::Idle;
                let _ = queue_response("ERR maintenance aborted because system armed\r\n");
            }

            match cx.local.flash_manager_state.operation {
                GoldenFlashOperation::EraseLogs { next_sector } => {
                    if next_sector >= layout.log_sector_count() {
                        cx.local.flash_manager_state.operation = GoldenFlashOperation::Idle;
                        cx.local.flash_manager_state.next_page = 0;
                        cx.local.flash_manager_state.next_flight = 1;
                        cx.local.flash_manager_state.log_writable = true;
                        cx.shared.storage_status.lock(|status| {
                            status.next_page = 0;
                            status.next_flight = 1;
                        });
                        let _ = queue_response("OK logs erased\r\n");
                    } else {
                        let address = layout.log_start_address
                            + next_sector * ferrowasp_tasks::flash_storage::CONFIG_SECTOR_SIZE;
                        if cx.local.flash_device.erase_sector_4k(address).is_ok() {
                            cx.local.flash_manager_state.operation =
                                GoldenFlashOperation::EraseLogs {
                                    next_sector: next_sector + 1,
                                };
                        } else {
                            cx.local.flash_manager_state.operation = GoldenFlashOperation::Idle;
                            cx.shared
                                .storage_status
                                .lock(|status| status.faults = status.faults.saturating_add(1));
                            let _ = queue_response("ERR log sector erase failed\r\n");
                        }
                    }
                    Mono::delay(1.millis().into()).await;
                    continue;
                }
                GoldenFlashOperation::SaveConfigErase { slot } => {
                    let address = layout.config_slot_addresses[slot as usize];
                    if cx
                        .local
                        .flash_device
                        .page_program(address, &cx.local.flash_manager_state.pending_config_page)
                        .is_ok()
                    {
                        cx.local.flash_manager_state.operation =
                            GoldenFlashOperation::SaveConfigProgram { slot };
                    } else {
                        cx.local.flash_manager_state.operation = GoldenFlashOperation::Idle;
                        cx.shared
                            .storage_status
                            .lock(|status| status.faults = status.faults.saturating_add(1));
                        let _ = queue_response("ERR config page program failed\r\n");
                    }
                    Mono::delay(1.millis().into()).await;
                    continue;
                }
                GoldenFlashOperation::SaveConfigProgram { slot } => {
                    let mut persisted = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                    let address = layout.config_slot_addresses[slot as usize];
                    let expected_sequence =
                        cx.local.flash_manager_state.config_sequence.wrapping_add(1);
                    let expected = cx.local.flash_manager_state.config.encode();
                    let verified = cx.local.flash_device.read(address, &mut persisted).is_ok()
                        && matches!(
                            ferrowasp_core::blackbox::decode_config_page(&persisted),
                            Ok((sequence, payload))
                                if sequence == expected_sequence && payload == expected.as_slice()
                        );
                    cx.local.flash_manager_state.operation = GoldenFlashOperation::Idle;
                    if verified {
                        cx.local.flash_manager_state.config_sequence = expected_sequence;
                        cx.local.flash_manager_state.config_active_slot = slot;
                        cx.shared
                            .tuning_profile
                            .lock(|profile| *profile = cx.local.flash_manager_state.config.tuning);
                        cx.shared.flash_log_rate_divisor.lock(|divisor| {
                            *divisor =
                                u32::from(cx.local.flash_manager_state.config.log_rate_divisor)
                                    .clamp(1, 16)
                        });
                        cx.shared
                            .tuning_request_seq
                            .lock(|seq| *seq = seq.wrapping_add(1).max(1));
                        let _ = queue_response("OK config saved\r\n");
                    } else {
                        cx.shared
                            .storage_status
                            .lock(|status| status.faults = status.faults.saturating_add(1));
                        let _ = queue_response("ERR config persistence verification failed\r\n");
                    }
                    Mono::delay(1.millis().into()).await;
                    continue;
                }
                GoldenFlashOperation::Idle => {}
            }

            if let Some(command) = cx.local.flash_command_consumer.dequeue() {
                let mut response = heapless::String::<
                    { ferrowasp_tasks::flash_storage::USB_RESPONSE_CAPACITY },
                >::new();
                match command {
                    StorageCommand::Help => {
                        let _ = queue_response(
                            "OK flash info | logs list/read-page/erase | config get/set/save\r\n",
                        );
                    }
                    StorageCommand::FlashInfo => {
                        let _ = write!(
                            response,
                            "OK jedec={:02x}:{:02x}:{:02x} bytes={} ready=1\r\n",
                            storage_status.jedec[0],
                            storage_status.jedec[1],
                            storage_status.jedec[2],
                            layout.capacity_bytes,
                        );
                        let _ = queue_response(response.as_str());
                    }
                    StorageCommand::FlashTestConfirmed => {
                        let _ = queue_response("ERR scratch test unavailable in generated app\r\n");
                    }
                    StorageCommand::LogsList => {
                        if let Some(line) = format_log_info_response(
                            cx.local.flash_manager_state.next_page,
                            cx.local.flash_manager_state.next_flight,
                            layout.log_page_count,
                            cx.local.flash_manager_state.log_writable,
                        ) {
                            let _ = queue_response(line.as_str());
                        }
                    }
                    StorageCommand::LogsReadPage(page) => {
                        if armed {
                            let _ = queue_response("ERR log reads disabled while armed\r\n");
                        } else if page >= cx.local.flash_manager_state.next_page {
                            let _ = queue_response("ERR log page is not present\r\n");
                        } else if let Some(address) = layout.log_page_address(page) {
                            let mut bytes = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                            if cx.local.flash_device.read(address, &mut bytes).is_ok() {
                                let mut frames = [None; 16];
                                let mut frame_count = 0_usize;
                                let emitted = emit_page_hex_lines(page, &bytes, |line| {
                                    let Some(frame) = ResponseFrame::from_text(line) else {
                                        return false;
                                    };
                                    if frame_count >= frames.len() {
                                        return false;
                                    }
                                    frames[frame_count] = Some(frame);
                                    frame_count += 1;
                                    true
                                });
                                drop(queue_response);
                                let queued = emitted
                                    && frames[..frame_count].iter().flatten().all(|frame| {
                                        cx.local.flash_response_producer.enqueue(*frame).is_ok()
                                    });
                                if !queued {
                                    if let Some(frame) =
                                        ResponseFrame::from_text("ERR log response queue full\r\n")
                                    {
                                        let _ = cx.local.flash_response_producer.enqueue(frame);
                                    }
                                }
                            } else {
                                let _ = queue_response("ERR log page read failed\r\n");
                            }
                        }
                    }
                    StorageCommand::LogsEraseConfirmed => {
                        if armed {
                            let _ = queue_response("ERR log erase disabled while armed\r\n");
                        } else {
                            cx.local.flash_manager_state.operation =
                                GoldenFlashOperation::EraseLogs { next_sector: 0 };
                            let _ = queue_response("OK log erase started\r\n");
                        }
                    }
                    StorageCommand::ConfigGet(key) => {
                        let _ = write!(
                            response,
                            "OK {}={:.4}\r\n",
                            key.name(),
                            cx.local.flash_manager_state.config.get(key),
                        );
                        let _ = queue_response(response.as_str());
                    }
                    StorageCommand::ConfigSet(key, value) => {
                        if armed {
                            let _ = queue_response("ERR config changes disabled while armed\r\n");
                        } else if cx.local.flash_manager_state.config.set(key, value) {
                            let _ = queue_response("OK staged; use config save\r\n");
                        } else {
                            let _ = queue_response("ERR value outside allowed range\r\n");
                        }
                    }
                    StorageCommand::ConfigSave => {
                        if armed {
                            let _ = queue_response("ERR config save disabled while armed\r\n");
                        } else {
                            let sequence =
                                cx.local.flash_manager_state.config_sequence.wrapping_add(1);
                            match ferrowasp_core::blackbox::encode_config_page(
                                sequence,
                                &cx.local.flash_manager_state.config.encode(),
                            ) {
                                Ok(page) => {
                                    let slot = 1 - cx.local.flash_manager_state.config_active_slot;
                                    cx.local.flash_manager_state.pending_config_page = page;
                                    if cx
                                        .local
                                        .flash_device
                                        .erase_sector_4k(
                                            layout.config_slot_addresses[slot as usize],
                                        )
                                        .is_ok()
                                    {
                                        cx.local.flash_manager_state.operation =
                                            GoldenFlashOperation::SaveConfigErase { slot };
                                        let _ = queue_response("OK config save started\r\n");
                                    } else {
                                        let _ = queue_response("ERR config slot erase failed\r\n");
                                    }
                                }
                                Err(_) => {
                                    let _ = queue_response("ERR config encoding failed\r\n");
                                }
                            }
                        }
                    }
                }
            }

            if let Some(record) = cx.local.flash_record_consumer.dequeue() {
                if armed && !cx.local.flash_manager_state.assembler.recording() {
                    cx.local.flash_manager_state.assembler.start(
                        cx.local.flash_manager_state.next_flight,
                        cx.local.flash_manager_state.boot_session_start_pending,
                    );
                    cx.local.flash_manager_state.next_flight = cx
                        .local
                        .flash_manager_state
                        .next_flight
                        .wrapping_add(1)
                        .max(1);
                    cx.local.flash_manager_state.boot_session_start_pending = false;
                }
                if armed
                    && cx.local.flash_manager_state.pending_log_page.is_none()
                    && matches!(
                        cx.local.flash_manager_state.assembler.push(record),
                        Ok(true)
                    )
                {
                    cx.local.flash_manager_state.pending_log_page =
                        cx.local.flash_manager_state.assembler.take_ready_page();
                }
            }
            if !armed && cx.local.flash_manager_state.assembler.recording() {
                if matches!(cx.local.flash_manager_state.assembler.stop(), Ok(true)) {
                    cx.local.flash_manager_state.pending_log_page =
                        cx.local.flash_manager_state.assembler.take_ready_page();
                }
            }

            if let Some(page) = cx.local.flash_manager_state.pending_log_page
                && cx.local.flash_manager_state.log_writable
                && let Some(address) =
                    layout.log_page_address(cx.local.flash_manager_state.next_page)
            {
                if cx.local.flash_device.page_program(address, &page).is_ok() {
                    cx.local.flash_manager_state.pending_log_page = None;
                    cx.local.flash_manager_state.next_page =
                        cx.local.flash_manager_state.next_page.saturating_add(1);
                    if cx.local.flash_manager_state.next_page >= layout.log_page_count {
                        cx.local.flash_manager_state.log_writable = false;
                    }
                    cx.shared.storage_status.lock(|status| {
                        status.next_page = cx.local.flash_manager_state.next_page;
                        status.next_flight = cx.local.flash_manager_state.next_flight;
                    });
                } else {
                    cx.local.flash_manager_state.log_writable = false;
                    cx.shared
                        .storage_status
                        .lock(|status| status.faults = status.faults.saturating_add(1));
                }
            }

            Mono::delay(1.millis().into()).await;
        }
    }

    /// Services USB CDC diagnostics and forwards only whitelisted storage commands.
    #[task(
        binds = OTG_FS,
        priority = 5,
        local = [usb_device, usb_serial, usb_header_sent, usb_command_parser, flash_command_producer, flash_response_consumer, usb_pending_response],
        shared = [flight_service_telemetry, storage_status, usb_status_due]
    )]
    fn usb_cdc(mut cx: usb_cdc::Context) {
        const USB_DEBUG_HEADER: &[u8] = b"FerroWasp Foxeer debug/config (disarmed writes only)\r\n";
        let _ = cx.local.usb_device.poll(&mut [cx.local.usb_serial]);

        let mut rx = [0_u8; 64];
        let read_len = cx.local.usb_serial.read(&mut rx).unwrap_or(0);
        for byte in &rx[..read_len] {
            let Some(parsed) = cx.local.usb_command_parser.ingest(*byte) else {
                continue;
            };
            match parsed {
                Ok(command) if cx.local.flash_command_producer.enqueue(command).is_ok() => {}
                Ok(_) => {
                    let _ = cx.local.usb_serial.write(b"ERR command queue full\r\n");
                }
                Err(_) => {
                    let _ = cx
                        .local
                        .usb_serial
                        .write(b"ERR invalid command; type help\r\n");
                }
            }
        }

        if cx.local.usb_device.state() != UsbDeviceState::Configured {
            *cx.local.usb_header_sent = false;
            *cx.local.usb_pending_response = None;
            return;
        }
        if cx.local.usb_serial.flush().is_err() {
            return;
        }

        if !*cx.local.usb_header_sent {
            if matches!(
                cx.local.usb_serial.write(USB_DEBUG_HEADER),
                Ok(written) if written == USB_DEBUG_HEADER.len()
            ) {
                *cx.local.usb_header_sent = true;
            }
            return;
        }

        if cx.local.usb_pending_response.is_none() {
            *cx.local.usb_pending_response = cx.local.flash_response_consumer.dequeue();
        }
        if let Some(response) = cx.local.usb_pending_response.as_ref() {
            if matches!(
                cx.local.usb_serial.write(response.as_bytes()),
                Ok(written) if written == response.as_bytes().len()
            ) {
                *cx.local.usb_pending_response = None;
                ferrowasp_stm32f4::usb_serial::pend_usb_irq();
            }
            return;
        }

        let due = cx.shared.usb_status_due.lock(|due| {
            let value = *due;
            *due = false;
            value
        });
        if !due {
            return;
        }

        let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
        let telemetry = cx
            .shared
            .flight_service_telemetry
            .lock(|telemetry| *telemetry);
        let _storage = cx.shared.storage_status.lock(|storage| *storage);
        let battery = telemetry
            .battery
            .is_fresh(now_ms)
            .then_some(telemetry.battery);
        let snapshot = StatusSnapshot {
            uptime_ms: now_ms,
            imu_kind: ImuKind::None,
            imu_ready: telemetry.imu_sequence != 0,
            imu_sequence: telemetry.imu_sequence,
            gyro_raw: [0; 3],
            imu_stale: telemetry.imu_stale,
            control_sequence: telemetry.control_sequence,
            rc_valid: telemetry.rc_link_valid,
            rc_armable: telemetry.rc_link_valid && !telemetry.armed,
            rc_throttle: telemetry.rc_throttle,
            rc_arm_high: false,
            system_armed: telemetry.armed,
            battery_voltage_decivolts: battery
                .map_or(0, |value| u32::from(value.voltage_decivolts)),
            battery_current_centiamps: battery
                .map_or(0, |value| i32::from(value.current_centiamps)),
            adc_voltage_mv: battery.map_or(0, |value| u32::from(value.adc_voltage_mv)),
            adc_current_mv: battery.map_or(0, |value| u32::from(value.adc_current_mv)),
        };
        match usb_debug::format_status(snapshot) {
            Ok(line)
                if matches!(
                    cx.local.usb_serial.write(line.as_bytes()),
                    Ok(written) if written == line.len()
                ) => {}
            _ => cx.shared.usb_status_due.lock(|due| *due = true),
        }
    }

    /// Emits bounded service identity/health and schedules USB diagnostics.
    #[task(
        priority = 1,
        shared = [flight_service_telemetry, storage_status, usb_status_due]
    )]
    async fn heartbeat(mut cx: heartbeat::Context) {
        loop {
            let telemetry = cx
                .shared
                .flight_service_telemetry
                .lock(|telemetry| *telemetry);
            let storage = cx.shared.storage_status.lock(|storage| *storage);
            defmt::info!(
                "FerroWasp Foxeer heartbeat ctl={} imu={} armed={} flash={} dropped={}",
                telemetry.control_sequence,
                telemetry.imu_sequence,
                telemetry.armed,
                storage.ready,
                storage.dropped_records,
            );
            cx.shared.usb_status_due.lock(|due| *due = true);
            ferrowasp_stm32f4::usb_serial::pend_usb_irq();
            Mono::delay(2_000.millis().into()).await;
        }
    }

    /// Acknowledges TIM6 and performs one bounded SPI1 timeout recovery check.
    #[task(
        binds = TIM6_DAC,
        priority = 9,
        local = [io_watchdog],
        shared = [spi1_owner]
    )]
    fn io_watchdog(mut cx: io_watchdog::Context) {
        acknowledge_watchdog_tick(cx.local.io_watchdog);
        let now_us = Mono::now().duration_since_epoch().to_micros();
        match cx
            .shared
            .spi1_owner
            .lock(|owner| owner.service_timeout(now_us))
        {
            SpiImuTimeoutOutcome::Idle | SpiImuTimeoutOutcome::Active => {}
            SpiImuTimeoutOutcome::TimedOut => {
                defmt::warn!("TIM6 watchdog recovered an expired SPI1 transaction")
            }
            SpiImuTimeoutOutcome::RecoveryFailed => {
                defmt::warn!("TIM6 watchdog could not recover SPI1; owner disabled")
            }
        }
    }

    /// Services one STM32F4 UART peripheral IDLE interrupt.
    #[task(binds = USART2, priority = 13, shared = [serial1_rx])]
    fn serial1_rx_idle_irq(mut cx: serial1_rx_idle_irq::Context) {
        match cx
            .shared
            .serial1_rx
            .lock(UartRxIrqService::service_idle_irq)
        {
            UartRxIrqOutcome::Ignored | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::Delivered => {
                let _ = serial1_rx_bridge::spawn();
            }
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX peripheral error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX IDLE buffer delivery error")
            }
        }
    }

    /// Services one STM32F4 UART receive-DMA interrupt.
    #[task(binds = DMA1_STREAM5, priority = 13, shared = [serial1_rx])]
    fn serial1_rx_dma_irq(mut cx: serial1_rx_dma_irq::Context) {
        match cx.shared.serial1_rx.lock(UartRxIrqService::service_dma_irq) {
            UartRxIrqOutcome::Ignored | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::Delivered => {
                let _ = serial1_rx_bridge::spawn();
            }
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX DMA error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX DMA buffer delivery error")
            }
        }
    }

    /// Drains every completed DMA buffer without unbounded waiting.
    #[task(priority = 12, local = [serial1_rx_bridge])]
    async fn serial1_rx_bridge(cx: serial1_rx_bridge::Context) {
        loop {
            match cx.local.serial1_rx_bridge.publish_next_untimed() {
                UartOwnedRxBridgeOutcome::Published => {}
                UartOwnedRxBridgeOutcome::NoChunk => return,
                UartOwnedRxBridgeOutcome::InvalidChunk => {
                    defmt::warn!("UART RX bridge rejected an invalid chunk")
                }
                UartOwnedRxBridgeOutcome::QueueOverflow => {
                    defmt::warn!("UART RX owned queue overflowed")
                }
                UartOwnedRxBridgeOutcome::Disabled => return,
                UartOwnedRxBridgeOutcome::RecycleFailed => {
                    defmt::warn!("UART RX bridge could not recycle its DMA buffer");
                    return;
                }
            }
        }
    }

    /// Services one STM32F4 UART transmit-DMA interrupt.
    #[task(
        binds = DMA1_STREAM6,
        priority = 13,
        local = [serial1_tx_completion],
        shared = [serial1_tx_dma]
    )]
    fn serial1_tx_dma_irq(mut cx: serial1_tx_dma_irq::Context) {
        let outcome = cx.shared.serial1_tx_dma.lock(UartTxDmaService::service_irq);

        match outcome {
            UartTxIrqOutcome::Ignored => {}
            UartTxIrqOutcome::Completed => {
                if cx.local.serial1_tx_completion.complete().is_err() {
                    defmt::warn!("UART TX completion arrived without an in-flight chunk");
                }
            }
            UartTxIrqOutcome::DmaError(error) => {
                cx.local
                    .serial1_tx_completion
                    .fail(SerialFault::DmaTransfer);
                match error {
                    ferrowasp_stm32f4::serial::UartTxDmaError::Transfer => {
                        defmt::warn!("UART TX DMA transfer error")
                    }
                    ferrowasp_stm32f4::serial::UartTxDmaError::DirectMode => {
                        defmt::warn!("UART TX DMA direct-mode error")
                    }
                }
            }
        }
    }

    /// Drains queued UART chunks through DMA and awaits each IRQ completion.
    #[task(priority = 4, local = [serial1_tx_owner], shared = [serial1_tx_dma])]
    async fn serial1_tx_worker(mut cx: serial1_tx_worker::Context) {
        loop {
            let chunk = match cx.local.serial1_tx_owner.next_chunk().await {
                Ok(chunk) => chunk,
                Err(_) => {
                    defmt::warn!("UART TX worker stopped before DMA start");
                    return;
                }
            };

            let start = cx
                .shared
                .serial1_tx_dma
                .lock(|tx_dma| tx_dma.start_chunk(&chunk));
            if let Err(error) = start {
                let fault = match error {
                    UartTxStartError::InvalidChunk => SerialFault::InvalidChunk,
                    UartTxStartError::Busy | UartTxStartError::TransferMissing => {
                        SerialFault::InvalidState
                    }
                };
                cx.local.serial1_tx_owner.fail(fault);
                defmt::warn!("UART TX DMA start failed");
                return;
            }

            if cx.local.serial1_tx_owner.wait_completion().await.is_err() {
                defmt::warn!("UART TX worker stopped after DMA start");
                return;
            }
        }
    }

    /// Services one STM32F4 UART peripheral IDLE interrupt.
    #[task(binds = UART4, priority = 6, shared = [serial2_rx])]
    fn serial2_rx_idle_irq(mut cx: serial2_rx_idle_irq::Context) {
        match cx
            .shared
            .serial2_rx
            .lock(UartRxIrqService::service_idle_irq)
        {
            UartRxIrqOutcome::Ignored | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::Delivered => {
                let _ = serial2_rx_bridge::spawn();
            }
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX peripheral error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX IDLE buffer delivery error")
            }
        }
    }

    /// Services one STM32F4 UART receive-DMA interrupt.
    #[task(binds = DMA1_STREAM2, priority = 6, shared = [serial2_rx])]
    fn serial2_rx_dma_irq(mut cx: serial2_rx_dma_irq::Context) {
        match cx.shared.serial2_rx.lock(UartRxIrqService::service_dma_irq) {
            UartRxIrqOutcome::Ignored | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::Delivered => {
                let _ = serial2_rx_bridge::spawn();
            }
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX DMA error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX DMA buffer delivery error")
            }
        }
    }

    /// Drains every completed DMA buffer without unbounded waiting.
    #[task(priority = 4, local = [serial2_rx_bridge])]
    async fn serial2_rx_bridge(cx: serial2_rx_bridge::Context) {
        loop {
            match cx.local.serial2_rx_bridge.publish_next_untimed() {
                UartOwnedRxBridgeOutcome::Published => {}
                UartOwnedRxBridgeOutcome::NoChunk => return,
                UartOwnedRxBridgeOutcome::InvalidChunk => {
                    defmt::warn!("UART RX bridge rejected an invalid chunk")
                }
                UartOwnedRxBridgeOutcome::QueueOverflow => {
                    defmt::warn!("UART RX owned queue overflowed")
                }
                UartOwnedRxBridgeOutcome::Disabled => return,
                UartOwnedRxBridgeOutcome::RecycleFailed => {
                    defmt::warn!("UART RX bridge could not recycle its DMA buffer");
                    return;
                }
            }
        }
    }

    /// Services one STM32F4 UART transmit-DMA interrupt.
    #[task(
        binds = DMA1_STREAM4,
        priority = 6,
        local = [serial2_tx_completion],
        shared = [serial2_tx_dma]
    )]
    fn serial2_tx_dma_irq(mut cx: serial2_tx_dma_irq::Context) {
        let outcome = cx.shared.serial2_tx_dma.lock(UartTxDmaService::service_irq);

        match outcome {
            UartTxIrqOutcome::Ignored => {}
            UartTxIrqOutcome::Completed => {
                if cx.local.serial2_tx_completion.complete().is_err() {
                    defmt::warn!("UART TX completion arrived without an in-flight chunk");
                }
            }
            UartTxIrqOutcome::DmaError(error) => {
                cx.local
                    .serial2_tx_completion
                    .fail(SerialFault::DmaTransfer);
                match error {
                    ferrowasp_stm32f4::serial::UartTxDmaError::Transfer => {
                        defmt::warn!("UART TX DMA transfer error")
                    }
                    ferrowasp_stm32f4::serial::UartTxDmaError::DirectMode => {
                        defmt::warn!("UART TX DMA direct-mode error")
                    }
                }
            }
        }
    }

    /// Drains queued UART chunks through DMA and awaits each IRQ completion.
    #[task(priority = 4, local = [serial2_tx_owner], shared = [serial2_tx_dma])]
    async fn serial2_tx_worker(mut cx: serial2_tx_worker::Context) {
        loop {
            let chunk = match cx.local.serial2_tx_owner.next_chunk().await {
                Ok(chunk) => chunk,
                Err(_) => {
                    defmt::warn!("UART TX worker stopped before DMA start");
                    return;
                }
            };

            let start = cx
                .shared
                .serial2_tx_dma
                .lock(|tx_dma| tx_dma.start_chunk(&chunk));
            if let Err(error) = start {
                let fault = match error {
                    UartTxStartError::InvalidChunk => SerialFault::InvalidChunk,
                    UartTxStartError::Busy | UartTxStartError::TransferMissing => {
                        SerialFault::InvalidState
                    }
                };
                cx.local.serial2_tx_owner.fail(fault);
                defmt::warn!("UART TX DMA start failed");
                return;
            }

            if cx.local.serial2_tx_owner.wait_completion().await.is_err() {
                defmt::warn!("UART TX worker stopped after DMA start");
                return;
            }
        }
    }

    /// Acknowledges one IMU data-ready edge and requests a sample.
    #[task(
        binds = EXTI4,
        priority = 14,
        local = [spi1_data_ready],
        shared = [spi1_kind]
    )]
    fn spi1_data_ready(mut cx: spi1_data_ready::Context) {
        if !cx.local.spi1_data_ready.check_interrupt() {
            return;
        }
        cx.local.spi1_data_ready.clear_interrupt_pending_bit();

        let kind = cx.shared.spi1_kind.lock(|kind| *kind);
        if kind == 0 {
            return;
        }
        let observed_at_us = Mono::now().duration_since_epoch().to_micros();
        if spi1_poll::spawn(observed_at_us).is_err() {
            defmt::warn!("IMU sample request rejected while the poll task is busy");
        }
    }

    /// Submits one full IMU burst through the bounded SPI mailbox.
    #[task(
        priority = 12,
        local = [spi1_device, spi1_unavailable_logged],
        shared = [spi1_kind]
    )]
    async fn spi1_poll(mut cx: spi1_poll::Context, observed_at_us: u64) {
        let kind = cx.shared.spi1_kind.lock(|kind| *kind);
        let request = match kind {
            IMU_KIND_MPU6500 => ferrowasp_drivers::mpu6500::Register::AccelXoutH as u8,
            IMU_KIND_ICM42688P => ferrowasp_drivers::icm42688p::Register::TempData1 as u8,
            _ => return,
        };

        let _ = spi1_timeout::spawn(observed_at_us);
        let result = cx
            .local
            .spi1_device
            .read_burst(request, observed_at_us)
            .await;

        match result {
            Ok(()) => {
                *cx.local.spi1_unavailable_logged = false;
            }
            Err(SpiDeviceError::Busy) => defmt::warn!("SPI1 IMU transaction already active"),
            Err(SpiDeviceError::Unavailable) => {
                if !*cx.local.spi1_unavailable_logged {
                    defmt::warn!("SPI1 IMU owner unavailable after recovery failure");
                    *cx.local.spi1_unavailable_logged = true;
                }
            }
            Err(SpiDeviceError::Timeout) => {}
            Err(SpiDeviceError::Cancelled) => {
                defmt::warn!("SPI1 IMU transaction cancelled")
            }
            Err(SpiDeviceError::DmaTransfer) => {
                defmt::warn!("SPI1 IMU DMA transaction failed")
            }
            Err(
                SpiDeviceError::TooManyOperations
                | SpiDeviceError::TxCapacityExceeded
                | SpiDeviceError::RxCapacityExceeded
                | SpiDeviceError::CopybackShapeMismatch,
            ) => defmt::warn!("SPI1 IMU transaction packing failed"),
            Err(
                SpiDeviceError::InvalidState
                | SpiDeviceError::StaleTransaction
                | SpiDeviceError::Backend,
            ) => defmt::warn!("SPI1 IMU transaction backend error"),
        }
    }

    /// Starts or cancels one transaction requested through the async mailbox.
    #[task(priority = 13, shared = [spi1_owner])]
    async fn spi1_owner_service(mut cx: spi1_owner_service::Context) {
        let now_us = Mono::now().duration_since_epoch().to_micros();
        match cx
            .shared
            .spi1_owner
            .lock(|owner| owner.service_request(now_us))
        {
            SpiImuOwnerOutcome::Idle
            | SpiImuOwnerOutcome::Started
            | SpiImuOwnerOutcome::Cancelled => {}
            SpiImuOwnerOutcome::RecoveryFailed => {
                defmt::warn!("SPI1 IMU cancellation recovery failed; owner disabled")
            }
            SpiImuOwnerOutcome::StartFailed => {
                defmt::warn!("SPI1 IMU transaction could not start")
            }
        }
    }

    /// Completes one SPI1 receive-DMA transaction.
    #[task(binds = DMA2_STREAM0, priority = 13, shared = [spi1_owner])]
    fn spi1_rx_dma_irq(mut cx: spi1_rx_dma_irq::Context) {
        match cx.shared.spi1_owner.lock(|owner| owner.service_dma_irq()) {
            SpiImuRxOutcome::Ignored | SpiImuRxOutcome::NoChunk => {}
            SpiImuRxOutcome::Delivered => {
                let _ = spi1_parser::spawn();
            }
            SpiImuRxOutcome::DmaError => defmt::warn!("SPI1 IMU RX DMA error"),
            SpiImuRxOutcome::NoFreshBuffer => {
                defmt::warn!("SPI1 IMU RX buffer pool exhausted")
            }
            SpiImuRxOutcome::TransferNotReady => {
                defmt::warn!("SPI1 IMU RX DMA restart failed")
            }
            SpiImuRxOutcome::FilledQueueFull => {
                defmt::warn!("SPI1 IMU filled queue full")
            }
            SpiImuRxOutcome::PlannerRejected => {
                defmt::warn!("SPI1 IMU RX planner rejected completion")
            }
        }
    }

    /// Recovers a transaction that did not complete before its deadline.
    #[task(priority = 13, shared = [spi1_owner])]
    async fn spi1_timeout(mut cx: spi1_timeout::Context, observed_at_us: u64) {
        let _ = observed_at_us;
        Mono::delay(1.millis().into()).await;
        let now_us = Mono::now().duration_since_epoch().to_micros();
        match cx
            .shared
            .spi1_owner
            .lock(|owner| owner.service_timeout(now_us))
        {
            SpiImuTimeoutOutcome::Idle | SpiImuTimeoutOutcome::Active => {}
            SpiImuTimeoutOutcome::TimedOut => {
                defmt::warn!("SPI1 IMU transaction timed out and was recovered")
            }
            SpiImuTimeoutOutcome::RecoveryFailed => {
                defmt::warn!("SPI1 IMU timeout recovery failed; owner disabled")
            }
        }
    }

    /// Decodes DMA buffers and publishes body-frame IMU samples.
    ///
    /// Downstream control code must not repeat the installation rotation. The
    /// controller's explicit body-to-controller compatibility transform remains
    /// a separate operation.
    #[task(priority = 11, local = [spi1_parser], shared = [spi1_kind, imu_sample])]
    async fn spi1_parser(mut cx: spi1_parser::Context) {
        let kind = cx.shared.spi1_kind.lock(|kind| *kind);
        while let Some(filled) = cx.local.spi1_parser.next_frame() {
            let len = filled.len.min(filled.buffer.len());
            let frame = &filled.buffer[..len];
            let expected_request = match kind {
                IMU_KIND_MPU6500 => Some(ferrowasp_drivers::mpu6500::Register::AccelXoutH as u8),
                IMU_KIND_ICM42688P => Some(ferrowasp_drivers::icm42688p::Register::TempData1 as u8),
                _ => None,
            };
            let parsed = if expected_request != Some(filled.request) {
                None
            } else {
                match kind {
                    IMU_KIND_MPU6500 => {
                        ferrowasp_drivers::mpu6500::decode_accel_temp_gyro_burst(frame)
                            .ok()
                            .map(|sample| {
                                (
                                    [
                                        sample.acc_raw[0] as f32 / 4_096.0,
                                        sample.acc_raw[1] as f32 / 4_096.0,
                                        sample.acc_raw[2] as f32 / 4_096.0,
                                    ],
                                    [
                                        sample.gyro_raw[0] as f32 / 16.4,
                                        sample.gyro_raw[1] as f32 / 16.4,
                                        sample.gyro_raw[2] as f32 / 16.4,
                                    ],
                                    sample.gyro_raw,
                                    sample.temp_raw as f32 / 333.87 + 21.0,
                                )
                            })
                    }
                    IMU_KIND_ICM42688P => {
                        ferrowasp_drivers::icm42688p::decode_temp_accel_gyro_burst(frame)
                            .ok()
                            .map(|sample| {
                                (
                                    sample
                                        .accel_g(ferrowasp_drivers::icm42688p::AccelFullScale::G16),
                                    sample.gyro_dps(
                                        ferrowasp_drivers::icm42688p::GyroFullScale::Dps2000,
                                    ),
                                    sample.gyro_raw,
                                    sample.temperature_c(),
                                )
                            })
                    }
                    _ => None,
                }
            };

            if let Some((acc, gyro, gyro_raw, temp)) = parsed {
                let acc = FrameRotation::new([1, 0, 2], [-1, -1, -1]).map_f32(acc);
                let gyro = FrameRotation::new([1, 0, 2], [-1, -1, -1]).map_f32(gyro);
                let gyro_raw =
                    FrameRotation::new([1, 0, 2], [-1, -1, -1]).map_i16_saturating(gyro_raw);
                cx.shared.imu_sample.lock(|sample| {
                    sample.acc = acc;
                    sample.gyro = gyro;
                    sample.gyro_raw = gyro_raw;
                    sample.temp = temp;
                    sample.sequence = sample.sequence.wrapping_add(1);
                });
            } else {
                defmt::warn!("SPI1 IMU frame could not be decoded");
            }

            if cx.local.spi1_parser.return_buffer(filled.buffer).is_err() {
                defmt::warn!("SPI1 IMU free-buffer queue rejected a returned buffer");
            }
        }
    }

    /// Runs the bounded Foxeer control calculation from the TIM4 interrupt.
    ///
    /// This checkpoint has no safety-master arm source. Its priority-15
    /// consumer drains and validates requests but cannot command motor hardware.
    #[task(
        binds = TIM4,
        priority = 14,
        local = [control_scheduler, control_phase, sbus_control_consumer, imu_control_consumer, motor_cmd_producer, prearm_health_producer, flash_record_producer, control_loop_cnt, samples_per_control_loop, flight_controller, imu_rate_filter, imu_angle_integrator, gyro_axis_map, gyro_bias_calibrator, imu_last_sequence, imu_stale_ticks, applied_tuning_seq, motor_cmd_seq, rc_link_was_valid, latest_rc_input, rc_last_valid_frames, rc_stale_ticks, control_was_armed],
        shared = [flight_service_telemetry, tuning_profile, tuning_request_seq, flash_log_rate_divisor, storage_status]
    )]
    fn control_loop(mut cx: control_loop::Context) {
        acknowledge_control_tick(cx.local.control_scheduler);
        *cx.local.control_phase = cx.local.control_phase.wrapping_add(1);
        if *cx.local.control_phase < 2 {
            return;
        }
        *cx.local.control_phase = 0;

        if *cx.local.samples_per_control_loop != 2 {
            cx.local.flight_controller.reset_control_state();
            cx.local.imu_rate_filter.reset();
            return;
        }
        *cx.local.control_loop_cnt = cx.local.control_loop_cnt.wrapping_add(1);

        let tuning = cx
            .shared
            .tuning_profile
            .lock(|profile| *profile)
            .sanitized();
        let tuning_seq = cx.shared.tuning_request_seq.lock(|sequence| *sequence);
        if tuning_seq != 0 && tuning_seq != *cx.local.applied_tuning_seq {
            cx.local.flight_controller.apply_tuning_profile(tuning);
            cx.local.imu_rate_filter.set_alpha(tuning.imu_lpf_alpha);
            *cx.local.applied_tuning_seq = tuning_seq;
        }

        let mut rc_observed = false;
        while let Some(snapshot) = cx.local.sbus_control_consumer.try_receive() {
            if snapshot.has_valid_frame && !snapshot.frame_lost && !snapshot.failsafe {
                *cx.local.latest_rc_input = snapshot;
                rc_observed = true;
            } else {
                *cx.local.rc_link_was_valid = false;
            }
        }

        let mut newest_imu = None;
        while let Some(sample) = cx.local.imu_control_consumer.try_receive() {
            newest_imu = Some(ferrowasp_tasks::foxeer_control::ImuControlSample {
                specific_force_body: sample.acc,
                gyro_raw_body: sample.gyro_raw,
                sequence: sample.sequence,
            });
        }
        let previous_imu_sequence = *cx.local.imu_last_sequence;
        let imu_fresh = newest_imu.is_some_and(|sample| {
            sample.sequence != previous_imu_sequence
                && sample
                    .specific_force_body
                    .iter()
                    .all(|value| value.is_finite())
        });
        let raw_gyro_dps = newest_imu.map_or([0.0; 3], |sample| {
            cx.local
                .gyro_axis_map
                .map_i32(sample.gyro_raw_body.map(i32::from))
                .map(|value| value as f32 / ferrowasp_tasks::foxeer_control::FOXEER_GYRO_RAW_TO_DPS)
        });

        // Arming remains hard-inhibited until the safety-master checkpoint.
        let outcome = ferrowasp_tasks::foxeer_control::run_foxeer_control_step(
            ferrowasp_tasks::foxeer_control::FoxeerControlState {
                flight_controller: cx.local.flight_controller,
                imu_rate_filter: cx.local.imu_rate_filter,
                imu_angle_integrator: cx.local.imu_angle_integrator,
                gyro_axis_map: cx.local.gyro_axis_map,
                gyro_bias_calibrator: cx.local.gyro_bias_calibrator,
                imu_last_sequence: cx.local.imu_last_sequence,
                imu_stale_ticks: cx.local.imu_stale_ticks,
                applied_tuning_seq: cx.local.applied_tuning_seq,
                rc_last_valid_frames: cx.local.rc_last_valid_frames,
                rc_stale_ticks: cx.local.rc_stale_ticks,
                rc_link_was_valid: cx.local.rc_link_was_valid,
                control_was_armed: cx.local.control_was_armed,
            },
            ferrowasp_tasks::foxeer_control::FoxeerControlInput {
                rc: cx.local.latest_rc_input,
                rc_observed,
                imu: newest_imu,
                armed: false,
                tuning,
            },
        );

        let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
        let health = PreArmHealthReport {
            health: PreArmHealth {
                imu_ready: *cx.local.imu_last_sequence != 0,
                imu_bias_calibrated: cx.local.gyro_bias_calibrator.ready(),
                imu_fresh,
            },
            sequence: *cx.local.control_loop_cnt,
            observed_at_us: now_us,
        };
        if cx.local.prearm_health_producer.try_send(health).is_err() {
            defmt::warn!("control-to-safety health channel full; evidence rejected");
        }

        cx.shared.flight_service_telemetry.lock(|telemetry| {
            telemetry.rates_dps = cx.local.imu_rate_filter.values();
            telemetry.angles_deg = cx.local.imu_angle_integrator.angles();
            telemetry.imu_sequence = *cx.local.imu_last_sequence;
            telemetry.control_sequence = *cx.local.control_loop_cnt;
            telemetry.imu_stale = !imu_fresh;
        });

        let compact = dt::CompactRateBlackboxSample::from_rate_sample(
            *cx.local.control_loop_cnt,
            *cx.local.imu_last_sequence,
            false,
            imu_fresh,
            raw_gyro_dps,
            cx.local.flight_controller.blackbox_sample(),
        );
        let log_divisor = cx
            .shared
            .flash_log_rate_divisor
            .lock(|divisor| (*divisor).clamp(1, 16));
        if matches!(
            ferrowasp_tasks::flash_storage::enqueue_rate_record(
                cx.local.flash_record_producer,
                compact,
                now_us,
                log_divisor,
            ),
            RecordEnqueueOutcome::Full
        ) {
            cx.shared
                .storage_status
                .lock(|status| status.dropped_records = status.dropped_records.saturating_add(1));
        }

        if let ferrowasp_tasks::foxeer_control::FoxeerControlOutcome::MotorRequest(motors) = outcome
        {
            let publication = ferrowasp_tasks::foxeer_control::publish_motor_request(
                cx.local.motor_cmd_seq,
                motors,
                now_us / 1_000,
                |command| match cx.local.motor_cmd_producer.try_send(command) {
                    Ok(()) => ferrowasp_tasks::foxeer_control::MotorQueueOutcome::Accepted,
                    Err(_) => ferrowasp_tasks::foxeer_control::MotorQueueOutcome::Full,
                },
                || actuator_output::spawn(ActuatorCmd::ApplyLatestThrottle).is_ok(),
            );
            match publication {
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::Published => {}
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::Inhibited => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                }
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::InvalidValues => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                    defmt::warn!("invalid motor request; control failed closed");
                }
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::QueueFull => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                    defmt::warn!("motor command queue full; control failed closed");
                }
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::WakeRejected => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                    defmt::warn!("actuator wake rejected; control failed closed");
                }
            }
        }
    }
}
