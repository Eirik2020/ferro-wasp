// ####  SET-UP  ####
// Compiler directives
#![deny(unsafe_code)]
#![no_main]
#![no_std]

use ferrowasp_app_tbs_lucid_h7::internal::*;

ferroforge::app! {
    device = pac,
    peripherals = true,
    dispatchers = [FDCAN1_IT0, FDCAN2_IT0, FDCAN1_IT1, FDCAN2_IT1, FDCAN_CAL, SAI1, SAI2, LTDC, DMA2D],

    use super::*; // Import everything from parent module

    // SAFETY CRITICAL SECTION
    //------------------------------------------------------------------------
    //------------------------------------------------------------------------

    // Monotonicss
    systick_monotonic!(Mono, 1000); // Set mono timer to 1ms resolution

    #[shared]
    struct Shared {
        #[lock_free]
        uart1_rx: EscTelemetryUartIrq,
        #[lock_free]
        uart2_rx: board::aliases::Uart2RxIrq,
        uart2_bridge: Uart2OwnedRxBridge,
        #[lock_free]
        uart4_rx: board::aliases::Uart4RxIrq,
        uart4_tx_dma: board::aliases::Uart4TxDmaSide,

        // IMU
        imu_data: imu::ImuData,
        imu_angles: [f32; 3],
        imu_rates: [f32; 3],
        tuning_profile: dt::TuningProfile,
        tuning_request_seq: u32,

        // ADC
        adc1_transfer: AdcTransfer,
        battery_voltage_v10: u8,
        battery_cell_count: u8,
        battery_cell_voltage_v100: u16,
        battery_current_ca: i16,

        // SPI1
        spi1_owner: board::aliases::Spi1ImuOwner,
        io_timebase: IoTimebase,
        dshot_motors: DshotShared,
    }
    #[local]
    struct Local {
        // Safety
        arm_qualifier: safety::ArmQualifier,

        // UART
        sbus: StreamingParser,
        esc_telemetry_uart: EscTelemetryUartParser,
        esc_manager_state: EscManagerState,
        esc_request_producer: EscRequestProducer,
        esc_request_consumer: EscRequestConsumer,
        esc_ack_producer: EscAckProducer,
        esc_ack_consumer: EscAckConsumer,
        esc_telemetry_update_producer: EscTelemetryUpdateProducer,
        esc_telemetry_update_consumer: EscTelemetryUpdateConsumer,

        // SPI1
        spi1_parser: SpiRxParserSide,
        spi1_device: Spi1Device,
        imu_data_ready: board::aliases::ImuDataReadyPin,

        // SD storage. Only the priority-1 flash manager owns this device.
        flash_device: FlashDevice,
        flash_record_producer: FlashRecordProducer,
        flash_record_consumer: FlashRecordConsumer,
        flash_command_producer: FlashCommandProducer,
        flash_command_consumer: FlashCommandConsumer,
        flash_response_producer: FlashResponseProducer,
        flash_response_consumer: FlashResponseConsumer,
        flash_rpc_command_producer: FlashRpcCommandProducer,
        flash_rpc_command_consumer: FlashRpcCommandConsumer,
        flash_rpc_response_producer: FlashRpcResponseProducer,
        flash_rpc_response_consumer: FlashRpcResponseConsumer,

        // ADC
        adc1_buffer: Option<&'static mut [u16; 3]>,

        // Control Loop
        control_loop_cnt: u32,
        samples_per_control_loop: u32,
        flight_controller: dt::FlightController,
        imu_rate_filter: dt::ImuRateLowPassFilter,
        imu_angle_integrator: dt::GyroAngleIntegrator,
        gyro_axis_map: dt::GyroAxisMap,
        gyro_bias_calibrator: dt::GyroBiasCalibrator,
        control_loop_scheduler: ControlScheduler,
        io_watchdog: IoWatchdog,
        imu_last_sequence: u32,
        imu_stale_ticks: u32,
        applied_tuning_seq: u32,

        // Logical UART2 (USART6) and UART4 (USART3)
        rc_rx_reader: Uart2OwnedReader,
        rc_rx_discontinuities: Uart2Discontinuities,
        osd_uart: Option<stm32_uart::UartRxParserSide>,
        osd_rx_producer: Uart4OwnedRxProducer,
        osd_rx_reader: Uart4OwnedReader,
        osd_rx_discontinuities: Uart4Discontinuities,
        osd_tx_writer: Uart4OwnedWriter,
        osd_tx_healthy: bool,
        uart4_tx_owner: Uart4OwnedTxOwner,
        uart4_tx_completion: Uart4OwnedTxCompletion,
        osd_task: osd::OsdTask,
        osd_tx_buffer: [u8; mspv1::OSD_TX_BUFFER_LEN],
        osd_refresh_tick: u8,
        //tele_uart: Option<stm32_uart::UartRxParserSide>,
        //gps_uart: Option<stm32_uart::UartRxParserSide>,

        // ----  SAFETY  ----
        // owned by rc_input only
        rc_arm_high_writer: signals::RcArmHighWriter,
        rc_throttle_writer: signals::RcThrottleWriter,
        rc_link_frame_writer: signals::RcLinkFrameWriter,

        // owned by safety_master only
        safety_arm_writer: signals::SafetyArmWriter,
        rc_link_invalidator: signals::RcLinkInvalidator,

        // readers copied to tasks
        safety_rc_arm_high_reader: signals::RcArmHighReader,
        safety_rc_throttle_reader: signals::RcThrottleReader,
        safety_rc_link_reader: signals::RcLinkReader,

        control_safety_arm_reader: signals::SafetyArmReader,
        control_throttle_reader: signals::RcThrottleReader,
        control_rc_link_reader: signals::RcLinkReader,
        control_arm_permit_reader: ActuatorArmPermitReader,

        actuator_safety_arm_reader: signals::SafetyArmReader,
        actuator_rc_arm_high_reader: signals::RcArmHighReader,
        actuator_rc_throttle_reader: signals::RcThrottleReader,
        actuator_rc_link_reader: signals::RcLinkReader,
        osd_safety_arm_reader: signals::SafetyArmReader,
        osd_rc_throttle_reader: signals::RcThrottleReader,
        osd_rc_rates_reader: RcRatesReader,
        usb_rc_link_reader: signals::RcLinkReader,
        actuator_arm_done_writer: signals::ActuatorArmDoneWriter,
        actuator_arm_done_reader: signals::ActuatorArmDoneReader,
        actuator_arm_permit_writer: ActuatorArmPermitWriter,
        actuator_arm_permit_reader: ActuatorArmPermitReader,
        // Control-to-actuator command authority is split across this SPSC
        // channel: control owns the writer and actuator_output owns the reader.
        motor_cmd_writer: safety::signals::MotorCmdWriter,
        motor_cmd_reader: safety::signals::MotorCmdReader,
        motor_cmd_seq: u32,

        rc_rates_writer: RcRatesWriter,
        rc_rates_reader: RcRatesReader,

        // USB CDC serial
        usb_dev: UsbDebugDevice,
        usb_serial: UsbDebugSerial,
        usb_header_sent: bool,
        configurator_usb: ConfiguratorUsbState,
    }
    #[init(local = [
        uart1_rx_buffers: stm32_storage::UartRxBufferBank =
            stm32_storage::new_uart_rx_buffer_bank(),
        uart1_free_queue: stm32_storage::UartRxFreeQueue =
            stm32_storage::UartRxFreeQueue::new(),
        uart1_filled_queue: stm32_storage::UartRxFilledQueue =
            stm32_storage::UartRxFilledQueue::new(),
        uart2_rx_buffers: stm32_storage::UartRxBufferBank =
            stm32_storage::new_uart_rx_buffer_bank(),
        uart2_free_queue: stm32_storage::UartRxFreeQueue =
            stm32_storage::UartRxFreeQueue::new(),
        uart2_filled_queue: stm32_storage::UartRxFilledQueue =
            stm32_storage::UartRxFilledQueue::new(),
        uart4_rx_buffers: stm32_storage::UartRxBufferBank =
            stm32_storage::new_uart_rx_buffer_bank(),
        uart4_free_queue: stm32_storage::UartRxFreeQueue =
            stm32_storage::UartRxFreeQueue::new(),
        uart4_filled_queue: stm32_storage::UartRxFilledQueue =
            stm32_storage::UartRxFilledQueue::new(),
        uart4_tx_buffer: stm32_storage::Uart4TxBuffer = [0; mspv1::OSD_TX_BUFFER_LEN],
        spi1_dma_buffers: stm32_storage::SpiDmaBufferBank =
            stm32_storage::new_spi_dma_buffer_bank(),
        spi1_free_queue: stm32_storage::SpiFreeQueue =
            stm32_storage::SpiFreeQueue::new(),
        spi1_filled_queue: stm32_storage::SpiFilledQueue =
            stm32_storage::SpiFilledQueue::new(),
        adc1_buffers: stm32_storage::AdcBufferBank =
            stm32_storage::new_adc_buffer_bank(),
        dshot_dma_storage: board::init::DshotDmaStorage =
            board::init::DshotDmaStorage::new(),
    ])]
    fn init(cx: init::Context) -> (Shared, Local) {
        info!("Begin system init..");
        // Take ownership of peripherals and configure RCC
        let dp: hal::pac::Peripherals = cx.device;
        let ccdr = stm32_clocks::freeze_hse(
            dp.PWR,
            dp.RCC,
            &dp.SYSCFG,
            board::HSE_FREQUENCY_HZ,
        );
        let clocks = ccdr.clocks;
        let rec = ccdr.peripheral;
        Mono::start(cx.core.SYST, stm32_clocks::SYSTEM_CLOCK_HZ);
        let mut delay = ferrowasp_stm32h7::eh1::CycleDelay::new(stm32_clocks::SYSTEM_CLOCK_HZ);

        // Assign peripherals
        let dma1 = StreamsTuple::new(dp.DMA1, rec.DMA1);
        let dma2 = StreamsTuple::new(dp.DMA2, rec.DMA2);
        let gpioa = dp.GPIOA.split(rec.GPIOA);
        let gpiob = dp.GPIOB.split(rec.GPIOB);
        let gpioc = dp.GPIOC.split(rec.GPIOC);
        let gpiod = dp.GPIOD.split(rec.GPIOD);
        let gpioe = dp.GPIOE.split(rec.GPIOE);

        // The scheduler and control loop run at the IMU's ODR; IMU sampling
        // itself is triggered by PB2/EXTI2.
        let scheduler_rate: Rate<u32, 1, 1> = board::profiles::SCHEDULER_TICK_RATE_HZ.Hz();
        let control_loop_rate: Rate<u32, 1, 1> = board::profiles::CONTROL_LOOP_RATE_HZ.Hz();
        let samples_per_control_loop = scheduler_rate.to_Hz() / control_loop_rate.to_Hz();

        let adc1_battery = board::init::init_adc1_battery(
            board::init::Adc1BatteryResources {
                adc: dp.ADC1,
                prec: rec.ADC12,
                voltage_pin: gpioc.pc0,
                current_pin: gpioc.pc1,
            },
            &mut delay,
            &clocks,
            stm32_storage::AdcStorageResources {
                buffers: cx.local.adc1_buffers,
            },
        );

        let control_loop_scheduler = ferrowasp_stm32h7::timers::init_control_scheduler(
            dp.TIM4,
            rec.TIM4,
            &clocks,
            scheduler_rate.to_Hz(),
        );
        let io_timebase = ferrowasp_stm32h7::timers::MicrosecondTimebase::new(dp.TIM2, rec.TIM2, &clocks);
        let io_watchdog = ferrowasp_stm32h7::timers::init_io_watchdog(dp.TIM6, rec.TIM6, &clocks);

        let (usb_dev, usb_serial) = stm32_usb::init_usb_cdc_serial(
            stm32_usb::UsbResources {
                global: dp.OTG2_HS_GLOBAL,
                device: dp.OTG2_HS_DEVICE,
                pwrclk: dp.OTG2_HS_PWRCLK,
                dm: gpioa.pa11,
                dp: gpioa.pa12,
                prec: rec.USB2OTG,
            },
            &clocks,
            board::USB_CDC_IDENTITY,
        )
        .unwrap();

        // Set-up Routing
        let mut rc_input_uart = None;
        let mut osd_uart = None;
        let mut tele_uart = None;
        let mut gps_uart = None;

        // ------------  Logical UART1: UART8 / BLHeli legacy ESC telemetry  ------------
        // Optional observational bring-up path: ESC TLM -> PE0 UART8_RX.
        let (uart1_rx, esc_telemetry_uart) = {
            let uart1 = stm32_uart::init_uart8_esc_telemetry(
                stm32_uart::Uart8EscTelemetryResources {
                    rx_pin: gpioe.pe0,
                    uart: dp.UART8,
                    prec: rec.UART8,
                    rx_dma: dma1.2,
                },
                &clocks,
                stm32_storage::UartRxStorageResources {
                    buffers: cx.local.uart1_rx_buffers,
                    free_queue: cx.local.uart1_free_queue,
                    filled_queue: cx.local.uart1_filled_queue,
                }
                .into_backend(),
            );
            (uart1.irq, uart1.parser)
        };

        // ------------  Logical UART2: USART6 / SBUS RC  ------------
        let uart2 = stm32_uart::init_usart6_sbus(
            stm32_uart::Usart6SbusResources {
                tx_pin: gpioc.pc6,
                rx_pin: gpioc.pc7,
                usart: dp.USART6,
                prec: rec.USART6,
                rx_dma: dma1.0,
            },
            &clocks,
            stm32_storage::UartRxStorageResources {
                buffers: cx.local.uart2_rx_buffers,
                free_queue: cx.local.uart2_free_queue,
                filled_queue: cx.local.uart2_filled_queue,
            }
            .into_backend(),
        );
        route_uart_to_task(
            UART2_CONSUMER,
            uart2.parser,
            &mut rc_input_uart,
            &mut osd_uart,
            &mut tele_uart,
            &mut gps_uart,
        );
        // ------------  Logical UART4: USART3 / DJI MSP OSD  ------------
        // Board connection: PD8 USART3_TX -> DJI RX, PD9 USART3_RX <- DJI TX.
        let uart4 = stm32_uart::init_usart3_msp_osd(
            stm32_uart::Usart3MspResources {
                tx_pin: gpiod.pd8,
                rx_pin: gpiod.pd9,
                usart: dp.USART3,
                prec: rec.USART3,
                rx_dma: dma1.1,
                tx_dma: dma1.3,
            },
            &clocks,
            stm32_storage::UartRxStorageResources {
                buffers: cx.local.uart4_rx_buffers,
                free_queue: cx.local.uart4_free_queue,
                filled_queue: cx.local.uart4_filled_queue,
            }
            .into_backend(),
            cx.local.uart4_tx_buffer,
        );
        route_uart_to_task(
            UART4_CONSUMER,
            uart4.parser,
            &mut rc_input_uart,
            &mut osd_uart,
            &mut tele_uart,
            &mut gps_uart,
        );
        let rc_input_uart = rc_input_uart
            .take()
            .expect("board support must route USART6 to the RC input task");
        let uart2_owned_rx =
            cortex_m::singleton!(: Uart2OwnedRxChannel = Uart2OwnedRxChannel::new()).unwrap();
        let (rc_rx_producer, rc_rx_reader, rc_rx_discontinuities) = uart2_owned_rx.split();
        let uart2_bridge = Uart2OwnedRxBridge::new(rc_input_uart, rc_rx_producer);
        let osd_tx_dma = uart4.tx_dma;
        let uart4_owned_rx =
            cortex_m::singleton!(: Uart4OwnedRxChannel = Uart4OwnedRxChannel::new()).unwrap();
        let (osd_rx_producer, osd_rx_reader, osd_rx_discontinuities) = uart4_owned_rx.split();
        let uart4_owned_tx =
            cortex_m::singleton!(: Uart4OwnedTxChannel = Uart4OwnedTxChannel::new()).unwrap();
        let (osd_tx_writer, uart4_tx_owner, uart4_tx_completion) = uart4_owned_tx.split();

        let (
            esc_manager_state,
            esc_request_producer,
            esc_request_consumer,
            esc_ack_producer,
            esc_ack_consumer,
            esc_telemetry_update_producer,
            esc_telemetry_update_consumer,
        ) = {
            let requests =
                cortex_m::singleton!(: esc::EscRequestQueue = esc::EscRequestQueue::new()).unwrap();
            let acknowledgements =
                cortex_m::singleton!(: esc::EscAckQueue = esc::EscAckQueue::new()).unwrap();
            let (request_producer, request_consumer) = requests.split();
            let (ack_producer, ack_consumer) = acknowledgements.split();
            let updates = cortex_m::singleton!(
                : esc::EscTelemetryUpdateQueue = esc::EscTelemetryUpdateQueue::new()
            )
            .unwrap();
            let (update_producer, update_consumer) = updates.split();
            (
                esc::EscManager::new(esc::EscManagerConfig::legacy_uart(), 0),
                request_producer,
                request_consumer,
                ack_producer,
                ack_consumer,
                update_producer,
                update_consumer,
            )
        };

        let dshot_motors = board::init::init_dshot_motor_bank(
            board::init::DshotMotorBankResources {
                tim3: dp.TIM3,
                tim3_rec: rec.TIM3,
                tim5: dp.TIM5,
                tim5_rec: rec.TIM5,
                motor1_pin: gpiob.pb0,
                motor2_pin: gpiob.pb1,
                motor3_pin: gpioa.pa0,
                motor4_pin: gpioa.pa1,
                motor1_dma: dma2.0,
                motor2_dma: dma2.1,
                motor3_dma: dma2.2,
                motor4_dma: dma2.3,
            },
            &clocks,
            cx.local.dshot_dma_storage,
        )
        .expect("TBS Lucid H7 DShot600 timing must be valid");

        let spi1_imu = board::init::init_spi1_imu(
            board::init::Spi1ImuResources {
                cs_pin: gpioc.pc15,
                sck_pin: gpioa.pa5,
                miso_pin: gpioa.pa6,
                mosi_pin: gpiod.pd7,
                spi: dp.SPI1,
                prec: rec.SPI1,
                rx_dma: dma1.4,
                tx_dma: dma1.5,
            },
            &clocks,
            &mut delay,
            stm32_storage::SpiDmaStorageResources {
                buffers: cx.local.spi1_dma_buffers,
                free_queue: cx.local.spi1_free_queue,
                filled_queue: cx.local.spi1_filled_queue,
            },
        );
        let mut syscfg = dp.SYSCFG;
        let mut exti = dp.EXTI;
        let imu_data_ready = board::init::init_imu_data_ready(gpiob.pb2, &mut syscfg, &mut exti);
        let spi1_device = AsyncSpiDevice::new(CriticalSectionSpiExecutor::new(
            &SPI1_MAILBOX,
            SpiDeadlineUs(SPI1_IMU_DEADLINE_US),
            || {
                let _ = spi1_owner_service::spawn();
            },
        ));
        if let Some(kind) = spi1_imu.bringup.kind() {
            ACTIVE_IMU_KIND.store(kind as u8, Ordering::Relaxed);
        }
        IMU_TRANSPORT_READY.store(spi1_imu.bringup.is_ready(), Ordering::Relaxed);

        let (
            flash_device,
            flash_record_producer,
            flash_record_consumer,
            flash_command_producer,
            flash_command_consumer,
            flash_response_producer,
            flash_response_consumer,
            flash_rpc_command_producer,
            flash_rpc_command_consumer,
            flash_rpc_response_producer,
            flash_rpc_response_consumer,
        ) = {
            let mut flash = board::init::init_sd_flash(
                board::init::SdCardResources {
                    sdmmc: dp.SDMMC1,
                    prec: rec.SDMMC1,
                    clk: gpioc.pc12,
                    cmd: gpiod.pd2,
                    d0: gpioc.pc8,
                    d1: gpioc.pc9,
                    d2: gpioc.pc10,
                    d3: gpioc.pc11,
                },
                &clocks,
            );
            match flash.read_jedec_id() {
                Ok(id) => {
                    FLASH_JEDEC_MANUFACTURER.store(id.manufacturer, Ordering::Relaxed);
                    FLASH_JEDEC_MEMORY_TYPE.store(id.memory_type, Ordering::Relaxed);
                    FLASH_JEDEC_CAPACITY_CODE.store(id.capacity_code, Ordering::Relaxed);
                    let capacity = id.capacity_bytes().unwrap_or(0);
                    FLASH_CAPACITY_BYTES.store(capacity, Ordering::Relaxed);
                    let supported =
                        id.plausible() && flash_task::StorageLayout::new(capacity).is_some();
                    FLASH_READY.store(supported, Ordering::Release);
                    info!(
                        "SD storage {:02x}:{:02x}:{:02x}, capacity {} bytes, supported {}",
                        id.manufacturer, id.memory_type, id.capacity_code, capacity, supported
                    );
                }
                Err(error) => warn!(
                    "SD storage unavailable ({}); storage remains disabled",
                    defmt::Debug2Format(&error)
                ),
            }
            let (producer, consumer) = {
                let queue = cortex_m::singleton!(
                    : flash_task::RecordQueue = flash_task::RecordQueue::new()
                )
                .unwrap();
                queue.split()
            };
            let commands = cortex_m::singleton!(
                : flash_task::CommandQueue = flash_task::CommandQueue::new()
            )
            .unwrap();
            let responses = cortex_m::singleton!(
                : flash_task::ResponseQueue = flash_task::ResponseQueue::new()
            )
            .unwrap();
            let (command_producer, command_consumer) = commands.split();
            let (response_producer, response_consumer) = responses.split();
            #[cfg(feature = "mspv2_configurator")]
            let (
                rpc_command_producer,
                rpc_command_consumer,
                rpc_response_producer,
                rpc_response_consumer,
            ) = {
                let commands = cortex_m::singleton!(
                    : flash_task::RpcCommandQueue = flash_task::RpcCommandQueue::new()
                )
                .unwrap();
                let responses = cortex_m::singleton!(
                    : flash_task::RpcResponseQueue = flash_task::RpcResponseQueue::new()
                )
                .unwrap();
                let (command_producer, command_consumer) = commands.split();
                let (response_producer, response_consumer) = responses.split();
                (
                    command_producer,
                    command_consumer,
                    response_producer,
                    response_consumer,
                )
            };
            #[cfg(not(feature = "mspv2_configurator"))]
            let (
                rpc_command_producer,
                rpc_command_consumer,
                rpc_response_producer,
                rpc_response_consumer,
            ) = ((), (), (), ());
            (
                flash,
                producer,
                consumer,
                command_producer,
                command_consumer,
                response_producer,
                response_consumer,
                rpc_command_producer,
                rpc_command_consumer,
                rpc_response_producer,
                rpc_response_consumer,
            )
        };

        // Init rate controller
        // The Foxeer tuning: the Lucid starts from the same airframe profile.
        let tuning_profile = dt::TuningProfile::default_foxeer_f405_v2();
        let flight_controller = dt::FlightController::new(
            dt::FlightControllerConfig::default(),
            dt::RateController::new(tuning_profile.rate_gains, dt::RATE_CONTROLLER_OUTPUT_LIMIT),
        );

        // ############ SAFETY HANDLES ###########
        let (rc_arm_high_writer, rc_arm_high_reader) = signals::split_rc_arm_high(&RC_ARM_HIGH);
        let (rc_throttle_writer, rc_throttle_reader) = signals::split_rc_throttle(&RC_THROTTLE);
        let (safety_arm_writer, safety_arm_reader) = signals::split_safety_arm(&SAFETY_ARMED);
        let (rc_rates_writer, rc_rates_reader) = signals::split_rc_rates(&RC_RATES);
        let (rc_link_frame_writer, rc_link_invalidator, rc_link_reader) =
            signals::split_rc_link(&RC_LINK);
        let (actuator_arm_done_writer, actuator_arm_done_reader) =
            signals::split_actuator_arm_done(&ACTUATOR_ARM_DONE);
        let (actuator_arm_permit_writer, actuator_arm_permit_reader) =
            signals::split_actuator_arm_permit(&ACTUATOR_ARM_PERMIT);
        let motor_cmd_q = cortex_m::singleton!(
            : safety::signals::MotorCmdQueue = safety::signals::MotorCmdQueue::new()
        )
        .unwrap();
        let (motor_cmd_writer, motor_cmd_reader) =
            safety::signals::split_motor_cmd_queue(motor_cmd_q);

        // --- Boot-strap program ---
        info!("{} system init successful", board::BOARD_IDENTITY.name);
        info!("FerroWasp RTT hello from TBS Lucid H7");
        match spi1_imu.bringup {
            board::init::Spi1ImuBringupStatus::Ready { kind, who_am_i } => match kind {
                Spi1ImuKind::Mpu6500 => {
                    info!("TBS Lucid H7 MPU6500 ready; WHO_AM_I {}", who_am_i);
                }
                Spi1ImuKind::Icm42688P => {
                    info!("TBS Lucid H7 ICM42688-P ready; WHO_AM_I {}", who_am_i);
                }
                Spi1ImuKind::Mpu6000 => {
                    info!("TBS Lucid H7 MPU-6000 ready; WHO_AM_I {}", who_am_i);
                }
            },
            board::init::Spi1ImuBringupStatus::UnsupportedIdentity { who_am_i } => {
                warn!(
                    "TBS Lucid H7 IMU unsupported; WHO_AM_I {}, sampling disabled",
                    who_am_i
                );
            }
            board::init::Spi1ImuBringupStatus::ProbeFailed => {
                warn!("TBS Lucid H7 IMU identity probe failed; sampling disabled");
            }
            board::init::Spi1ImuBringupStatus::ConfigurationFailed { kind, who_am_i } => match kind
            {
                Spi1ImuKind::Mpu6500 => {
                    warn!(
                        "TBS Lucid H7 MPU6500 configuration failed; WHO_AM_I {}, sampling disabled",
                        who_am_i
                    );
                }
                Spi1ImuKind::Icm42688P => {
                    warn!(
                        "TBS Lucid H7 ICM42688-P configuration failed; WHO_AM_I {}, sampling disabled",
                        who_am_i
                    );
                }
                Spi1ImuKind::Mpu6000 => {
                    warn!(
                        "TBS Lucid H7 MPU-6000 configuration failed; WHO_AM_I {}, sampling disabled",
                        who_am_i
                    );
                }
            },
        }
        if SMOKE_ACTUATOR_INHIBIT_ENABLED {
            warn!("Flight arming inhibited: {}", ACTUATOR_INHIBIT_REASON);
        } else if BENCH_ACTUATOR_VALIDATION_ENABLED {
            warn!("PROPS OFF: capped TBS Lucid H7 actuator-validation mode enabled");
            warn!("Normal mixer output is not active in this commissioning image");
        } else if !FLIGHT_ARMING_ENABLED {
            warn!("Flight arming inhibited: {}", ARMING_INHIBIT_REASON);
        } else if !spi1_imu.bringup.kind().is_some_and(imu_kind_flight_verified) {
            warn!("Flight arming inhibited: the fitted IMU is not verified on this board");
        } else {
            info!("TBS Lucid H7 flight arming enabled with runtime IMU health checks");
            info!("TBS Lucid H7 ADC uses Betaflight target voltage/current values");
        }
        info!("TBS Lucid H7 BLHeli telemetry-qualified DShot arming active on PE0 UART8 RX");
        #[cfg(feature = "bench_dshot_idle_output1_not_running")]
        warn!(
            "FAULT INJECTION ACTIVE: TBS Lucid H7 physical ESC output 1 (logical M1/rear-right) idle qualification eRPM forced to zero; arming must fail"
        );
        #[cfg(feature = "bench_prearm_imu_stale")]
        warn!("FAULT INJECTION ACTIVE: pre-arm IMU freshness forced stale; arming must fail");
        heartbeat::spawn().unwrap();
        adc1_polling::spawn().ok();
        uart4_tx_worker::spawn().unwrap();
        rc_input::spawn().unwrap();
        osd_refresh::spawn().ok();
        dshot_service::spawn().unwrap();
        esc_manager_task::spawn().unwrap();
        flash_manager_task::spawn().unwrap();

        (
            Shared {
                uart1_rx,
                uart2_rx: uart2.irq,
                uart2_bridge,
                uart4_rx: uart4.rx_irq,
                uart4_tx_dma: osd_tx_dma,

                // SPI1
                spi1_owner: spi1_imu.owner,
                io_timebase,
                dshot_motors,

                // IMU
                imu_data: imu::ImuData::default(), // all values are zero
                imu_angles: [0.0; 3],
                imu_rates: [0.0; 3],
                tuning_profile,
                tuning_request_seq: 0,

                // ADC
                adc1_transfer: adc1_battery.transfer,
                battery_voltage_v10: 0,
                battery_cell_count: 0,
                battery_cell_voltage_v100: 0,
                battery_current_ca: 0,
                // SAFETY
            },
            Local {
                // Safety
                arm_qualifier: safety::ArmQualifier::default(),

                // UART
                sbus: StreamingParser::new(),
                esc_telemetry_uart,
                esc_manager_state,
                esc_request_producer,
                esc_request_consumer,
                esc_ack_producer,
                esc_ack_consumer,
                esc_telemetry_update_producer,
                esc_telemetry_update_consumer,

                // SPI1
                spi1_parser: spi1_imu.parser,
                spi1_device,
                imu_data_ready,
                flash_device,
                flash_record_producer,
                flash_record_consumer,
                flash_command_producer,
                flash_command_consumer,
                flash_response_producer,
                flash_response_consumer,
                flash_rpc_command_producer,
                flash_rpc_command_consumer,
                flash_rpc_response_producer,
                flash_rpc_response_consumer,

                // ADC
                adc1_buffer: Some(adc1_battery.spare_buffer),

                // Control Loop
                control_loop_cnt: 0,
                samples_per_control_loop,
                flight_controller,
                imu_rate_filter: dt::ImuRateLowPassFilter::new(dt::gyro_lpf_alpha(
                    dt::IMU_GYRO_LPF_HZ,
                    board::profiles::CONTROL_LOOP_RATE_HZ as f32,
                )),
                imu_angle_integrator: dt::GyroAngleIntegrator::new(),
                gyro_axis_map: CONTROL_IMU_TO_RATE_CONTROLLER_MAP,
                gyro_bias_calibrator: dt::GyroBiasCalibrator::new(
                    GYRO_BIAS_CALIBRATION_SAMPLES,
                    GYRO_BIAS_CALIBRATION_MAX_RAW,
                ),
                control_loop_scheduler,
                io_watchdog,
                imu_last_sequence: 0,
                imu_stale_ticks: 0,
                applied_tuning_seq: 0,

                // Parser
                rc_rx_reader,
                rc_rx_discontinuities,
                osd_uart,
                osd_rx_producer,
                osd_rx_reader,
                osd_rx_discontinuities,
                osd_tx_writer,
                osd_tx_healthy: true,
                uart4_tx_owner,
                uart4_tx_completion,
                osd_task: osd::OsdTask::new(),
                osd_tx_buffer: [0; mspv1::OSD_TX_BUFFER_LEN],
                osd_refresh_tick: 0,
                //tele_uart,
                //gps_uart,

                // ----  SAFETY  ----
                // rc_input Writer
                rc_arm_high_writer,
                rc_throttle_writer,
                rc_link_frame_writer,

                // safety_master writer
                safety_arm_writer,
                rc_link_invalidator,

                // safety_master reader
                safety_rc_arm_high_reader: rc_arm_high_reader,
                safety_rc_throttle_reader: rc_throttle_reader,
                safety_rc_link_reader: rc_link_reader,

                // control_loop reader
                control_safety_arm_reader: safety_arm_reader,
                control_throttle_reader: rc_throttle_reader,
                control_rc_link_reader: rc_link_reader,
                control_arm_permit_reader: actuator_arm_permit_reader,

                // actuator reader
                actuator_safety_arm_reader: safety_arm_reader,
                actuator_rc_arm_high_reader: rc_arm_high_reader,
                actuator_rc_throttle_reader: rc_throttle_reader,
                actuator_rc_link_reader: rc_link_reader,
                osd_safety_arm_reader: safety_arm_reader,
                osd_rc_throttle_reader: rc_throttle_reader,
                osd_rc_rates_reader: rc_rates_reader,
                usb_rc_link_reader: rc_link_reader,
                actuator_arm_done_writer,
                actuator_arm_done_reader,
                actuator_arm_permit_writer,
                actuator_arm_permit_reader,
                motor_cmd_writer,
                motor_cmd_reader,
                motor_cmd_seq: 0,

                // RC Rates
                rc_rates_writer,
                rc_rates_reader,

                // USB CDC serial
                usb_dev,
                usb_serial,
                usb_header_sent: false,
                configurator_usb: ConfiguratorUsbState::new(),
            },
        )
    }

    // ----  SAFETY MASTER  ----
    //---------------------------------------------------------------------------------------------------------------------------
    #[task(
        from = flight_tasks::safety_master,
        priority = 16,
        local = [
            safety_rc_arm_high_reader,
            safety_rc_throttle_reader,
            safety_rc_link_reader,
            safety_arm_writer,
            rc_link_invalidator,
            actuator_arm_permit_writer,
            actuator_arm_done_reader
        ],
        spawn = [actuator_output],
        config = [
            actuator_output_enabled: bool = ACTUATOR_OUTPUT_ENABLED,
            actuator_inhibit_reason: &'static str = ACTUATOR_INHIBIT_REASON,
            bench_actuator_validation_enabled: bool = BENCH_ACTUATOR_VALIDATION_ENABLED,
            validate_live_arming_guard: fn(
                bool,
                bool,
                bool,
                u32,
            ) -> Result<(), safety::ArmingAbortReason> = validate_live_arming_guard,
        ]
    )]
    async fn safety_master(cx: safety_master::Context, event: safety::SafetyEvent);
    //---------------------------------------------------------------------------------------------------------------------------

    // ---- USB CDC READ-ONLY DEBUG ----
    #[task(
        binds = OTG_FS,
        priority = 5,
        local = [
            usb_dev,
            usb_serial,
            usb_header_sent,
            flash_command_producer,
            flash_response_consumer,
            flash_rpc_command_producer,
            flash_rpc_response_consumer,
            flash_command_parser: flash_task::CommandParser = flash_task::CommandParser::new(),
            flash_pending_response: Option<flash_task::ResponseFrame> = None,
            configurator_usb
        ]
    )]
    fn usb_fs(cx: usb_fs::Context) {
        let usb_dev = cx.local.usb_dev;
        let serial = cx.local.usb_serial;

        let _ = usb_dev.poll(&mut [serial]);

        let mut rx_buf = [0u8; 64];
        let read_len = serial.read(&mut rx_buf).unwrap_or(0);

        #[cfg(not(feature = "mspv2_configurator"))]
        for byte in &rx_buf[..read_len] {
            let Some(parsed) = cx.local.flash_command_parser.ingest(*byte) else {
                continue;
            };
            match parsed {
                Ok(command) => {
                    if cx.local.flash_command_producer.enqueue(command).is_err() {
                        let _ = serial.write(b"ERR command queue full\r\n");
                    }
                }
                Err(_) => {
                    let _ = serial.write(b"ERR invalid command; type help\r\n");
                }
            }
        }

        #[cfg(feature = "mspv2_configurator")]
        for byte in &rx_buf[..read_len] {
            if let Ok(Some(packet)) = cx.local.configurator_usb.parser.parse(*byte) {
                handle_msp_packet(
                    packet,
                    cx.local.configurator_usb,
                    cx.local.flash_rpc_command_producer,
                );
            }
        }

        if usb_dev.state() != UsbDeviceState::Configured {
            *cx.local.usb_header_sent = false;
            #[cfg(not(feature = "mspv2_configurator"))]
            cx.local.flash_command_parser.clear();
            #[cfg(feature = "mspv2_configurator")]
            {
                cx.local.configurator_usb.parser.clear();
                cx.local.configurator_usb.pending_tx.clear();
            }
            return;
        }

        if serial.flush().is_err() {
            return;
        }

        #[cfg(feature = "mspv2_configurator")]
        {
            if !cx.local.configurator_usb.pending_tx.is_pending()
                && let Some(response) = cx.local.flash_rpc_response_consumer.dequeue()
            {
                let _ = stage_rpc_response(cx.local.configurator_usb, &response);
            }
            if cx.local.configurator_usb.pending_tx.is_pending()
                && let Ok(written) = serial.write(cx.local.configurator_usb.pending_tx.remaining())
            {
                cx.local.configurator_usb.pending_tx.advance(written);
            }
            return;
        }

        #[cfg(not(feature = "mspv2_configurator"))]
        if !*cx.local.usb_header_sent {
            if matches!(
                serial.write(USB_DEBUG_HEADER),
                Ok(written) if written == USB_DEBUG_HEADER.len()
            ) {
                *cx.local.usb_header_sent = true;
            }
            return;
        }

        #[cfg(not(feature = "mspv2_configurator"))]
        {
            if cx.local.flash_pending_response.is_none() {
                *cx.local.flash_pending_response = cx.local.flash_response_consumer.dequeue();
            }
            if let Some(response) = cx.local.flash_pending_response.as_ref() {
                if matches!(
                    serial.write(response.as_bytes()),
                    Ok(written) if written == response.as_bytes().len()
                ) {
                    *cx.local.flash_pending_response = None;
                    cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
                }
                return;
            }
        }

        #[cfg(not(feature = "mspv2_configurator"))]
        if !USB_DEBUG_DUE.swap(false, Ordering::AcqRel) {
            return;
        }

        #[cfg(not(feature = "mspv2_configurator"))]
        let snapshot = usb_debug::StatusSnapshot {
            // The status format carries 32-bit milliseconds, 49.7 days.
            uptime_ms: Mono::now().duration_since_epoch().to_millis() as u32,
            imu_kind: active_usb_debug_imu_kind(),
            imu_ready: IMU_TRANSPORT_READY.load(Ordering::Relaxed),
            imu_sequence: IMU_LATEST_SEQ.load(Ordering::Relaxed),
            gyro_raw: [
                IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed),
                IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed),
                IMU_LATEST_YAW_RAW.load(Ordering::Relaxed),
            ],
            imu_stale: IMU_STALE.load(Ordering::Relaxed),
            control_sequence: CONTROL_RATE_SEQ.load(Ordering::Relaxed),
            control_loop_hz: board::profiles::CONTROL_LOOP_RATE_HZ,
            rc_valid: USB_RC_VALID_SNAPSHOT.load(Ordering::Relaxed),
            rc_armable: USB_RC_ARMABLE_SNAPSHOT.load(Ordering::Relaxed),
            rc_throttle: RC_THROTTLE.load(Ordering::Relaxed),
            rc_arm_high: RC_ARM_HIGH.load(Ordering::Relaxed),
            system_armed: SAFETY_ARMED.load(Ordering::Relaxed),
            battery_voltage_decivolts: BATTERY_VOLTAGE_V10_SNAPSHOT.load(Ordering::Relaxed),
            battery_current_centiamps: BATTERY_CURRENT_CA_SNAPSHOT.load(Ordering::Relaxed),
            adc_voltage_mv: ADC_VOLTAGE_MV_SNAPSHOT.load(Ordering::Relaxed),
            adc_current_mv: ADC_CURRENT_MV_SNAPSHOT.load(Ordering::Relaxed),
        };
        #[cfg(not(feature = "mspv2_configurator"))]
        let Ok(line) = usb_debug::format_status(snapshot) else {
            USB_DEBUG_DUE.store(true, Ordering::Release);
            return;
        };

        #[cfg(not(feature = "mspv2_configurator"))]
        if !matches!(
            serial.write(line.as_bytes()),
            Ok(written) if written == line.len()
        ) {
            USB_DEBUG_DUE.store(true, Ordering::Release);
        }
    }

    // IDLE TASK

    #[task(
        from = flight_tasks::heartbeat,
        priority = 1,
        local = [usb_rc_link_reader],
        config = [
            physical_imu_to_drone_rotation: dt::FrameRotation =
                IMU_CONTROL_AXIS_PROFILE.imu_to_drone_rotation(),
            control_imu_to_rate_controller_map: dt::FrameRotation =
                IMU_CONTROL_AXIS_PROFILE.imu_to_rate_controller_map(),
        ]
    )]
    async fn heartbeat(cx: heartbeat::Context);

    #[task(
        priority = 1,
        local = [
            flash_device,
            flash_record_consumer,
            flash_command_consumer,
            flash_response_producer,
            flash_rpc_command_consumer,
            flash_rpc_response_producer,
            assembler: flash_task::PageAssembler = flash_task::PageAssembler::new(),
            boot_session_start_pending: bool = true,
            pending_page: Option<[u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN]> = None,
            initialized: bool = false,
            next_page_index: u32 = 0,
            next_flight_id: u32 = 1,
            log_region_writable: bool = false,
            stored_config: flash_task::StoredConfig =
                flash_task::StoredConfig::foxeer_f405_v2_default(),
            config_sequence: u32 = 0,
            config_active_slot: u8 = 1,
            erase_sector_index: Option<u32> = None,
            flash_test_phase: u8 = 0,
            config_save_phase: u8 = 0,
            config_save_slot: u8 = 0,
            config_save_candidate: flash_task::StoredConfig =
                flash_task::StoredConfig::foxeer_f405_v2_default(),
            config_save_page: [u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN] =
                [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN],
            staged_rpc_config: Option<flash_task::StoredConfig> = None,
            config_rpc_completion: Option<ConfigRpcCompletion> = None
        ],
        shared = [tuning_profile, tuning_request_seq]
    )]
    async fn flash_manager_task(mut cx: flash_manager_task::Context) {
        let flash_manager_task::LocalResources {
            flash_device,
            flash_record_consumer,
            flash_command_consumer,
            flash_response_producer,
            flash_rpc_command_consumer,
            flash_rpc_response_producer,
            assembler,
            boot_session_start_pending,
            pending_page,
            initialized,
            next_page_index,
            next_flight_id,
            log_region_writable,
            stored_config,
            config_sequence,
            config_active_slot,
            erase_sector_index,
            flash_test_phase,
            config_save_phase,
            config_save_slot,
            config_save_candidate,
            config_save_page,
            staged_rpc_config,
            config_rpc_completion,
            ..
        } = cx.local;
        #[cfg(not(feature = "mspv2_configurator"))]
        let _ = (
            flash_rpc_command_consumer,
            flash_rpc_response_producer,
            staged_rpc_config,
            config_rpc_completion,
        );
        loop {
            {
                if !FLASH_READY.load(Ordering::Acquire) {
                    #[cfg(feature = "mspv2_configurator")]
                    if let Some(request) = flash_rpc_command_consumer.dequeue() {
                        let _ = queue_rpc_response(
                            flash_rpc_response_producer,
                            mspv2::rpc::error(
                                request.request_id,
                                mspv2::rpc::DeviceError::StorageUnavailable,
                            ),
                        );
                    }
                    Mono::delay(100u64.millis()).await;
                    continue;
                }
                let Some(layout) =
                    flash_task::StorageLayout::new(FLASH_CAPACITY_BYTES.load(Ordering::Relaxed))
                else {
                    FLASH_READY.store(false, Ordering::Release);
                    #[cfg(feature = "mspv2_configurator")]
                    if let Some(request) = flash_rpc_command_consumer.dequeue() {
                        let _ = queue_rpc_response(
                            flash_rpc_response_producer,
                            mspv2::rpc::error(
                                request.request_id,
                                mspv2::rpc::DeviceError::StorageUnavailable,
                            ),
                        );
                    }
                    continue;
                };

                if !*initialized {
                    let Ok((page, flight, writable)) = scan_flash_log(flash_device, layout) else {
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SD storage log recovery failed; storage disabled");
                        continue;
                    };
                    let Ok((config, sequence, slot)) = load_flash_config(flash_device, layout)
                    else {
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SD storage configuration read failed; storage disabled");
                        continue;
                    };
                    *next_page_index = page;
                    *next_flight_id = flight;
                    *log_region_writable = writable;
                    *stored_config = config;
                    *config_save_candidate = config;
                    *config_sequence = sequence;
                    *config_active_slot = slot;
                    FLASH_LOG_RATE_DIVISOR
                        .store(u32::from(config.log_rate_divisor), Ordering::Relaxed);
                    cx.shared
                        .tuning_profile
                        .lock(|profile| *profile = config.tuning);
                    cx.shared.tuning_request_seq.lock(|request_sequence| {
                        *request_sequence = request_sequence.wrapping_add(1)
                    });
                    *initialized = true;
                    info!(
                        "SD storage recovered at log page {}, next flight {}, writable {}, config seq {}",
                        page, flight, writable, sequence
                    );
                }

                let status = match flash_device.read_status() {
                    Ok(status) => status,
                    Err(_) => {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SD storage status read failed; storage disabled");
                        #[cfg(feature = "mspv2_configurator")]
                        let _ = finish_config_rpc_error(
                            flash_rpc_response_producer,
                            config_rpc_completion,
                            mspv2::rpc::DeviceError::StorageUnavailable,
                        );
                        continue;
                    }
                };
                if status.busy() {
                    Mono::delay(1u64.millis()).await;
                    continue;
                }

                if SAFETY_ARMED.load(Ordering::Acquire)
                    && (erase_sector_index.is_some()
                        || *flash_test_phase != 0
                        || *config_save_phase != 0)
                {
                    *erase_sector_index = None;
                    *flash_test_phase = 0;
                    *config_save_phase = 0;
                    #[cfg(feature = "mspv2_configurator")]
                    let rpc_notified = finish_config_rpc_error(
                        flash_rpc_response_producer,
                        config_rpc_completion,
                        mspv2::rpc::DeviceError::Armed,
                    );
                    #[cfg(not(feature = "mspv2_configurator"))]
                    let rpc_notified = false;
                    if !rpc_notified {
                        queue_storage_response(
                            flash_response_producer,
                            "ERR maintenance aborted because system armed\r\n",
                        );
                    }
                }

                if let Some(sector) = *erase_sector_index {
                    if sector >= layout.log_sector_count() {
                        *erase_sector_index = None;
                        *next_page_index = 0;
                        *next_flight_id = 1;
                        *log_region_writable = true;
                        queue_storage_response(flash_response_producer, "OK logs erased\r\n");
                    } else {
                        let address =
                            layout.log_start_address + sector * flash_task::CONFIG_SECTOR_SIZE;
                        if flash_device.erase_sector_4k(address).is_err() {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            *erase_sector_index = None;
                            queue_storage_response(
                                flash_response_producer,
                                "ERR log sector erase failed\r\n",
                            );
                        } else {
                            *erase_sector_index = Some(sector + 1);
                        }
                    }
                    Mono::delay(1u64.millis()).await;
                    continue;
                }

                match *flash_test_phase {
                    1 => {
                        if flash_device
                            .erase_sector_4k(flash_task::SCRATCH_SECTOR_ADDRESS)
                            .is_err()
                        {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            *flash_test_phase = 0;
                            queue_storage_response(
                                flash_response_producer,
                                "ERR flash test scratch erase failed\r\n",
                            );
                        } else {
                            *flash_test_phase = 2;
                        }
                        continue;
                    }
                    2 => {
                        let page = flash_scratch_test_page();
                        if flash_device
                            .page_program(flash_task::SCRATCH_SECTOR_ADDRESS, &page)
                            .is_err()
                        {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            *flash_test_phase = 0;
                            queue_storage_response(
                                flash_response_producer,
                                "ERR flash test scratch program failed\r\n",
                            );
                        } else {
                            *flash_test_phase = 3;
                        }
                        continue;
                    }
                    3 => {
                        let expected = flash_scratch_test_page();
                        let mut actual = [0u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                        *flash_test_phase = 0;
                        if flash_device
                            .read(flash_task::SCRATCH_SECTOR_ADDRESS, &mut actual)
                            .is_ok()
                            && actual == expected
                        {
                            queue_storage_response(
                                flash_response_producer,
                                "OK flash scratch erase/program/read verified\r\n",
                            );
                        } else {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            queue_storage_response(
                                flash_response_producer,
                                "ERR flash test readback mismatch\r\n",
                            );
                        }
                        continue;
                    }
                    _ => {}
                }

                match *config_save_phase {
                    1 => {
                        let address = layout.config_slot_addresses[*config_save_slot as usize];
                        if flash_device.erase_sector_4k(address).is_err() {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            *config_save_phase = 0;
                            #[cfg(feature = "mspv2_configurator")]
                            let rpc_notified = finish_config_rpc_error(
                                flash_rpc_response_producer,
                                config_rpc_completion,
                                mspv2::rpc::DeviceError::WriteFailure,
                            );
                            #[cfg(not(feature = "mspv2_configurator"))]
                            let rpc_notified = false;
                            if !rpc_notified {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR config slot erase failed\r\n",
                                );
                            }
                        } else {
                            *config_save_phase = 2;
                        }
                        continue;
                    }
                    2 => {
                        let address = layout.config_slot_addresses[*config_save_slot as usize];
                        if flash_device
                            .page_program(address, config_save_page)
                            .is_err()
                        {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            *config_save_phase = 0;
                            #[cfg(feature = "mspv2_configurator")]
                            let rpc_notified = finish_config_rpc_error(
                                flash_rpc_response_producer,
                                config_rpc_completion,
                                mspv2::rpc::DeviceError::WriteFailure,
                            );
                            #[cfg(not(feature = "mspv2_configurator"))]
                            let rpc_notified = false;
                            if !rpc_notified {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR config page program failed\r\n",
                                );
                            }
                        } else {
                            *config_save_phase = 3;
                        }
                        continue;
                    }
                    3 => {
                        let address = layout.config_slot_addresses[*config_save_slot as usize];
                        let mut persisted = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                        let expected_sequence = config_sequence.wrapping_add(1);
                        let expected_payload = config_save_candidate.encode();
                        let verified = flash_device.read(address, &mut persisted).is_ok()
                            && matches!(
                                ferrowasp_core::blackbox::decode_config_page(&persisted),
                                Ok((sequence, payload))
                                    if sequence == expected_sequence
                                        && payload == expected_payload.as_slice()
                            );
                        if !verified {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            *config_save_phase = 0;
                            #[cfg(feature = "mspv2_configurator")]
                            let rpc_notified = finish_config_rpc_error(
                                flash_rpc_response_producer,
                                config_rpc_completion,
                                mspv2::rpc::DeviceError::ChecksumMismatch,
                            );
                            #[cfg(not(feature = "mspv2_configurator"))]
                            let rpc_notified = false;
                            if !rpc_notified {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR config persistence verification failed\r\n",
                                );
                            }
                            continue;
                        }
                        *stored_config = *config_save_candidate;
                        *config_active_slot = *config_save_slot;
                        *config_sequence = config_sequence.wrapping_add(1);
                        *config_save_phase = 0;
                        FLASH_LOG_RATE_DIVISOR
                            .store(u32::from(stored_config.log_rate_divisor), Ordering::Relaxed);
                        cx.shared
                            .tuning_profile
                            .lock(|profile| *profile = stored_config.tuning);
                        cx.shared
                            .tuning_request_seq
                            .lock(|sequence| *sequence = sequence.wrapping_add(1));
                        #[cfg(feature = "mspv2_configurator")]
                        let rpc_notified = if let Some(completion) = config_rpc_completion.take() {
                            let crc = stored_config.crc32();
                            let (request_id, response) = match completion {
                                ConfigRpcCompletion::Commit(id) => (
                                    id,
                                    mspv2::rpc::Response::ConfigCommitted {
                                        persisted_crc32: crc,
                                        reboot_required: false,
                                    },
                                ),
                                ConfigRpcCompletion::Defaults(id) => (
                                    id,
                                    mspv2::rpc::Response::DefaultsRestored {
                                        persisted_crc32: crc,
                                        reboot_required: false,
                                    },
                                ),
                            };
                            queue_rpc_response(
                                flash_rpc_response_producer,
                                mspv2::rpc::ok(request_id, response),
                            )
                        } else {
                            false
                        };
                        #[cfg(not(feature = "mspv2_configurator"))]
                        let rpc_notified = false;
                        if !rpc_notified {
                            queue_storage_response(flash_response_producer, "OK config saved\r\n");
                        }
                    }
                    _ => {}
                }

                if let Some(command) = flash_command_consumer.dequeue() {
                    let mut response =
                        heapless::String::<{ flash_task::USB_RESPONSE_CAPACITY }>::new();
                    match command {
                        flash_task::StorageCommand::Help => {
                            queue_storage_response(
                                flash_response_producer,
                                "OK flash info | flash test CONFIRM | logs list\r\n",
                            );
                            queue_storage_response(
                                flash_response_producer,
                                "OK logs erase CONFIRM | config get/set KEY | config save\r\n",
                            );
                        }
                        flash_task::StorageCommand::FlashInfo => {
                            let _ = write!(
                                response,
                                "OK jedec={:02x}:{:02x}:{:02x} bytes={} ready=1\r\n",
                                FLASH_JEDEC_MANUFACTURER.load(Ordering::Relaxed),
                                FLASH_JEDEC_MEMORY_TYPE.load(Ordering::Relaxed),
                                FLASH_JEDEC_CAPACITY_CODE.load(Ordering::Relaxed),
                                layout.capacity_bytes
                            );
                            queue_storage_response(flash_response_producer, response.as_str());
                        }
                        flash_task::StorageCommand::FlashTestConfirmed => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR flash test disabled while armed\r\n",
                                );
                            } else if erase_sector_index.is_some()
                                || *flash_test_phase != 0
                                || *config_save_phase != 0
                            {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR flash maintenance already active\r\n",
                                );
                            } else {
                                *flash_test_phase = 1;
                                queue_storage_response(
                                    flash_response_producer,
                                    "OK flash scratch test started\r\n",
                                );
                            }
                        }
                        flash_task::StorageCommand::LogsList => {
                            if let Some(response) = flash_task::format_log_info_response(
                                *next_page_index,
                                *next_flight_id,
                                layout.log_page_count,
                                *log_region_writable,
                            ) {
                                queue_storage_response(flash_response_producer, response.as_str());
                            } else {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR log summary formatting failed\r\n",
                                );
                            }
                        }
                        flash_task::StorageCommand::LogsReadPage(page_index) => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR log reads disabled while armed\r\n",
                                );
                            } else if page_index >= *next_page_index {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR log page is not present\r\n",
                                );
                            } else if let Some(address) = layout.log_page_address(page_index) {
                                let mut page = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                                if flash_device.read(address, &mut page).is_err()
                                    || !queue_page_hex(flash_response_producer, page_index, &page)
                                {
                                    queue_storage_response(
                                        flash_response_producer,
                                        "ERR log page read/response failed\r\n",
                                    );
                                }
                            }
                        }
                        flash_task::StorageCommand::LogsEraseConfirmed => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR log erase disabled while armed\r\n",
                                );
                            } else if erase_sector_index.is_some()
                                || *flash_test_phase != 0
                                || *config_save_phase != 0
                            {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR flash maintenance already active\r\n",
                                );
                            } else {
                                *erase_sector_index = Some(0);
                                queue_storage_response(
                                    flash_response_producer,
                                    "OK log erase started\r\n",
                                );
                            }
                        }
                        flash_task::StorageCommand::ConfigGet(key) => {
                            let _ = write!(
                                response,
                                "OK {}={:.4}\r\n",
                                key.name(),
                                stored_config.get(key)
                            );
                            queue_storage_response(flash_response_producer, response.as_str());
                        }
                        flash_task::StorageCommand::ConfigSet(key, value) => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR config changes disabled while armed\r\n",
                                );
                            } else if stored_config.set(key, value) {
                                queue_storage_response(
                                    flash_response_producer,
                                    "OK staged; use config save\r\n",
                                );
                            } else {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR value outside allowed range\r\n",
                                );
                            }
                        }
                        flash_task::StorageCommand::ConfigSave => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR config save disabled while armed\r\n",
                                );
                            } else if erase_sector_index.is_some()
                                || *flash_test_phase != 0
                                || *config_save_phase != 0
                            {
                                queue_storage_response(
                                    flash_response_producer,
                                    "ERR flash maintenance already active\r\n",
                                );
                            } else {
                                let next_sequence = config_sequence.wrapping_add(1);
                                match ferrowasp_core::blackbox::encode_config_page(
                                    next_sequence,
                                    &stored_config.encode(),
                                ) {
                                    Ok(page) => {
                                        *config_save_candidate = *stored_config;
                                        *config_save_page = page;
                                        *config_save_slot = 1 - *config_active_slot;
                                        *config_save_phase = 1;
                                        queue_storage_response(
                                            flash_response_producer,
                                            "OK config save started\r\n",
                                        );
                                    }
                                    Err(_) => {
                                        queue_storage_response(
                                            flash_response_producer,
                                            "ERR config encoding failed\r\n",
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                #[cfg(feature = "mspv2_configurator")]
                if let Some(request) = flash_rpc_command_consumer.dequeue() {
                    let request_id = request.request_id;
                    let maintenance_busy = erase_sector_index.is_some()
                        || *flash_test_phase != 0
                        || *config_save_phase != 0;
                    let active_blackbox_id = assembler
                        .recording()
                        .then(|| next_flight_id.wrapping_sub(1).max(1));
                    let response = match request.operation {
                        mspv2::rpc::Request::Hello => Some(mspv2::rpc::ok(
                            request_id,
                            mspv2::rpc::Response::Hello(rpc_device_info()),
                        )),
                        mspv2::rpc::Request::GetConfig => {
                            let crc = stored_config.crc32();
                            Some(mspv2::rpc::ok(
                                request_id,
                                mspv2::rpc::Response::Config(mspv2::rpc::ConfigSnapshot {
                                    schema_version: mspv2::rpc::CONFIG_SCHEMA_VERSION,
                                    active_crc32: crc,
                                    persisted_crc32: crc,
                                    config: stored_config.to_rpc(),
                                }),
                            ))
                        }
                        mspv2::rpc::Request::StageConfig(config) => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::Armed,
                                ))
                            } else if maintenance_busy {
                                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                            } else {
                                match stored_config.apply_rpc(config) {
                                    Ok(candidate) => {
                                        let crc = candidate.crc32();
                                        *staged_rpc_config = Some(candidate);
                                        Some(mspv2::rpc::ok(
                                            request_id,
                                            mspv2::rpc::Response::ConfigStaged {
                                                staged_crc32: crc,
                                                reboot_required: false,
                                            },
                                        ))
                                    }
                                    Err(field) => Some(mspv2::rpc::RpcResponse {
                                        protocol_version: mspv2::rpc::FWSP_RPC_VERSION,
                                        request_id,
                                        result: mspv2::rpc::RpcResult::Err(
                                            mspv2::rpc::ErrorDetail {
                                                code: mspv2::rpc::DeviceError::InvalidConfiguration,
                                                field: Some(field),
                                                argument: None,
                                            },
                                        ),
                                    }),
                                }
                            }
                        }
                        mspv2::rpc::Request::CommitConfig {
                            expected_staged_crc32,
                        } => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::Armed,
                                ))
                            } else if maintenance_busy {
                                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                            } else if let Some(candidate) = *staged_rpc_config {
                                if candidate.crc32() != expected_staged_crc32 {
                                    Some(mspv2::rpc::error(
                                        request_id,
                                        mspv2::rpc::DeviceError::ConfigurationConflict,
                                    ))
                                } else {
                                    let next_sequence = config_sequence.wrapping_add(1);
                                    match ferrowasp_core::blackbox::encode_config_page(
                                        next_sequence,
                                        &candidate.encode(),
                                    ) {
                                        Ok(page) => {
                                            *config_save_candidate = candidate;
                                            *config_save_page = page;
                                            *config_save_slot = 1 - *config_active_slot;
                                            *config_save_phase = 1;
                                            *config_rpc_completion =
                                                Some(ConfigRpcCompletion::Commit(request_id));
                                            *staged_rpc_config = None;
                                            None
                                        }
                                        Err(_) => Some(mspv2::rpc::error(
                                            request_id,
                                            mspv2::rpc::DeviceError::Internal,
                                        )),
                                    }
                                }
                            } else {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::ConfigurationConflict,
                                ))
                            }
                        }
                        mspv2::rpc::Request::ResetConfigToDefaults => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::Armed,
                                ))
                            } else if maintenance_busy {
                                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                            } else {
                                let candidate = flash_task::StoredConfig::foxeer_f405_v2_default();
                                let next_sequence = config_sequence.wrapping_add(1);
                                match ferrowasp_core::blackbox::encode_config_page(
                                    next_sequence,
                                    &candidate.encode(),
                                ) {
                                    Ok(page) => {
                                        *config_save_candidate = candidate;
                                        *config_save_page = page;
                                        *config_save_slot = 1 - *config_active_slot;
                                        *config_save_phase = 1;
                                        *config_rpc_completion =
                                            Some(ConfigRpcCompletion::Defaults(request_id));
                                        *staged_rpc_config = None;
                                        None
                                    }
                                    Err(_) => Some(mspv2::rpc::error(
                                        request_id,
                                        mspv2::rpc::DeviceError::Internal,
                                    )),
                                }
                            }
                        }
                        mspv2::rpc::Request::ListBlackboxes => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::Armed,
                                ))
                            } else {
                                match list_blackboxes(
                                    flash_device,
                                    layout,
                                    *next_page_index,
                                    active_blackbox_id,
                                ) {
                                    Ok(list) => Some(mspv2::rpc::ok(
                                        request_id,
                                        mspv2::rpc::Response::BlackboxList(list),
                                    )),
                                    Err(_) => Some(mspv2::rpc::error(
                                        request_id,
                                        mspv2::rpc::DeviceError::ReadFailure,
                                    )),
                                }
                            }
                        }
                        mspv2::rpc::Request::GetBlackboxInfo { id } => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::Armed,
                                ))
                            } else {
                                match blackbox_info(
                                    flash_device,
                                    layout,
                                    *next_page_index,
                                    id,
                                    active_blackbox_id,
                                ) {
                                    Ok(Some(info)) => Some(mspv2::rpc::ok(
                                        request_id,
                                        mspv2::rpc::Response::BlackboxInfo(info),
                                    )),
                                    Ok(None) => Some(mspv2::rpc::error(
                                        request_id,
                                        mspv2::rpc::DeviceError::BlackboxNotFound,
                                    )),
                                    Err(_) => Some(mspv2::rpc::error(
                                        request_id,
                                        mspv2::rpc::DeviceError::ReadFailure,
                                    )),
                                }
                            }
                        }
                        mspv2::rpc::Request::ReadBlackboxChunk {
                            id,
                            offset,
                            requested_length,
                        } => {
                            if SAFETY_ARMED.load(Ordering::Acquire) {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::Armed,
                                ))
                            } else if active_blackbox_id == Some(id.0) {
                                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                            } else {
                                match read_blackbox_chunk(
                                    flash_device,
                                    layout,
                                    *next_page_index,
                                    id,
                                    offset,
                                    requested_length,
                                ) {
                                    Ok(chunk) => Some(mspv2::rpc::ok(
                                        request_id,
                                        mspv2::rpc::Response::BlackboxChunk(chunk),
                                    )),
                                    Err(error) => Some(mspv2::rpc::error(request_id, error)),
                                }
                            }
                        }
                        mspv2::rpc::Request::EraseBlackbox { .. }
                        | mspv2::rpc::Request::Reboot { .. } => Some(mspv2::rpc::error(
                            request_id,
                            mspv2::rpc::DeviceError::UnsupportedOperation,
                        )),
                    };
                    if let Some(response) = response {
                        let _ = queue_rpc_response(flash_rpc_response_producer, response);
                    }
                }
                if *log_region_writable && pending_page.is_none() {
                    *pending_page = assembler.take_ready_page();
                }
                if let Some(page) = pending_page.as_ref() {
                    let Some(address) = layout.log_page_address(*next_page_index) else {
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SD storage blackbox region full; recording stopped");
                        continue;
                    };
                    if flash_device.page_program(address, page).is_err() {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SD storage page program failed; recording stopped");
                        continue;
                    }
                    *pending_page = None;
                    *next_page_index = next_page_index.saturating_add(1);
                    FLASH_PAGES_WRITTEN.fetch_add(1, Ordering::Relaxed);
                    // Fall through to the record drain rather than yielding
                    // here. Yielding meant a pass either wrote a page or
                    // consumed records, never both, so the store alternated
                    // between the two and halved its own throughput. The loop
                    // still yields once at the end of every pass.
                }
                if !*log_region_writable {
                    if flash_record_consumer.dequeue().is_some() {
                        FLASH_RECORDS_DROPPED.fetch_add(1, Ordering::Relaxed);
                    }
                } else if flash_record_consumer.peek().is_some() {
                    // Drain a bounded batch rather than one record per pass.
                    //
                    // This loop ends in a one millisecond yield, so consuming a
                    // single record per pass capped the store near one record
                    // per millisecond however fast the flash is - measured at
                    // 84 pages/s, where the part itself programs a page in well
                    // under a millisecond. At 400 Hz that left about five
                    // percent of margin nobody had measured, and any higher
                    // rate silently dropped most of the log.
                    //
                    // The bound keeps this task from starving the rest of the
                    // system on a full queue. A page still goes out once per
                    // pass, which at roughly a millisecond a pass is ample for
                    // the few hundred pages a second these rates ask for.
                    let mut drained = 0;
                    while drained < FLASH_RECORD_DRAIN_PER_PASS {
                        let Some(record) = flash_record_consumer.dequeue() else {
                            break;
                        };
                        drained += 1;
                        // Bench-only: record while disarmed so the flash write
                        // path can be exercised without flying. The record's own
                        // armed flag stays honest, so a capture made this way is
                        // still identifiable as ground data.
                        #[cfg(feature = "bench_blackbox")]
                        let armed = true;
                        #[cfg(not(feature = "bench_blackbox"))]
                        let armed = record.flags & 1 != 0;
                        if armed && !assembler.recording() {
                            assembler.start(*next_flight_id, *boot_session_start_pending);
                            *boot_session_start_pending = false;
                            *next_flight_id = next_flight_id.wrapping_add(1).max(1);
                        }
                        if armed {
                            if assembler.push(record).is_err() {
                                FLASH_RECORDS_DROPPED.fetch_add(1, Ordering::Relaxed);
                            }
                        } else if assembler.recording() && assembler.stop().is_err() {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                } else if !SAFETY_ARMED.load(Ordering::Acquire)
                    && assembler.recording()
                    && assembler.stop().is_err()
                {
                    FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                }
            }

            Mono::delay(1u64.millis()).await;
        }
    }

    // ---- MAIN CONTROL LOOP ----
    #[task(
        from = flight_tasks::control_loop,
        binds = TIM4,
        priority = 14,
        local = [
            control_loop_cnt,
            samples_per_control_loop,
            flight_controller,
            control_loop_scheduler,
            imu_rate_filter,
            imu_angle_integrator,
            gyro_axis_map,
            gyro_bias_calibrator,
            imu_last_sequence,
            imu_stale_ticks,
            applied_tuning_seq,
            rc_rates_reader,
            control_throttle_reader,
            control_safety_arm_reader,
            control_rc_link_reader,
            control_arm_permit_reader,
            motor_cmd_writer,
            motor_cmd_seq,
            flash_record_producer
        ],
        shared = [imu_data, imu_angles, imu_rates, tuning_profile, tuning_request_seq],
        spawn = [actuator_output, safety_master],
        config = [
            imu_control_axis_profile: ferrowasp_core::frames::ImuControlAxisProfile =
                IMU_CONTROL_AXIS_PROFILE,
            imu_gyro_raw_to_dps: f32 = IMU_GYRO_RAW_TO_DPS,
            logical_to_physical_motor_output: [usize; 4] =
                board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
            control_loop_rate_hz: u32 = board::profiles::CONTROL_LOOP_RATE_HZ,
        ]
    )]
    fn control_loop(cx: control_loop::Context);

    #[task(
        from = flight_tasks::actuator_idle_notify,
        priority = 13,
        spawn = [safety_master]
    )]
    async fn actuator_idle_notify(cx: actuator_idle_notify::Context);

    #[task(
        from = flight_tasks::dshot_service,
        priority = 13,
        shared = [dshot_motors],
        local = [esc_request_consumer, esc_ack_producer],
        spawn = [safety_master],
        config = [
            dshot_motor_for_output: fn(esc::EscOutput) -> board::init::DshotMotor =
                dshot_motor_for_output,
        ]
    )]
    async fn dshot_service(cx: dshot_service::Context);

    #[task(
        from = flight_tasks::dshot_dma_complete,
        binds = DMA2_STR0,
        priority = 16,
        shared = [dshot_motors],
        config = [
            motor: board::init::DshotMotor = board::init::DshotMotor::Motor1,
            stream: u8 = 0,
        ]
    )]
    fn dshot_motor1_dma_complete(cx: dshot_motor1_dma_complete::Context);

    #[task(
        from = flight_tasks::dshot_dma_complete,
        binds = DMA2_STR1,
        priority = 16,
        shared = [dshot_motors],
        config = [
            motor: board::init::DshotMotor = board::init::DshotMotor::Motor2,
            stream: u8 = 1,
        ]
    )]
    fn dshot_motor2_dma_complete(cx: dshot_motor2_dma_complete::Context);

    #[task(
        from = flight_tasks::dshot_dma_complete,
        binds = DMA2_STR2,
        priority = 16,
        shared = [dshot_motors],
        config = [
            motor: board::init::DshotMotor = board::init::DshotMotor::Motor3,
            stream: u8 = 2,
        ]
    )]
    fn dshot_motor3_dma_complete(cx: dshot_motor3_dma_complete::Context);

    #[task(
        from = flight_tasks::dshot_dma_complete,
        binds = DMA2_STR3,
        priority = 16,
        shared = [dshot_motors],
        config = [
            motor: board::init::DshotMotor = board::init::DshotMotor::Motor4,
            stream: u8 = 3,
        ]
    )]
    fn dshot_motor4_dma_complete(cx: dshot_motor4_dma_complete::Context);

    // ########### Logical UART1: UART8 / BLHeli legacy ESC telemetry ########
    #[task(
        from = flight_tasks::usart1_rx_dma_transfer,
        binds = DMA1_STR2,
        priority = 5,
        shared = [uart1_rx]
    )]
    fn usart1_rx_dma_transfer(cx: usart1_rx_dma_transfer::Context);

    #[task(
        from = flight_tasks::usart1_rx_peripheral,
        binds = UART8,
        priority = 5,
        shared = [uart1_rx]
    )]
    fn usart1_rx_peripheral(cx: usart1_rx_peripheral::Context);

    #[task(
        from = flight_tasks::esc_manager_task,
        priority = 4,
        local = [
            esc_telemetry_uart,
            esc_manager_state,
            esc_request_producer,
            esc_ack_consumer,
            esc_telemetry_update_producer
        ],
        config = [
            esc_output_to_logical_motor: [u8; 4] = [
                logical_motor_for_physical_index(0),
                logical_motor_for_physical_index(1),
                logical_motor_for_physical_index(2),
                logical_motor_for_physical_index(3),
            ],
        ]
    )]
    async fn esc_manager_task(cx: esc_manager_task::Context);

    #[task(
        from = flight_tasks::actuator_output,
        priority = 15,
        shared = [dshot_motors],
        local = [
            actuator_safety_arm_reader,
            actuator_arm_permit_reader,
            actuator_rc_arm_high_reader,
            actuator_rc_throttle_reader,
            actuator_rc_link_reader,
            actuator_arm_done_writer,
            motor_cmd_reader,
            esc_telemetry_update_consumer
        ],
        spawn = [safety_master, actuator_idle_notify],
        config = [
            validate_live_arming_guard: flight_tasks::ArmingGuard = validate_live_arming_guard,
            dshot_idle_command: f32 = BOARD_DSHOT_IDLE_COMMAND,
            actuator_output_enabled: bool = ACTUATOR_OUTPUT_ENABLED,
            actuator_inhibit_reason: &'static str = ACTUATOR_INHIBIT_REASON,
            esc_output_to_logical_motor: [u8; 4] = [
                logical_motor_for_physical_index(0),
                logical_motor_for_physical_index(1),
                logical_motor_for_physical_index(2),
                logical_motor_for_physical_index(3),
            ],
        ]
    )]
    async fn actuator_output(cx: actuator_output::Context, cmd: safety::ActuatorCmd);

    // ########### SPI 1 ###################################
    #[task(
        from = flight_tasks::imu_data_ready,
        binds = EXTI2,
        priority = 14,
        local = [imu_data_ready],
        shared = [io_timebase],
        spawn = [spi1_poll]
    )]
    fn imu_data_ready(cx: imu_data_ready::Context);

    #[task(
        from = flight_tasks::spi1_poll,
        priority = 12,
        local = [spi1_device],
        config = [spi1_imu_burst_register: fn(u8) -> Option<u8> = spi1_imu_burst_register]
    )]
    async fn spi1_poll(cx: spi1_poll::Context, observed_at_us: u64);

    #[task(
        from = flight_tasks::spi1_owner_service,
        priority = 13,
        shared = [spi1_owner, io_timebase]
    )]
    async fn spi1_owner_service(cx: spi1_owner_service::Context);

    #[task(
        from = flight_tasks::spi1_rx_dma,
        binds = DMA1_STR4,
        priority = 13,
        shared = [spi1_owner],
        spawn = [spi1_parser]
    )]
    fn spi1_rx_dma(cx: spi1_rx_dma::Context);

    #[task(
        from = flight_tasks::io_watchdog,
        binds = TIM6_DAC,
        priority = 9,
        local = [io_watchdog],
        shared = [io_timebase],
        spawn = [spi1_timeout]
    )]
    fn io_watchdog(cx: io_watchdog::Context);

    #[task(from = flight_tasks::spi1_timeout, priority = 13, shared = [spi1_owner])]
    async fn spi1_timeout(cx: spi1_timeout::Context, observed_at_us: u64);

    #[task(
        from = flight_tasks::spi1_parser,
        priority = 11,
        local = [spi1_parser],
        shared = [imu_data]
    )]
    async fn spi1_parser(cx: spi1_parser::Context);

    // ########### Logical UART2: USART6 / SBUS ###################################
    #[task(
        from = flight_tasks::usart2_rx_dma_transfer,
        binds = DMA1_STR0,
        priority = 11,
        shared = [uart2_rx, uart2_bridge],
        spawn = [safety_master]
    )]
    fn usart2_rx_dma_transfer(cx: usart2_rx_dma_transfer::Context);

    #[task(
        from = flight_tasks::usart2_rx_peripheral,
        binds = USART6,
        priority = 11,
        shared = [uart2_rx, uart2_bridge],
        spawn = [safety_master]
    )]
    fn usart2_rx_peripheral(cx: usart2_rx_peripheral::Context);

    // ########### Logical UART4: USART3 / DJI MSP OSD ###########################
    #[task(
        from = flight_tasks::uart4_rx_dma_transfer,
        binds = DMA1_STR1,
        priority = 6,
        shared = [uart4_rx],
        spawn = [osd_refresh]
    )]
    fn uart4_rx_dma_transfer(cx: uart4_rx_dma_transfer::Context);

    #[task(
        from = flight_tasks::uart4_rx_peripheral,
        binds = USART3,
        priority = 6,
        shared = [uart4_rx],
        spawn = [osd_refresh]
    )]
    fn uart4_rx_peripheral(cx: uart4_rx_peripheral::Context);

    #[task(
        from = flight_tasks::osd_refresh,
        priority = 3,
        local = [
            osd_uart,
            osd_rx_producer,
            osd_rx_reader,
            osd_rx_discontinuities,
            osd_tx_writer,
            osd_tx_healthy,
            osd_task,
            osd_tx_buffer,
            osd_refresh_tick,
            osd_safety_arm_reader,
            osd_rc_throttle_reader,
            osd_rc_rates_reader
        ],
        shared = [
            battery_voltage_v10,
            battery_cell_count,
            battery_cell_voltage_v100,
            battery_current_ca,
            imu_angles,
            imu_rates,
            imu_data,
            tuning_profile,
            tuning_request_seq
        ]
    )]
    async fn osd_refresh(cx: osd_refresh::Context);

    #[task(
        from = flight_tasks::uart4_tx_worker,
        priority = 4,
        local = [uart4_tx_owner = uart4_tx_owner],
        shared = [uart4_tx_dma = uart4_tx_dma],
    )]
    async fn uart4_tx_worker(cx: uart4_tx_worker::Context);

    #[task(
        from = flight_tasks::uart4_tx_dma_transfer,
        binds = DMA1_STR3,
        priority = 6,
        local = [uart4_tx_completion],
        shared = [uart4_tx_dma]
    )]
    fn uart4_tx_dma_transfer(cx: uart4_tx_dma_transfer::Context);

    #[task(
        from = flight_tasks::rc_input,
        priority = 10,
        local = [
            rc_rx_reader,
            rc_rx_discontinuities,
            sbus,
            arm_qualifier,
            rc_rates_writer,
            rc_throttle_writer,
            rc_arm_high_writer,
            rc_link_frame_writer
        ],
        shared = [tuning_profile],
        spawn = [safety_master]
    )]
    async fn rc_input(cx: rc_input::Context);

    // ---- ADC1 ----
    #[task(
        from = flight_tasks::dma_adc1,
        binds = ADC1_2,
        priority = 1,
        shared = [
            adc1_transfer,
            battery_voltage_v10,
            battery_cell_count,
            battery_cell_voltage_v100,
            battery_current_ca
        ],
        local = [
            adc1_buffer,
            battery_cell_detector: osd::BatteryCellDetector =
                osd::BatteryCellDetector::new(
                    BATTERY_MAX_CELL_MV,
                    BATTERY_DETECT_CELL_MV,
                    BATTERY_MAX_CELLS,
                )
        ],
        config = [
            adc_vbat_divider_ratio: f32 = ADC_VBAT_DIVIDER_RATIO,
            adc_current_betaflight_scale: u32 = ADC_CURRENT_BETAFLIGHT_SCALE,
            adc_current_offset_ma: i32 = ADC_CURRENT_OFFSET_MA,
        ]
    )]
    fn dma_adc1(cx: dma_adc1::Context);

    #[task(from = flight_tasks::adc1_polling, priority = 1, shared = [adc1_transfer])]
    async fn adc1_polling(cx: adc1_polling::Context);
}
