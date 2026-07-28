// ####  SET-UP  ####
// Compiler directives
#![deny(unsafe_code)]
#![no_main]
#![no_std]

use ferrowasp_app_foxeer_f405_v2::internal::*;

#[rtic::app(device = pac, peripherals = true, dispatchers = [CAN1_TX, CAN2_TX, CAN1_RX0, CAN1_RX1, CAN1_SCE, CAN2_RX0, CAN2_RX1, OTG_HS_EP1_OUT, OTG_HS_EP1_IN])]
mod app {
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
        uart2_rx: stm32_uart::Uart2RxIrq,
        uart2_bridge: Uart2OwnedRxBridge,
        #[lock_free]
        uart4_rx: stm32_uart::Uart4RxIrq,
        uart4_tx_dma: stm32_uart::Uart4TxDmaSide,

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

        // SPI2 storage. Only the priority-1 flash manager owns this device.
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

        // USART2
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
        let mut rcc = dp.RCC.constrain();

        // Assign peripherals
        let dma1 = StreamsTuple::new(dp.DMA1, &mut rcc);
        let dma2 = StreamsTuple::new(dp.DMA2, &mut rcc);
        let gpioa = dp.GPIOA.split(&mut rcc);
        let gpiob = dp.GPIOB.split(&mut rcc);
        let gpioc = dp.GPIOC.split(&mut rcc);

        // Keep the existing 800 Hz scheduler and 400 Hz PID/motor cadence.
        // IMU sampling itself is independently triggered by PC4/EXTI4.
        let scheduler_rate = dt::IMU_POLL_RATE_HZ.Hz();
        let control_loop_rate: Rate<u32, 1, 1> = dt::CONTROL_LOOP_RATE_HZ.Hz();
        let samples_per_control_loop = scheduler_rate.to_Hz() / control_loop_rate.to_Hz();

        let adc1_battery = board::init::init_adc1_battery(
            board::init::Adc1BatteryResourcesFoxeer {
                adc: dp.ADC1,
                voltage_pin: gpioc.pc0,
                current_pin: gpioc.pc1,
                dma: dma2.4,
            },
            &mut rcc,
            stm32_storage::AdcStorageResources {
                buffers: cx.local.adc1_buffers,
            },
        );

        // Configure clocks through the shared STM32F4 mechanism; HSE remains a board fact.
        let mut clocks = stm32_clocks::freeze_hse(
            rcc,
            board::HSE_FREQUENCY_HZ,
            stm32_clocks::SYSTEM_CLOCK_HZ,
            true,
        );
        info!(
            "PLL48 valid: {}, PLL48: {} Hz",
            clocks.clocks.is_pll48clk_valid(),
            clocks.clocks.pll48clk().map(|clk| clk.raw()).unwrap_or(0)
        );
        Mono::start(cx.core.SYST, stm32_clocks::SYSTEM_CLOCK_HZ);
        let mut delay = dp.TIM5.delay::<DELAY_TIMER_HZ>(&mut clocks);
        //let mut syscfg = dp.SYSCFG.constrain(&mut clocks);

        let control_loop_scheduler =
            stm32_scheduler::init_control_scheduler(dp.TIM4, &mut clocks, scheduler_rate).unwrap();
        let io_timebase = stm32_timebase::MicrosecondTimebase::new(dp.TIM2, &mut clocks).unwrap();
        let io_watchdog = stm32_watchdog::init_io_watchdog(dp.TIM6, &mut clocks).unwrap();

        let (usb_dev, usb_serial) = stm32_usb::init_usb_cdc_serial(
            (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
            (gpioa.pa11, gpioa.pa12),
            &clocks.clocks,
            board::USB_CDC_IDENTITY,
        )
        .unwrap();

        // Set-up Routing
        let mut rc_input_uart = None;
        let mut osd_uart = None;
        let mut tele_uart = None;
        let mut gps_uart = None;

        // ------------  USART1 / BLHeli legacy ESC telemetry  ------------
        // Optional observational bring-up path: ESC TLM -> PA10 USART1_RX.
        let (uart1_rx, esc_telemetry_uart) = {
            board::aliases::assert_usart1_esc_telemetry_route_compile();
            let uart1 = stm32_uart::init_usart1_esc_telemetry(
                stm32_uart::Usart1EscTelemetryResources {
                    rx_pin: gpioa.pa10,
                    usart: dp.USART1,
                    rx_dma: dma2.5,
                },
                &mut clocks,
                stm32_storage::UartRxStorageResources {
                    buffers: cx.local.uart1_rx_buffers,
                    free_queue: cx.local.uart1_free_queue,
                    filled_queue: cx.local.uart1_filled_queue,
                },
            );
            (uart1.irq, uart1.parser)
        };

        // ------------  USART2 / SBUS RC  ------------
        let uart2 = stm32_uart::init_usart2_sbus(
            stm32_uart::Usart2SbusResources {
                tx_pin: gpioa.pa2,
                rx_pin: gpioa.pa3,
                usart: dp.USART2,
                rx_dma: dma1.5,
            },
            &mut clocks,
            stm32_storage::UartRxStorageResources {
                buffers: cx.local.uart2_rx_buffers,
                free_queue: cx.local.uart2_free_queue,
                filled_queue: cx.local.uart2_filled_queue,
            },
        );
        route_uart_to_task(
            UART2_CONSUMER,
            uart2.parser,
            &mut rc_input_uart,
            &mut osd_uart,
            &mut tele_uart,
            &mut gps_uart,
        );
        // ------------  UART4 / DJI O4 MSP OSD  ------------
        // Board connection: PA0 UART4_TX -> DJI O4 RX, PA1 UART4_RX <- DJI O4 TX.
        let uart4 = stm32_uart::init_uart4_msp_osd(
            stm32_uart::Uart4MspResources {
                tx_pin: gpioa.pa0,
                rx_pin: gpioa.pa1,
                uart: dp.UART4,
                rx_dma: dma1.2,
                tx_dma: dma1.4,
            },
            &mut clocks,
            stm32_storage::UartRxStorageResources {
                buffers: cx.local.uart4_rx_buffers,
                free_queue: cx.local.uart4_free_queue,
                filled_queue: cx.local.uart4_filled_queue,
            },
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
            .expect("board support must route USART2 to the RC input task");
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

        let dshot_motors = {
            board::aliases::assert_four_motor_dshot_routes_compile();
            let tim1 = Timer::new(dp.TIM1, &mut clocks);
            let tim8 = Timer::new(dp.TIM8, &mut clocks);
            board::init::init_dshot_motor_bank(
                board::init::DshotMotorBankResources {
                    tim1,
                    tim8,
                    motor1_pin: gpioa.pa8,
                    motor2_pin: gpioc.pc9,
                    motor3_pin: gpioc.pc8,
                    motor4_pin: gpiob.pb15,
                    motor1_dma: dma2.1,
                    motor2_dma: dma2.7,
                    motor3_dma: dma2.2,
                    motor4_dma: dma2.6,
                },
                &clocks.clocks,
                cx.local.dshot_dma_storage,
            )
            .expect("Foxeer DShot600 timing must be valid")
        };

        // Minimum Throttle
        // SBUS 1175

        let spi1_imu = board::init::init_spi1_imu(
            board::init::Spi1ImuResources {
                cs_pin: gpioa.pa4,
                sck_pin: gpioa.pa5,
                miso_pin: gpioa.pa6,
                mosi_pin: gpioa.pa7,
                spi: dp.SPI1,
                rx_dma: dma2.0,
                tx_dma: dma2.3,
            },
            &mut clocks,
            &mut delay,
            stm32_storage::SpiDmaStorageResources {
                buffers: cx.local.spi1_dma_buffers,
                free_queue: cx.local.spi1_free_queue,
                filled_queue: cx.local.spi1_filled_queue,
            },
        );
        let mut syscfg = dp.SYSCFG.constrain(&mut clocks);
        let mut exti = dp.EXTI;
        let imu_data_ready = board::init::init_imu_data_ready(gpioc.pc4, &mut syscfg, &mut exti);
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
            let mut flash = board::init::init_spi2_flash(
                board::init::Spi2FlashResources {
                    cs_pin: gpiob.pb12,
                    sck_pin: gpiob.pb13,
                    miso_pin: gpioc.pc2,
                    mosi_pin: gpioc.pc3,
                    spi: dp.SPI2,
                },
                &mut clocks,
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
                        "SPI2 flash JEDEC {:02x}:{:02x}:{:02x}, capacity {} bytes, supported {}",
                        id.manufacturer, id.memory_type, id.capacity_code, capacity, supported
                    );
                }
                Err(_) => warn!("SPI2 flash JEDEC probe failed; storage remains disabled"),
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
        info!("FerroWasp RTT hello from Foxeer");
        match spi1_imu.bringup {
            board::init::Spi1ImuBringupStatus::Ready { kind, who_am_i } => match kind {
                Spi1ImuKind::Mpu6500 => {
                    info!("Foxeer MPU6500 ready; WHO_AM_I {}", who_am_i);
                }
                Spi1ImuKind::Icm42688P => {
                    info!("Foxeer ICM42688-P ready; WHO_AM_I {}", who_am_i);
                }
            },
            board::init::Spi1ImuBringupStatus::UnsupportedIdentity { who_am_i } => {
                warn!(
                    "Foxeer IMU unsupported; WHO_AM_I {}, sampling disabled",
                    who_am_i
                );
            }
            board::init::Spi1ImuBringupStatus::ProbeFailed => {
                warn!("Foxeer IMU identity probe failed; sampling disabled");
            }
            board::init::Spi1ImuBringupStatus::ConfigurationFailed { kind, who_am_i } => match kind
            {
                Spi1ImuKind::Mpu6500 => {
                    warn!(
                        "Foxeer MPU6500 configuration failed; WHO_AM_I {}, sampling disabled",
                        who_am_i
                    );
                }
                Spi1ImuKind::Icm42688P => {
                    warn!(
                        "Foxeer ICM42688-P configuration failed; WHO_AM_I {}, sampling disabled",
                        who_am_i
                    );
                }
            },
        }
        if SMOKE_ACTUATOR_INHIBIT_ENABLED {
            warn!("Flight arming inhibited: {}", ACTUATOR_INHIBIT_REASON);
        } else if BENCH_ACTUATOR_VALIDATION_ENABLED {
            warn!("PROPS OFF: capped Foxeer actuator-validation mode enabled");
            warn!("Normal mixer output is not active in this commissioning image");
        } else if !FLIGHT_ARMING_ENABLED {
            warn!("Flight arming inhibited: {}", ARMING_INHIBIT_REASON);
        } else {
            info!("Foxeer flight arming enabled with runtime IMU health checks");
            info!("Foxeer ADC uses Betaflight target voltage/current values");
        }
        info!("Foxeer BLHeli telemetry-qualified DShot arming active on PA10 USART1 RX");
        #[cfg(feature = "bench_dshot_idle_output1_not_running")]
        warn!(
            "FAULT INJECTION ACTIVE: Foxeer physical ESC output 1 (logical M1/rear-right) idle qualification eRPM forced to zero; arming must fail"
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
                imu_rate_filter: dt::ImuRateLowPassFilter::new(dt::IMU_GYRO_LPF_ALPHA),
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
    priority = 16,
    local = [
        safety_rc_arm_high_reader,
        safety_rc_throttle_reader,
        safety_rc_link_reader,
        safety_arm_writer,
        rc_link_invalidator,
        actuator_arm_permit_writer,
        actuator_arm_done_reader
    ]
    )]
    async fn safety_master(cx: safety_master::Context, event: safety::SafetyEvent) {
        let rc_arm_high = cx.local.safety_rc_arm_high_reader;
        let rc_throttle = cx.local.safety_rc_throttle_reader;
        let rc_link = cx.local.safety_rc_link_reader;
        let system_arm = cx.local.safety_arm_writer;
        let link_invalidator = cx.local.rc_link_invalidator;
        let arm_permit = cx.local.actuator_arm_permit_writer;
        let actuator_done = cx.local.actuator_arm_done_reader;
        let now_us = Mono::now().duration_since_epoch().to_micros();

        match event {
            safety::SafetyEvent::ArmRequested => {
                system_arm.disarm();

                if !ACTUATOR_OUTPUT_ENABLED {
                    arm_permit.revoke();
                    warn!("Arming inhibited: {}", ACTUATOR_INHIBIT_REASON);
                    return;
                }

                let guard = validate_live_arming_guard(
                    true,
                    rc_link.is_armable(now_us),
                    rc_arm_high.read(),
                    rc_throttle.read(),
                );
                if guard.is_ok() {
                    arm_permit.allow();
                    info!("Attempting DShot safety arming");

                    if actuator_output::spawn(safety::ActuatorCmd::EnterIdle).is_err() {
                        arm_permit.revoke();
                        warn!("Failed to spawn actuator EnterIdle");
                    }
                } else {
                    arm_permit.revoke();
                    warn_arming_abort(guard.unwrap_err());
                }
            }

            safety::SafetyEvent::ActuatorIdling => {
                if ACTUATOR_OUTPUT_ENABLED
                    && validate_live_arming_guard(
                        arm_permit.is_allowed(),
                        rc_link.is_armable(now_us),
                        rc_arm_high.read(),
                        rc_throttle.read(),
                    )
                    .is_ok()
                    && actuator_done.read()
                {
                    arm_permit.revoke();
                    system_arm.arm();
                    if BENCH_ACTUATOR_VALIDATION_ENABLED {
                        warn!("SYSTEM ARMED FOR CAPPED FOXEER ACTUATOR VALIDATION");
                    } else {
                        info!("SYSTEM ARMED");
                    }
                } else {
                    arm_permit.revoke();
                    system_arm.disarm();

                    let _ = actuator_output::spawn(safety::ActuatorCmd::Disarm);
                    warn!("ARM FAILED after actuator idle");
                    info!(
                        "RC throttle {} vs min {}",
                        rc_throttle.read(),
                        safety::ESC_IDLE_THROTTLE
                    );
                }
            }

            safety::SafetyEvent::ArmingAborted(reason) => {
                if !arm_permit.revoke() {
                    return;
                }
                system_arm.disarm();
                warn_arming_abort(reason);
            }

            safety::SafetyEvent::DisarmRequested => {
                let arming_active = arm_permit.revoke();
                system_arm.disarm();
                info!("SYSTEM DISARMED");

                if actuator_output::spawn(safety::ActuatorCmd::Disarm).is_err() && !arming_active {
                    warn!("Failed to spawn actuator Disarm");
                }
            }

            safety::SafetyEvent::RcLinkInvalid(reason) => {
                if !link_invalidator.invalidate(reason) {
                    return;
                }
                let arming_active = arm_permit.revoke();
                system_arm.disarm();

                if actuator_output::spawn(safety::ActuatorCmd::Disarm).is_err() && !arming_active {
                    warn!("Failed to spawn actuator Disarm after RC invalidation");
                }

                match reason {
                    safety::RcLinkInvalidation::Startup => warn!("RC link invalid at startup"),
                    safety::RcLinkInvalidation::TransportDiscontinuity => {
                        warn!("RC link invalidated by transport discontinuity")
                    }
                    safety::RcLinkInvalidation::DmaError => {
                        warn!("RC link invalidated by USART2 DMA error")
                    }
                    safety::RcLinkInvalidation::ParserError => {
                        warn!("RC link invalidated by SBUS parser error")
                    }
                    safety::RcLinkInvalidation::SbusFrameLost => {
                        warn!("RC link invalidated by SBUS frame-lost flag")
                    }
                    safety::RcLinkInvalidation::SbusFailsafe => {
                        warn!("RC link invalidated by SBUS failsafe flag")
                    }
                    safety::RcLinkInvalidation::Timeout => {
                        warn!("RC link invalidated by frame timeout")
                    }
                }
            }
        }
    }
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
            uptime_ms: Mono::now().duration_since_epoch().to_millis(),
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
        priority = 1,
        local = [
            usb_rc_link_reader,
            previous_drdy_count: u32 = 0,
            previous_drdy_rejected: u32 = 0
        ]
    )]
    async fn heartbeat(cx: heartbeat::Context) {
        info!("Running heartbeat!");

        loop {
            let now_us = Mono::now().duration_since_epoch().to_micros();
            let rc_link = cx.local.usb_rc_link_reader.status(now_us);
            USB_RC_VALID_SNAPSHOT.store(rc_link.valid, Ordering::Relaxed);
            USB_RC_ARMABLE_SNAPSHOT.store(rc_link.armable, Ordering::Relaxed);
            USB_DEBUG_DUE.store(true, Ordering::Release);
            cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);

            #[cfg(feature = "imu_transport_rtt")]
            info!(
                "IMU raw gyro [{}, {}, {}], seq {}",
                IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed),
                IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed),
                IMU_LATEST_YAW_RAW.load(Ordering::Relaxed),
                IMU_LATEST_SEQ.load(Ordering::Relaxed)
            );
            #[cfg(feature = "imu_orientation_rtt")]
            if let Some((sequence, accel_mg, gyro_dps10, temp_c10)) = imu_orientation_snapshot() {
                let body_gyro_dps10 = PHYSICAL_IMU_TO_DRONE_ROTATION.map_i32(gyro_dps10);
                let control_gyro_dps10 = CONTROL_IMU_TO_RATE_CONTROLLER_MAP.map_i32(gyro_dps10);
                let body_specific_force_mg = PHYSICAL_IMU_TO_DRONE_ROTATION.map_i32(accel_mg);
                let body_gravity_mg = [
                    -body_specific_force_mg[0],
                    -body_specific_force_mg[1],
                    -body_specific_force_mg[2],
                ];
                info!(
                    "IMU ORIENT sensor seq {} acc_mg [{}, {}, {}] gyro_dps10 [{}, {}, {}] temp_c10 {}",
                    sequence,
                    accel_mg[0],
                    accel_mg[1],
                    accel_mg[2],
                    gyro_dps10[0],
                    gyro_dps10[1],
                    gyro_dps10[2],
                    temp_c10
                );
                info!(
                    "IMU ORIENT body seq {} gravity_mg [{}, {}, {}] gyro_dps10 [{}, {}, {}]",
                    sequence,
                    body_gravity_mg[0],
                    body_gravity_mg[1],
                    body_gravity_mg[2],
                    body_gyro_dps10[0],
                    body_gyro_dps10[1],
                    body_gyro_dps10[2]
                );
                info!(
                    "IMU ORIENT control seq {} gyro_dps10 [{}, {}, {}]",
                    sequence, control_gyro_dps10[0], control_gyro_dps10[1], control_gyro_dps10[2]
                );
            }
            #[cfg(feature = "imu_transport_rtt")]
            {
                let drdy_count = IMU_DRDY_IRQ_COUNT.load(Ordering::Relaxed);
                let drdy_rejected = IMU_DRDY_REJECTED_COUNT.load(Ordering::Relaxed);
                info!(
                    "IMU DRDY IRQ {}, delta {}, rejected {}, delta {}, last {} us",
                    drdy_count,
                    drdy_count.wrapping_sub(*cx.local.previous_drdy_count),
                    drdy_rejected,
                    drdy_rejected.wrapping_sub(*cx.local.previous_drdy_rejected),
                    IMU_DRDY_LAST_US.load(Ordering::Relaxed)
                );
                *cx.local.previous_drdy_count = drdy_count;
                *cx.local.previous_drdy_rejected = drdy_rejected;
            }
            info!(
                "SPI2 flash ready {}, JEDEC {:02x}:{:02x}:{:02x}, capacity {} bytes",
                FLASH_READY.load(Ordering::Relaxed),
                FLASH_JEDEC_MANUFACTURER.load(Ordering::Relaxed),
                FLASH_JEDEC_MEMORY_TYPE.load(Ordering::Relaxed),
                FLASH_JEDEC_CAPACITY_CODE.load(Ordering::Relaxed),
                FLASH_CAPACITY_BYTES.load(Ordering::Relaxed)
            );
            info!(
                "SPI2 blackbox pages {}, dropped records {}, write faults {}, divisor {}",
                FLASH_PAGES_WRITTEN.load(Ordering::Relaxed),
                FLASH_RECORDS_DROPPED.load(Ordering::Relaxed),
                FLASH_WRITE_FAULTS.load(Ordering::Relaxed),
                FLASH_LOG_RATE_DIVISOR.load(Ordering::Relaxed)
            );
            Mono::delay(2000.millis()).await;
        }
    }

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
                    Mono::delay(100.millis()).await;
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
                        warn!("SPI2 flash log recovery failed; storage disabled");
                        continue;
                    };
                    let Ok((config, sequence, slot)) = load_flash_config(flash_device, layout)
                    else {
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SPI2 flash configuration read failed; storage disabled");
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
                        "SPI2 flash recovered at log page {}, next flight {}, writable {}, config seq {}",
                        page, flight, writable, sequence
                    );
                }

                let status = match flash_device.read_status() {
                    Ok(status) => status,
                    Err(_) => {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SPI2 flash status read failed; storage disabled");
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
                    Mono::delay(1.millis()).await;
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
                    Mono::delay(1.millis()).await;
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
                        warn!("SPI2 flash blackbox region full; recording stopped");
                        continue;
                    };
                    if flash_device.page_program(address, page).is_err() {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        FLASH_READY.store(false, Ordering::Release);
                        warn!("SPI2 flash page program failed; recording stopped");
                        continue;
                    }
                    *pending_page = None;
                    *next_page_index = next_page_index.saturating_add(1);
                    FLASH_PAGES_WRITTEN.fetch_add(1, Ordering::Relaxed);
                    Mono::delay(1.millis()).await;
                    continue;
                }
                if !*log_region_writable {
                    if flash_record_consumer.dequeue().is_some() {
                        FLASH_RECORDS_DROPPED.fetch_add(1, Ordering::Relaxed);
                    }
                } else if let Some(record) = flash_record_consumer.dequeue() {
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
                } else if !SAFETY_ARMED.load(Ordering::Acquire)
                    && assembler.recording()
                    && assembler.stop().is_err()
                {
                    FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                }
            }

            Mono::delay(1.millis()).await;
        }
    }

    // ---- MAIN CONTROL LOOP ----
    #[task(binds = TIM4, priority=14,
        local = [
            control_loop_cnt, samples_per_control_loop, flight_controller, control_loop_scheduler,
            imu_rate_filter, imu_angle_integrator,
            gyro_axis_map, gyro_bias_calibrator,
            imu_last_sequence, imu_stale_ticks, applied_tuning_seq,
            rc_rates_reader, control_throttle_reader, control_safety_arm_reader,
            control_rc_link_reader, control_arm_permit_reader,
            motor_cmd_writer, motor_cmd_seq,
            flash_record_producer,
            rc_link_was_valid: bool = false,
            ],
            shared = [imu_data, imu_angles, imu_rates, tuning_profile, tuning_request_seq])]
    fn control_loop(mut cx: control_loop::Context) {
        //info!("PING!");
        // Alias
        let fc = cx.local.flight_controller;
        stm32_scheduler::acknowledge_control_tick(cx.local.control_loop_scheduler);
        let cnt = cx.local.control_loop_cnt;
        let samples_per_control_loop = cx.local.samples_per_control_loop;
        let mut publish_motor_command = |motors, wake| {
            let outcome = actuator_task::publish_motor_command(
                cx.local.motor_cmd_writer,
                cx.local.motor_cmd_seq,
                motors,
                Mono::now().duration_since_epoch().to_millis(),
                wake,
                motor_command_timestamp,
                |command| actuator_output::spawn(command).is_ok(),
            );
            match outcome {
                actuator_task::PublishOutcome::Published => {}
                actuator_task::PublishOutcome::QueueFull => {
                    warn!("Motor command queue full; requesting disarm");
                    if safety_master::spawn(safety::SafetyEvent::DisarmRequested).is_err() {
                        warn!("Failed to report motor command queue overflow");
                    }
                }
                actuator_task::PublishOutcome::WakeRejected => {
                    warn!("Actuator command wake rejected; requesting disarm");
                    if safety_master::spawn(safety::SafetyEvent::DisarmRequested).is_err() {
                        warn!("Failed to report rejected actuator command wake");
                    }
                }
            }
        };
        let mut enqueue_flash_record = |sample| {
            let outcome = flash_task::enqueue_rate_record(
                cx.local.flash_record_producer,
                sample,
                Mono::now().duration_since_epoch().to_micros(),
                FLASH_LOG_RATE_DIVISOR.load(Ordering::Relaxed),
            );
            if outcome == flash_task::RecordEnqueueOutcome::Full {
                FLASH_RECORDS_DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        };

        // Incremet Counter
        *cnt += 1;
        CONTROL_ISR_SEQ.fetch_add(1, Ordering::Relaxed);

        if !IMU_TRANSPORT_READY.load(Ordering::Relaxed) {
            *cnt = 0;
            IMU_STALE.store(true, Ordering::Release);
            return;
        }

        // ---- CONTROL LOOP ----
        // Check if required samples per control loop is reached
        if cnt >= samples_per_control_loop {
            *cnt = 0; // Reset sampling counter

            let sensor_accel = cx.shared.imu_data.lock(|imu| imu.acc);
            let drone_gravity =
                IMU_CONTROL_AXIS_PROFILE.sensor_accel_to_drone_gravity(sensor_accel);
            let gyro_raw = [
                IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed) as i16,
                IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed) as i16,
                IMU_LATEST_YAW_RAW.load(Ordering::Relaxed) as i16,
            ];
            let imu_sequence = IMU_LATEST_SEQ.load(Ordering::Relaxed);

            let imu_fresh =
                imu::classify_sample_freshness(*cx.local.imu_last_sequence, imu_sequence)
                    == imu::SampleFreshness::Fresh;
            *cx.local.imu_last_sequence = imu_sequence;
            IMU_STALE.store(!imu_fresh, Ordering::Release);

            let control_armed = cx.local.control_safety_arm_reader.read();
            let gyro_bias_update = cx.local.gyro_bias_calibrator.update_if_fresh(
                control_armed,
                imu_fresh,
                cx.local.gyro_axis_map.map_raw(gyro_raw),
            );
            if gyro_bias_update.newly_calibrated {
                IMU_BIAS_CALIBRATED.store(true, Ordering::Release);
                info!(
                    "Gyro bias calibrated raw [{}, {}, {}]",
                    gyro_bias_update.bias_raw[0],
                    gyro_bias_update.bias_raw[1],
                    gyro_bias_update.bias_raw[2]
                );
            }
            let control_gyro_raw = gyro_bias_update.corrected_raw;
            let imu_roll_raw = control_gyro_raw[0] as f32 / IMU_GYRO_RAW_TO_DPS;
            let imu_pitch_raw = control_gyro_raw[1] as f32 / IMU_GYRO_RAW_TO_DPS;
            let imu_yaw_raw = control_gyro_raw[2] as f32 / IMU_GYRO_RAW_TO_DPS;
            CONTROL_ROLL_RAW.store(control_gyro_raw[0], Ordering::Relaxed);
            CONTROL_PITCH_RAW.store(control_gyro_raw[1], Ordering::Relaxed);
            CONTROL_YAW_RAW.store(control_gyro_raw[2], Ordering::Relaxed);
            let (imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered) = cx
                .local
                .imu_rate_filter
                .update(imu_roll_raw, imu_pitch_raw, imu_yaw_raw);
            // The rate controller retains FCU3's historical nose-down-positive
            // pitch convention. Translate its filtered rates back to physical
            // body rates for attitude estimation and external reporting.
            let body_rates = ferrowasp_core::frames::RATE_CONTROLLER_TO_BODY_MAP.map_f32([
                imu_roll_filtered,
                imu_pitch_filtered,
                imu_yaw_filtered,
            ]);
            CONTROL_ROLL_DPS10.store((imu_roll_filtered * 10.0) as i32, Ordering::Relaxed);
            CONTROL_PITCH_DPS10.store((imu_pitch_filtered * 10.0) as i32, Ordering::Relaxed);
            CONTROL_YAW_DPS10.store((imu_yaw_filtered * 10.0) as i32, Ordering::Relaxed);
            CONTROL_RATE_SEQ.fetch_add(1, Ordering::Relaxed);
            cx.shared.imu_rates.lock(|rates| {
                *rates = body_rates;
            });
            if !control_armed {
                // Keep Foxeer aligned with the FCU3 golden app: no
                // PID/filter/setpoint/mixer state crosses an arming boundary.
                fc.reset_control_state();

                let pending_seq = cx.shared.tuning_request_seq.lock(|seq| *seq);
                if pending_seq != *cx.local.applied_tuning_seq {
                    let profile = cx.shared.tuning_profile.lock(|profile| *profile);
                    fc.apply_tuning_profile(profile);
                    cx.local
                        .imu_rate_filter
                        .set_alpha(profile.sanitized().imu_lpf_alpha);
                    *cx.local.applied_tuning_seq = pending_seq;
                    info!("Applied disarmed OSD tuning profile {}", pending_seq);
                }

                #[cfg(feature = "blackbox_defmt")]
                {
                    let rc_raw = cx.local.rc_rates_reader.read();
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        false,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        cx.local.control_throttle_reader.read() as f32,
                        [0.0; 4],
                    );
                }
            }

            if !imu_fresh {
                *cx.local.imu_stale_ticks = cx.local.imu_stale_ticks.saturating_add(1);

                if *cx.local.imu_stale_ticks == 1 || (*cx.local.imu_stale_ticks).is_multiple_of(100)
                {
                    warn!(
                        "IMU stale in control loop: seq {}, stale ticks {}",
                        imu_sequence, *cx.local.imu_stale_ticks
                    );
                }

                if cx.local.control_safety_arm_reader.read() {
                    let _ = actuator_output::spawn(safety::ActuatorCmd::Disarm);
                    let _ = safety_master::spawn(safety::SafetyEvent::DisarmRequested);
                }

                return;
            }

            *cx.local.imu_stale_ticks = 0;

            let now_us = Mono::now().duration_since_epoch().to_micros();
            let rc_link = cx.local.control_rc_link_reader.status(now_us);
            if rc_link.valid {
                *cx.local.rc_link_was_valid = true;
            } else {
                if rc_link.timed_out
                    && *cx.local.rc_link_was_valid
                    && safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(
                        safety::RcLinkInvalidation::Timeout,
                    ))
                    .is_ok()
                {
                    *cx.local.rc_link_was_valid = false;
                }

                if control_armed || cx.local.control_arm_permit_reader.read() {
                    return;
                }
            }

            let imu_angles = cx.local.imu_angle_integrator.update_with_accel(
                body_rates,
                drone_gravity,
                dt::CONTROL_LOOP_DT_SECONDS,
            );
            cx.shared.imu_angles.lock(|angles| {
                *angles = imu_angles;
            });

            if control_armed {
                // Read rc_inputs
                let rc_raw = cx.local.rc_rates_reader.read();
                #[cfg(all(
                    not(feature = "blackbox_defmt"),
                    any(
                        feature = "bench_equal_motors",
                        feature = "bench_motor1_only",
                        feature = "bench_motor2_only",
                        feature = "bench_motor3_only",
                        feature = "bench_motor4_only",
                        feature = "bench_logical_motor1_only",
                        feature = "bench_logical_motor2_only",
                        feature = "bench_logical_motor3_only",
                        feature = "bench_logical_motor4_only"
                    )
                ))]
                let _ = rc_raw;

                #[cfg(feature = "bench_motor1_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = [bench_throttle, 0.0, 0.0, 0.0];

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(feature = "bench_motor2_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = [0.0, bench_throttle, 0.0, 0.0];

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(feature = "bench_motor3_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = [0.0, 0.0, bench_throttle, 0.0];

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(feature = "bench_motor4_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = [0.0, 0.0, 0.0, bench_throttle];

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(feature = "bench_logical_motor1_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = remap_motor_outputs(
                        [bench_throttle, 0.0, 0.0, 0.0],
                        board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                    );

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(feature = "bench_logical_motor2_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = remap_motor_outputs(
                        [0.0, bench_throttle, 0.0, 0.0],
                        board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                    );

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(feature = "bench_logical_motor3_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = remap_motor_outputs(
                        [0.0, 0.0, bench_throttle, 0.0],
                        board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                    );

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(feature = "bench_logical_motor4_only")]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = remap_motor_outputs(
                        [0.0, 0.0, 0.0, bench_throttle],
                        board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                    );

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyBenchSelectedMotor,
                        );
                    }

                    return;
                }

                #[cfg(all(
                    feature = "bench_equal_motors",
                    not(any(
                        feature = "bench_motor1_only",
                        feature = "bench_motor2_only",
                        feature = "bench_motor3_only",
                        feature = "bench_motor4_only",
                        feature = "bench_logical_motor1_only",
                        feature = "bench_logical_motor2_only",
                        feature = "bench_logical_motor3_only",
                        feature = "bench_logical_motor4_only"
                    ))
                ))]
                {
                    let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                    let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                    let motor_commands = [bench_throttle; 4];

                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_compact_blackbox(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                        [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        [0.0; 3],
                        bench_throttle,
                        motor_commands,
                    );
                    enqueue_flash_record(
                        cx.local.flash_record_producer,
                        dt::CompactRateBlackboxSample::from_fields(dt::CompactRateBlackboxFields {
                            seq: CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                            imu_seq: imu_sequence,
                            armed: control_armed,
                            imu_fresh,
                            raw_gyro_dps: [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                            filtered_gyro_dps: [
                                imu_roll_filtered,
                                imu_pitch_filtered,
                                imu_yaw_filtered,
                            ],
                            command_dps: [
                                rc_raw.roll as f32,
                                rc_raw.pitch as f32,
                                rc_raw.yaw as f32,
                            ],
                            pid: [0.0; 3],
                            throttle: bench_throttle,
                            motors: motor_commands,
                        }),
                    );
                    {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyLatestThrottle,
                        );
                    }

                    return;
                }

                #[cfg(not(any(
                    feature = "bench_equal_motors",
                    feature = "bench_motor1_only",
                    feature = "bench_motor2_only",
                    feature = "bench_motor3_only",
                    feature = "bench_motor4_only",
                    feature = "bench_logical_motor1_only",
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )))]
                {
                    // In your control loop, at fixed rate:
                    fc.update_throttle_setpoint(cx.local.control_throttle_reader.read() as f32);
                    fc.update_attitude_rate_setpoint(
                        rc_raw.roll as f32,
                        rc_raw.pitch as f32,
                        rc_raw.yaw as f32,
                    ); // deg/s or rad/s, but be consistent
                    fc.update_rate_measured(
                        imu_roll_filtered,
                        imu_pitch_filtered,
                        imu_yaw_filtered,
                    ); // filtered gyro rates
                    fc.update_motor_commands();
                    let motor_commands = remap_motor_outputs(
                        fc.get_logical_motor_commands(),
                        board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                    );

                    {
                        let mut sample = fc.blackbox_sample();
                        sample.motors = remap_motor_outputs(
                            fc.get_logical_motor_commands(),
                            board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                        );
                        let compact = dt::CompactRateBlackboxSample::from_rate_sample(
                            CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                            imu_sequence,
                            control_armed,
                            imu_fresh,
                            [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                            sample,
                        );
                        #[cfg(feature = "blackbox_defmt")]
                        dt::emit_rate_blackbox(compact);
                        enqueue_flash_record(compact);
                    }

                    // Apply throttle
                    {
                        if control_armed {
                            publish_motor_command(
                                motor_commands,
                                safety::ActuatorCmd::ApplyLatestThrottle,
                            );
                        }
                    }
                }
            }
        }
    }

    #[task(priority = 13)]
    async fn actuator_idle_notify(_: actuator_idle_notify::Context) {
        let mut retry_logged = false;
        while safety_master::spawn(safety::SafetyEvent::ActuatorIdling).is_err() {
            if !retry_logged {
                retry_logged = true;
                warn!("Retrying actuator-idle notification");
            }
            Mono::delay(1.millis()).await;
        }
    }

    #[task(
        priority = 13,
        shared = [dshot_motors],
        local = [
            fault_reported: bool = false,
            disarm_pending: bool = false,
            report_ticks: u16 = 0,
            esc_request_consumer,
            esc_ack_producer,
            esc_actuator_request: Option<esc::EscActuatorRequest> = None,
            esc_actuator_request_submitted: bool = false
        ]
    )]
    async fn dshot_service(mut cx: dshot_service::Context) {
        loop {
            let release = Mono::now();
            let next_release = release + board::init::DSHOT_SERVICE_PERIOD_MS.millis();
            let now_ms = release.duration_since_epoch().to_millis();
            if cx.local.esc_actuator_request.is_none() {
                *cx.local.esc_actuator_request = cx.local.esc_request_consumer.dequeue();
                *cx.local.esc_actuator_request_submitted = false;
            }

            let (event, telemetry_sent) = cx.shared.dshot_motors.lock(|dshot| {
                if let Some(request) = *cx.local.esc_actuator_request
                    && !*cx.local.esc_actuator_request_submitted
                {
                    let result = match request.operation {
                        esc::EscOperation::RequestTelemetry => {
                            dshot.request_telemetry(dshot_motor_for_output(request.output))
                        }
                    };
                    match result {
                        Ok(()) => *cx.local.esc_actuator_request_submitted = true,
                        Err(board::init::DshotTelemetryRequestError::Busy) => {}
                        Err(board::init::DshotTelemetryRequestError::Faulted) => {
                            *cx.local.esc_actuator_request = None;
                        }
                    }
                }

                let event = dshot.service(now_ms);
                let telemetry_sent = dshot.take_telemetry_request_sent();
                (event, telemetry_sent)
            });
            if let (Some(request), Some(sent_motor)) =
                (*cx.local.esc_actuator_request, telemetry_sent)
            {
                if sent_motor == dshot_motor_for_output(request.output) {
                    let ack = esc::EscActuatorAck {
                        request,
                        started_at_ms: now_ms,
                    };
                    if cx.local.esc_ack_producer.enqueue(ack).is_err() {
                        warn!("Foxeer ESC actuator acknowledgement queue full");
                    }
                } else {
                    warn!("Foxeer ESC telemetry acknowledgement output mismatch");
                }
                *cx.local.esc_actuator_request = None;
                *cx.local.esc_actuator_request_submitted = false;
            }

            match event {
                board::init::DshotServiceEvent::LeaseExpired => {
                    warn!("Foxeer DShot command lease expired; stop frames selected");
                    *cx.local.disarm_pending = true;
                }
                board::init::DshotServiceEvent::Faulted if !*cx.local.fault_reported => {
                    warn!("Foxeer DShot bank faulted; all outputs forced low");
                    *cx.local.fault_reported = true;
                    *cx.local.disarm_pending = true;
                }
                _ => {}
            }

            if *cx.local.disarm_pending
                && safety_master::spawn(safety::SafetyEvent::DisarmRequested).is_ok()
            {
                *cx.local.disarm_pending = false;
            }

            *cx.local.report_ticks = cx.local.report_ticks.wrapping_add(1);
            if *cx.local.report_ticks >= 1_000 {
                *cx.local.report_ticks = 0;
                let (requested, stats) = cx
                    .shared
                    .dshot_motors
                    .lock(|dshot| (dshot.requested_values(), dshot.stats()));
                info!(
                    "Foxeer DShot values [{}, {}, {}, {}], sets {}/{}, lanes [{}, {}, {}, {}], busy {}, expired {}, timeouts {}, faults {}, at {} ms",
                    requested[0],
                    requested[1],
                    requested[2],
                    requested[3],
                    stats.frames_completed,
                    stats.frames_started,
                    stats.lane_completions[0],
                    stats.lane_completions[1],
                    stats.lane_completions[2],
                    stats.lane_completions[3],
                    stats.busy_skips,
                    stats.lease_expiries,
                    stats.frame_timeouts,
                    stats.dma_faults,
                    now_ms
                );
            }
            Mono::delay_until(next_release).await;
        }
    }

    #[task(binds = DMA2_STREAM1, priority = 16, shared = [dshot_motors])]
    fn dshot_motor1_dma_complete(mut cx: dshot_motor1_dma_complete::Context) {
        service_dshot_dma_irq(
            &mut cx.shared.dshot_motors,
            board::init::DshotMotor::Motor1,
            1,
        );
    }

    #[task(binds = DMA2_STREAM7, priority = 16, shared = [dshot_motors])]
    fn dshot_motor2_dma_complete(mut cx: dshot_motor2_dma_complete::Context) {
        service_dshot_dma_irq(
            &mut cx.shared.dshot_motors,
            board::init::DshotMotor::Motor2,
            7,
        );
    }

    #[task(binds = DMA2_STREAM2, priority = 16, shared = [dshot_motors])]
    fn dshot_motor3_dma_complete(mut cx: dshot_motor3_dma_complete::Context) {
        service_dshot_dma_irq(
            &mut cx.shared.dshot_motors,
            board::init::DshotMotor::Motor3,
            2,
        );
    }

    #[task(binds = DMA2_STREAM6, priority = 16, shared = [dshot_motors])]
    fn dshot_motor4_dma_complete(mut cx: dshot_motor4_dma_complete::Context) {
        service_dshot_dma_irq(
            &mut cx.shared.dshot_motors,
            board::init::DshotMotor::Motor4,
            6,
        );
    }

    // ########### USART1 / BLHeli legacy ESC telemetry #####################
    #[task(binds = DMA2_STREAM5, priority = 5, shared = [uart1_rx])]
    fn usart1_rx_dma_transfer(cx: usart1_rx_dma_transfer::Context) {
        match cx.shared.uart1_rx.service_dma_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered
            | stm32_uart::UartRxIrqOutcome::Ignored
            | stm32_uart::UartRxIrqOutcome::NoChunk => {}
            stm32_uart::UartRxIrqOutcome::DmaError
            | stm32_uart::UartRxIrqOutcome::DeliveryError(_) => {
                ESC_TELEMETRY_DISCONTINUITY.store(true, Ordering::Relaxed);
                warn!("Foxeer USART1 ESC telemetry RX DMA discontinuity");
            }
        }
    }

    #[task(binds = USART1, priority = 5, shared = [uart1_rx])]
    fn usart1_rx_peripheral(cx: usart1_rx_peripheral::Context) {
        match cx.shared.uart1_rx.service_idle_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered
            | stm32_uart::UartRxIrqOutcome::Ignored
            | stm32_uart::UartRxIrqOutcome::NoChunk => {}
            stm32_uart::UartRxIrqOutcome::DmaError
            | stm32_uart::UartRxIrqOutcome::DeliveryError(_) => {
                ESC_TELEMETRY_DISCONTINUITY.store(true, Ordering::Relaxed);
                warn!("Foxeer USART1 ESC telemetry RX IDLE discontinuity");
            }
        }
    }

    #[task(
        priority = 4,
        local = [
            esc_telemetry_uart,
            esc_manager_state,
            esc_request_producer,
            esc_ack_consumer,
            esc_telemetry_update_producer,
            report_ticks: u16 = 0
        ]
    )]
    async fn esc_manager_task(cx: esc_manager_task::Context) {
        loop {
            let release = Mono::now();
            let next_release = release + ESC_MANAGER_PERIOD_MS.millis();
            let now_ms = release.duration_since_epoch().to_millis();

            if ESC_TELEMETRY_DISCONTINUITY.swap(false, Ordering::Relaxed) {
                cx.local.esc_manager_state.record_wire_discontinuity();
            }
            while let Some(ack) = cx.local.esc_ack_consumer.dequeue() {
                if let esc::EscAckOutcome::Sample(update) =
                    cx.local.esc_manager_state.on_actuator_ack(ack)
                {
                    let _ = cx.local.esc_telemetry_update_producer.enqueue(update);
                }
            }
            while let Some(filled) = cx.local.esc_telemetry_uart.filled_consumer.dequeue() {
                if filled.uart_error_seen {
                    cx.local.esc_manager_state.record_wire_discontinuity();
                }
                let len = filled.len.min(filled.buf.len());
                for byte in &filled.buf[..len] {
                    if let Some(update) = cx.local.esc_manager_state.push_wire_byte(*byte, now_ms) {
                        let _ = cx.local.esc_telemetry_update_producer.enqueue(update);
                    }
                }
                if cx
                    .local
                    .esc_telemetry_uart
                    .free_producer
                    .enqueue(filled.buf)
                    .is_err()
                {
                    warn!("Foxeer USART1 ESC telemetry DMA buffer recycle failed");
                }
            }

            cx.local.esc_manager_state.refresh_wire_stats();
            if let Some(timeout) = cx.local.esc_manager_state.poll_timeout(now_ms) {
                match timeout {
                    esc::EscManagerTimeout::ActuatorAck(request) => warn!(
                        "Foxeer ESC telemetry manager latched fault after actuator acknowledgement timeout for physical output {} (logical M{}), request {}",
                        request.output.index() + 1,
                        logical_motor_for_esc_output(request.output),
                        request.sequence
                    ),
                    esc::EscManagerTimeout::TelemetryResponse(request) => warn!(
                        "Foxeer ESC telemetry manager latched fault after response timeout for physical output {} (logical M{}), request {}",
                        request.output.index() + 1,
                        logical_motor_for_esc_output(request.output),
                        request.sequence
                    ),
                }
            }

            if let Some(request) = cx.local.esc_manager_state.next_request(now_ms)
                && cx.local.esc_request_producer.enqueue(request).is_ok()
            {
                let marked = cx
                    .local
                    .esc_manager_state
                    .mark_request_queued(request, now_ms);
                debug_assert!(marked);
            }

            *cx.local.report_ticks = cx.local.report_ticks.wrapping_add(1);
            if *cx.local.report_ticks >= 1_000 {
                *cx.local.report_ticks = 0;
                let stats = cx.local.esc_manager_state.stats();
                for (index, observation) in cx.local.esc_manager_state.samples().iter().enumerate()
                {
                    if let Some(observation) = observation {
                        info!(
                            "Foxeer physical ESC output {} (logical M{}) telemetry: {}.{}V {}.{}A {}mAh {}00eRPM {}C",
                            index + 1,
                            logical_motor_for_physical_index(index),
                            observation.sample.voltage_cv / 100,
                            observation.sample.voltage_cv % 100,
                            observation.sample.current_ca / 100,
                            observation.sample.current_ca % 100,
                            observation.sample.consumption_mah,
                            observation.sample.erpm_div100,
                            observation.sample.temperature_c
                        );
                    }
                }
                info!(
                    "Foxeer ESC telemetry manager: queued/started {}/{}, faulted {}, ack timeouts {}, response timeouts {}, mismatched acks {}, unsolicited {}, valid {}, CRC failures {}, discarded {}",
                    stats.requests_queued,
                    stats.requests_started,
                    cx.local.esc_manager_state.is_faulted(),
                    stats.actuator_ack_timeouts,
                    stats.telemetry_response_timeouts,
                    stats.mismatched_acks,
                    stats.unsolicited_frames,
                    stats.wire.valid_frames,
                    stats.wire.crc_failures,
                    stats.wire.discarded_bytes
                );
            }

            Mono::delay_until(next_release).await;
        }
    }

    #[task(
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
        esc_telemetry_update_consumer,

    ]
    )]
    #[allow(unused_mut)]
    async fn actuator_output(mut cx: actuator_output::Context, cmd: safety::ActuatorCmd) {
        let mut actuator = ActuatorHardware::new(&mut cx.shared.dshot_motors);

        if !ACTUATOR_OUTPUT_ENABLED {
            actuator.force_off();
            if !matches!(cmd, safety::ActuatorCmd::Disarm) {
                warn!("Actuator command inhibited: {}", ACTUATOR_INHIBIT_REASON);
            }
            return;
        }

        let safety_armed = cx.local.actuator_safety_arm_reader.read();
        let output = match cmd {
            safety::ActuatorCmd::Disarm => {
                cx.local.motor_cmd_reader.discard_all();
                cx.local.actuator_arm_done_writer.clear();
                actuator.force_off();
                return;
            }

            safety::ActuatorCmd::EnterIdle => {
                cx.local.motor_cmd_reader.discard_all();
                if let Err(reason) = current_live_arming_guard(
                    cx.local.actuator_arm_permit_reader,
                    cx.local.actuator_rc_link_reader,
                    cx.local.actuator_rc_arm_high_reader,
                    cx.local.actuator_rc_throttle_reader,
                    Mono::now().duration_since_epoch().to_micros(),
                ) {
                    cx.local.actuator_arm_done_writer.clear();
                    actuator.force_off();
                    if safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason)).is_err() {
                        warn!("Failed to report rejected actuator preparation");
                    }
                    return;
                }

                let prepared_output = {
                    info!(
                        "Preparing DShot actuators with {} ms of stop frames",
                        DSHOT_PREARM_STOP_HOLD_MS
                    );
                    cx.local.actuator_arm_done_writer.clear();
                    actuator.force_off();
                    if let Err(reason) = wait_live_arming_hold(
                        cx.local.actuator_arm_permit_reader,
                        cx.local.actuator_rc_link_reader,
                        cx.local.actuator_rc_arm_high_reader,
                        cx.local.actuator_rc_throttle_reader,
                        DSHOT_PREARM_STOP_HOLD_MS,
                        || Mono::now().duration_since_epoch().to_micros(),
                        |delay_ms| Mono::delay(delay_ms.millis()),
                    )
                    .await
                    {
                        cx.local.actuator_arm_done_writer.clear();
                        actuator.force_off();
                        if safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason)).is_err()
                        {
                            warn!("Failed to report aborted DShot pre-arm stop hold");
                        }
                        return;
                    }

                    // Remove samples accumulated while stopped. Qualification
                    // accepts only responses observed after idle spin starts.
                    while cx.local.esc_telemetry_update_consumer.dequeue().is_some() {}

                    let qualification_started_ms = Mono::now().duration_since_epoch().to_millis();
                    let mut qualification = esc::EscIdleQualification::new(
                        DSHOT_IDLE_QUALIFICATION_CONFIG,
                        qualification_started_ms,
                    );
                    info!(
                        "DShot pre-arm stop complete; qualifying idle eRPM {}00..{}00 with {} samples/physical output",
                        DSHOT_IDLE_QUALIFICATION_CONFIG.min_erpm_div100,
                        DSHOT_IDLE_QUALIFICATION_CONFIG.max_erpm_div100,
                        DSHOT_IDLE_QUALIFICATION_CONFIG.required_consecutive_samples
                    );

                    loop {
                        if let Err(reason) = current_live_arming_guard(
                            cx.local.actuator_arm_permit_reader,
                            cx.local.actuator_rc_link_reader,
                            cx.local.actuator_rc_arm_high_reader,
                            cx.local.actuator_rc_throttle_reader,
                            Mono::now().duration_since_epoch().to_micros(),
                        ) {
                            actuator.abort_arming(
                                cx.local.actuator_arm_done_writer,
                                reason,
                                "DShot idle qualification aborted by arming guard",
                                |reason| {
                                    safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason))
                                        .is_ok()
                                },
                            );
                            return;
                        }

                        // Idle remains under the temporary arm permit. Renew
                        // its bounded lease while the system is still disarmed.
                        if !actuator.apply_with_lease(
                            [FOXEER_DSHOT_IDLE_COMMAND; 4],
                            Mono::now().duration_since_epoch().to_millis(),
                            safety::MOTOR_CMD_MAX_AGE_MS,
                        ) {
                            return;
                        }
                        let now_ms = Mono::now().duration_since_epoch().to_millis();
                        let mut status = esc::EscIdleQualificationStatus::Pending;
                        while let Some(update) = cx.local.esc_telemetry_update_consumer.dequeue() {
                            status = qualification
                                .observe(inject_idle_qualification_fault(update), now_ms);
                        }
                        if status == esc::EscIdleQualificationStatus::Pending {
                            status = qualification.status(now_ms);
                        }

                        match status {
                            esc::EscIdleQualificationStatus::Pending => {}
                            esc::EscIdleQualificationStatus::Qualified => break,
                            esc::EscIdleQualificationStatus::Failed(
                                esc::EscIdleQualificationFailure::Overspeed {
                                    output,
                                    erpm_div100,
                                },
                            ) => {
                                warn!(
                                    "Foxeer physical ESC output {} (logical M{}) idle qualification overspeed: {}00 eRPM",
                                    output.index() + 1,
                                    logical_motor_for_esc_output(output),
                                    erpm_div100
                                );
                                actuator.abort_arming(
                                cx.local.actuator_arm_done_writer,
                                safety::ArmingAbortReason::EscIdleRpmOutOfRange,
                                "Foxeer DShot idle qualification rejected an overspeed physical output",
                                |reason| {
                                    safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason))
                                        .is_ok()
                                },
                            );
                                return;
                            }
                            esc::EscIdleQualificationStatus::Failed(
                                esc::EscIdleQualificationFailure::Timeout {
                                    consecutive_samples,
                                },
                            ) => {
                                warn!(
                                    "Foxeer DShot idle qualification timeout; physical outputs 1/2/3/4 samples [{}, {}, {}, {}]",
                                    consecutive_samples[0],
                                    consecutive_samples[1],
                                    consecutive_samples[2],
                                    consecutive_samples[3]
                                );
                                for (index, samples) in
                                    consecutive_samples.iter().copied().enumerate()
                                {
                                    if samples
                                        < DSHOT_IDLE_QUALIFICATION_CONFIG
                                            .required_consecutive_samples
                                    {
                                        warn!(
                                            "Foxeer physical ESC output {} (logical M{}) idle qualification failed: motor not running or RPM evidence invalid ({} of {} samples)",
                                            index + 1,
                                            logical_motor_for_physical_index(index),
                                            samples,
                                            DSHOT_IDLE_QUALIFICATION_CONFIG
                                                .required_consecutive_samples
                                        );
                                    }
                                }
                                actuator.abort_arming(
                                cx.local.actuator_arm_done_writer,
                                safety::ArmingAbortReason::EscIdleTelemetryTimeout,
                                "Foxeer DShot idle qualification did not prove all physical outputs turning",
                                |reason| {
                                    safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason))
                                        .is_ok()
                                },
                            );
                                return;
                            }
                            esc::EscIdleQualificationStatus::Failed(
                                esc::EscIdleQualificationFailure::InvalidConfig,
                            ) => {
                                actuator.abort_arming(
                                    cx.local.actuator_arm_done_writer,
                                    safety::ArmingAbortReason::EscIdleQualificationInvalid,
                                    "Foxeer DShot idle qualification profile is invalid",
                                    |reason| {
                                        safety_master::spawn(safety::SafetyEvent::ArmingAborted(
                                            reason,
                                        ))
                                        .is_ok()
                                    },
                                );
                                return;
                            }
                        }

                        Mono::delay(10.millis()).await;
                    }

                    info!(
                        "Foxeer DShot idle eRPM qualified; physical outputs 1/2/3/4 samples [{}, {}, {}, {}]",
                        qualification.consecutive_samples()[0],
                        qualification.consecutive_samples()[1],
                        qualification.consecutive_samples()[2],
                        qualification.consecutive_samples()[3]
                    );
                    [FOXEER_DSHOT_IDLE_COMMAND; 4]
                };

                cx.local.actuator_arm_done_writer.set_done();

                if actuator_idle_notify::spawn().is_err() {
                    cx.local.actuator_arm_done_writer.clear();
                    actuator.force_off();
                    if safety_master::spawn(safety::SafetyEvent::ArmingAborted(
                        safety::ArmingAbortReason::CompletionDeliveryFailed,
                    ))
                    .is_err()
                    {
                        warn!("Failed to report actuator preparation completion failure");
                    }
                    return;
                }

                prepared_output
            }

            safety::ActuatorCmd::ApplyLatestThrottle if safety_armed => {
                match take_fresh_motor_outputs(
                    cx.local.motor_cmd_reader,
                    Mono::now().duration_since_epoch().to_millis(),
                ) {
                    Some(throttles) => match safety::validate_active_motor_outputs_with_idle(
                        throttles,
                        FOXEER_DSHOT_IDLE_COMMAND,
                    ) {
                        Ok(outputs) => outputs,
                        Err(_) => {
                            warn!("Actuator command refused: invalid motor output");
                            [safety::ESC_LOW_THROTTLE; 4]
                        }
                    },
                    None => [safety::ESC_LOW_THROTTLE; 4],
                }
            }

            #[cfg(any(
                feature = "bench_motor1_only",
                feature = "bench_motor2_only",
                feature = "bench_motor3_only",
                feature = "bench_motor4_only",
                feature = "bench_logical_motor1_only",
                feature = "bench_logical_motor2_only",
                feature = "bench_logical_motor3_only",
                feature = "bench_logical_motor4_only"
            ))]
            safety::ActuatorCmd::ApplyBenchSelectedMotor if safety_armed => {
                let mut outputs = [safety::ESC_LOW_THROTTLE; 4];

                if let Some(throttles) = take_fresh_motor_outputs(
                    cx.local.motor_cmd_reader,
                    Mono::now().duration_since_epoch().to_millis(),
                ) {
                    for index in 0..4 {
                        if !throttles[index].is_finite() {
                            warn!("Bench selected motor command refused: invalid motor output");
                            outputs = [safety::ESC_LOW_THROTTLE; 4];
                            break;
                        }

                        if throttles[index] > 0.0 {
                            let idle = FOXEER_DSHOT_IDLE_COMMAND;
                            outputs[index] = throttles[index].clamp(idle, safety::ESC_MAX_THROTTLE);
                        }
                    }
                }

                outputs
            }

            _ => {
                warn!("Actuator command refused");
                actuator.force_off();
                return;
            }
        };

        if !actuator.apply(output, Mono::now().duration_since_epoch().to_millis()) {
            return;
        }
    }

    // ########### SPI 1 ###################################
    #[task(
        binds = EXTI4,
        priority = 14,
        local = [imu_data_ready],
        shared = [io_timebase]
    )]
    fn imu_data_ready(mut cx: imu_data_ready::Context) {
        if !cx.local.imu_data_ready.check_interrupt() {
            return;
        }
        cx.local.imu_data_ready.clear_interrupt_pending_bit();

        let observed_at = cx.shared.io_timebase.lock(|timebase| timebase.now());
        IMU_DRDY_IRQ_COUNT.fetch_add(1, Ordering::Relaxed);
        IMU_DRDY_LAST_US.store(observed_at.0 as u32, Ordering::Relaxed);

        if IMU_TRANSPORT_READY.load(Ordering::Relaxed) && spi1_poll::spawn(observed_at.0).is_err() {
            IMU_DRDY_REJECTED_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[task(
    priority = 12,
    local = [spi1_device, unavailable_logged: bool = false]
    )]
    async fn spi1_poll(cx: spi1_poll::Context, observed_at_us: u64) {
        let Some(kind) = Spi1ImuKind::from_discriminant(ACTIVE_IMU_KIND.load(Ordering::Relaxed))
        else {
            return;
        };
        let request = kind.dma_burst_register();
        cx.local
            .spi1_device
            .executor_mut()
            .set_start(TimestampMicros(observed_at_us));

        let mut read = [0; SPI_BUFFER_SIZE];
        let mut write = [0; SPI_BUFFER_SIZE];
        write[0] = 0x80 | request;
        let mut operations = [Operation::Transfer(&mut read, &write)];
        let result = cx.local.spi1_device.transaction(&mut operations).await;

        match result {
            Ok(()) => {}
            Err(ferrowasp_io_core::spi::SpiDeviceError::Busy) => {
                info!("SPI1 TX DMA busy");
            }
            Err(ferrowasp_io_core::spi::SpiDeviceError::TooManyOperations)
            | Err(ferrowasp_io_core::spi::SpiDeviceError::TxCapacityExceeded)
            | Err(ferrowasp_io_core::spi::SpiDeviceError::RxCapacityExceeded)
            | Err(ferrowasp_io_core::spi::SpiDeviceError::CopybackShapeMismatch) => {
                warn!("SPI1 transaction packing failed");
            }
            Err(ferrowasp_io_core::spi::SpiDeviceError::Unavailable) => {
                if !*cx.local.unavailable_logged {
                    warn!("SPI1 owner unavailable after recovery failure");
                    *cx.local.unavailable_logged = true;
                }
            }
            Err(ferrowasp_io_core::spi::SpiDeviceError::Timeout) => {}
            Err(ferrowasp_io_core::spi::SpiDeviceError::Cancelled) => {
                info!("SPI1 transaction cancelled");
            }
            Err(ferrowasp_io_core::spi::SpiDeviceError::DmaTransfer) => {
                warn!("SPI1 DMA transaction failed");
            }
            Err(ferrowasp_io_core::spi::SpiDeviceError::InvalidState)
            | Err(ferrowasp_io_core::spi::SpiDeviceError::StaleTransaction)
            | Err(ferrowasp_io_core::spi::SpiDeviceError::Backend) => {
                warn!("SPI1 transaction backend error");
            }
        }
    }

    #[task(priority = 13, shared = [spi1_owner, io_timebase])]
    async fn spi1_owner_service(mut cx: spi1_owner_service::Context) {
        let now = cx.shared.io_timebase.lock(|timebase| timebase.now());
        let outcome = cx.shared.spi1_owner.lock(|owner| {
            critical_section::with(|cs| {
                owner.service_request(&mut SPI1_MAILBOX.borrow_ref_mut(cs), now.0)
            })
        });

        match outcome {
            stm32_spi::SpiOwnerServiceOutcome::Idle
            | stm32_spi::SpiOwnerServiceOutcome::Started
            | stm32_spi::SpiOwnerServiceOutcome::Cancelled => {}
            stm32_spi::SpiOwnerServiceOutcome::RecoveryFailed => {
                warn!("SPI1 cancellation recovery failed; owner disabled");
            }
            stm32_spi::SpiOwnerServiceOutcome::StartFailed(_) => {}
        }
    }

    #[task(binds = DMA2_STREAM0, priority = 13, shared = [spi1_owner])]
    fn spi1_rx_dma(mut cx: spi1_rx_dma::Context) {
        let outcome = cx.shared.spi1_owner.lock(|owner| {
            critical_section::with(|cs| owner.service_dma_irq(&mut SPI1_MAILBOX.borrow_ref_mut(cs)))
        });
        let delivered = match outcome {
            stm32_spi::SpiRxIrqOutcome::Ignored => return,
            stm32_spi::SpiRxIrqOutcome::Delivered => true,
            stm32_spi::SpiRxIrqOutcome::NoChunk => false,
            stm32_spi::SpiRxIrqOutcome::DmaError => {
                warn!("SPI1 RX DMA error");
                return;
            }
            stm32_spi::SpiRxIrqOutcome::DeliveryError(
                stm32_spi::SpiRxDeliveryError::NoFreshBuffer,
            ) => {
                panic!("SPI1 RX free-buffer pool exhausted");
            }
            stm32_spi::SpiRxIrqOutcome::DeliveryError(
                stm32_spi::SpiRxDeliveryError::TransferNotReady,
            ) => {
                info!("SPI1 DMA next_transfer failed");
                return;
            }
            stm32_spi::SpiRxIrqOutcome::DeliveryError(
                stm32_spi::SpiRxDeliveryError::FilledQueueFull,
            ) => {
                panic!("SPI1 filled queue full; RX buffer ownership would be lost");
            }
            stm32_spi::SpiRxIrqOutcome::DeliveryError(
                stm32_spi::SpiRxDeliveryError::PlannerRejected,
            ) => {
                return;
            }
        };

        if delivered {
            let _ = spi1_parser::spawn();
        }
    }

    #[task(
        binds = TIM6_DAC,
        priority = 9,
        local = [io_watchdog],
        shared = [io_timebase]
    )]
    fn io_watchdog(mut cx: io_watchdog::Context) {
        stm32_watchdog::acknowledge_watchdog_tick(cx.local.io_watchdog);

        let now = cx.shared.io_timebase.lock(|timebase| timebase.now());
        let expired = critical_section::with(|cs| {
            SPI1_MAILBOX
                .borrow_ref(cs)
                .lifecycle()
                .deadline_expired(now)
        });

        if expired {
            let _ = spi1_timeout::spawn(now.0);
        }
    }

    #[task(priority = 13, shared = [spi1_owner])]
    async fn spi1_timeout(mut cx: spi1_timeout::Context, observed_at_us: u64) {
        let outcome = cx.shared.spi1_owner.lock(|owner| {
            critical_section::with(|cs| {
                owner.service_timeout(&mut SPI1_MAILBOX.borrow_ref_mut(cs), observed_at_us)
            })
        });

        match outcome {
            stm32_spi::SpiWatchdogOutcome::Idle | stm32_spi::SpiWatchdogOutcome::Active => {}
            stm32_spi::SpiWatchdogOutcome::TimedOut => {
                warn!("SPI1 transaction timed out; DMA ownership recovered");
            }
            stm32_spi::SpiWatchdogOutcome::RecoveryFailed => {
                warn!("SPI1 timeout recovery failed; owner disabled");
            }
        }
    }

    #[task(priority = 11, local = [spi1_parser], shared = [imu_data])]
    async fn spi1_parser(cx: spi1_parser::Context) {
        let mut imu_data = cx.shared.imu_data;
        let active_kind = Spi1ImuKind::from_discriminant(ACTIVE_IMU_KIND.load(Ordering::Relaxed));

        while let Some(filled) = cx.local.spi1_parser.filled_consumer.dequeue() {
            let len = filled.len.min(filled.buf.len());
            let frame = &filled.buf[..len];

            let parsed =
                active_kind.and_then(|kind| {
                    if filled.request != kind.dma_burst_register() {
                        return None;
                    }

                    match kind {
                        Spi1ImuKind::Mpu6500 => {
                            imu::decode_accel_temp_gyro_burst(frame).ok().map(|sample| {
                                let acc_scale = 4_096.0;
                                let gyro_scale = 16.4;
                                ParsedImuSample {
                                    acc: [
                                        sample.acc_raw[0] as f32 / acc_scale,
                                        sample.acc_raw[1] as f32 / acc_scale,
                                        sample.acc_raw[2] as f32 / acc_scale,
                                    ],
                                    gyro: [
                                        sample.gyro_raw[0] as f32 / gyro_scale,
                                        sample.gyro_raw[1] as f32 / gyro_scale,
                                        sample.gyro_raw[2] as f32 / gyro_scale,
                                    ],
                                    gyro_raw: sample.gyro_raw,
                                    temp: sample.temp_raw as f32 / 333.87 + 21.0,
                                }
                            })
                        }
                        Spi1ImuKind::Icm42688P => icm::decode_temp_accel_gyro_burst(frame)
                            .ok()
                            .map(|sample| ParsedImuSample {
                                acc: sample.accel_g(icm::AccelFullScale::G16),
                                gyro: sample.gyro_dps(icm::GyroFullScale::Dps2000),
                                gyro_raw: sample.gyro_raw,
                                temp: sample.temperature_c(),
                            }),
                    }
                });

            match parsed {
                Some(sample) => {
                    #[cfg(feature = "imu_orientation_rtt")]
                    {
                        IMU_ORIENTATION_VERSION.fetch_add(1, Ordering::AcqRel);
                        IMU_LATEST_ACCEL_X_MG
                            .store((sample.acc[0] * 1_000.0) as i32, Ordering::Relaxed);
                        IMU_LATEST_ACCEL_Y_MG
                            .store((sample.acc[1] * 1_000.0) as i32, Ordering::Relaxed);
                        IMU_LATEST_ACCEL_Z_MG
                            .store((sample.acc[2] * 1_000.0) as i32, Ordering::Relaxed);
                        IMU_LATEST_GYRO_X_DPS10
                            .store((sample.gyro[0] * 10.0) as i32, Ordering::Relaxed);
                        IMU_LATEST_GYRO_Y_DPS10
                            .store((sample.gyro[1] * 10.0) as i32, Ordering::Relaxed);
                        IMU_LATEST_GYRO_Z_DPS10
                            .store((sample.gyro[2] * 10.0) as i32, Ordering::Relaxed);
                        IMU_LATEST_TEMP_C10.store((sample.temp * 10.0) as i32, Ordering::Relaxed);
                        IMU_ORIENTATION_VERSION.fetch_add(1, Ordering::Release);
                    }
                    IMU_LATEST_ROLL_RAW.store(sample.gyro_raw[0] as i32, Ordering::Relaxed);
                    IMU_LATEST_PITCH_RAW.store(sample.gyro_raw[1] as i32, Ordering::Relaxed);
                    IMU_LATEST_YAW_RAW.store(sample.gyro_raw[2] as i32, Ordering::Relaxed);
                    IMU_LATEST_SEQ.fetch_add(1, Ordering::Relaxed);

                    imu_data.lock(|data| {
                        data.acc = sample.acc;
                        data.gyro = sample.gyro;
                        data.gyro_raw = sample.gyro_raw;
                        data.temp = sample.temp;
                        data.sequence = data.sequence.wrapping_add(1);
                    });
                }
                None => match active_kind {
                    Some(Spi1ImuKind::Mpu6500) => {
                        warn!("Invalid MPU6500 accel/temp/gyro frame");
                    }
                    Some(Spi1ImuKind::Icm42688P) => {
                        warn!("Invalid ICM42688-P temp/accel/gyro frame");
                    }
                    None => warn!("IMU frame received without an active sensor"),
                },
            }

            cx.local.spi1_parser.free_producer.enqueue(filled.buf).ok();
        }
    }

    // ########### UART 2 ###################################
    #[task(
        binds = DMA1_STREAM5,
        priority = 11,
        shared = [uart2_rx, uart2_bridge]
    )]
    fn usart2_rx_dma_transfer(mut cx: usart2_rx_dma_transfer::Context) {
        let uart = cx.shared.uart2_rx;
        let timestamp = || TimestampMicros(Mono::now().duration_since_epoch().to_micros() as u64);
        let invalidate =
            |reason| safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(reason)).is_ok();
        let delivered = match uart.service_dma_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered => true,
            stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
            stm32_uart::UartRxIrqOutcome::DmaError => {
                warn!("USART2 RX DMA error");
                cx.shared.uart2_bridge.lock(|bridge| {
                    record_uart2_dma_error(bridge, uart.rx_generation(), timestamp(), invalidate)
                });
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::NoFreshBuffer,
            ) => {
                panic!("USART2 RX free-buffer pool exhausted");
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::TransferNotReady,
            ) => {
                info!("USART2 DMA next_transfer failed");
                cx.shared.uart2_bridge.lock(|bridge| {
                    record_uart2_discontinuity(
                        bridge,
                        Discontinuity::TransportReset,
                        uart.rx_generation(),
                        timestamp(),
                        safety::RcLinkInvalidation::TransportDiscontinuity,
                        invalidate,
                    )
                });
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::FilledQueueFull,
            ) => {
                panic!("USART2 filled queue full; RX buffer ownership would be lost");
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::PlannerRejected,
            ) => {
                cx.shared.uart2_bridge.lock(|bridge| {
                    record_uart2_discontinuity(
                        bridge,
                        Discontinuity::TransportReset,
                        uart.rx_generation(),
                        timestamp(),
                        safety::RcLinkInvalidation::TransportDiscontinuity,
                        invalidate,
                    )
                });
                false
            }
        };

        if delivered {
            let generation = uart.rx_generation();
            cx.shared
                .uart2_bridge
                .lock(|bridge| publish_uart2_owned(bridge, generation, timestamp(), invalidate));
        }
    }

    #[task(
        binds = USART2,
        priority = 11,
        shared = [uart2_rx, uart2_bridge]
    )]
    fn usart2_rx_peripheral(mut cx: usart2_rx_peripheral::Context) {
        let uart = cx.shared.uart2_rx;
        let timestamp = || TimestampMicros(Mono::now().duration_since_epoch().to_micros() as u64);
        let invalidate =
            |reason| safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(reason)).is_ok();

        let delivered = match uart.service_idle_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered => true,
            stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
            stm32_uart::UartRxIrqOutcome::DmaError => {
                cx.shared.uart2_bridge.lock(|bridge| {
                    record_uart2_dma_error(bridge, uart.rx_generation(), timestamp(), invalidate)
                });
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::NoFreshBuffer,
            ) => {
                warn!("USART2 RX free-buffer pool exhausted on IDLE");
                cx.shared.uart2_bridge.lock(|bridge| {
                    record_uart2_discontinuity(
                        bridge,
                        Discontinuity::TransportReset,
                        uart.rx_generation(),
                        timestamp(),
                        safety::RcLinkInvalidation::TransportDiscontinuity,
                        invalidate,
                    )
                });
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::TransferNotReady,
            ) => {
                info!("USART2 IDLE next_transfer failed");
                cx.shared.uart2_bridge.lock(|bridge| {
                    record_uart2_discontinuity(
                        bridge,
                        Discontinuity::TransportReset,
                        uart.rx_generation(),
                        timestamp(),
                        safety::RcLinkInvalidation::TransportDiscontinuity,
                        invalidate,
                    )
                });
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::FilledQueueFull,
            ) => {
                panic!("USART2 filled queue full; RX buffer ownership would be lost");
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::PlannerRejected,
            ) => {
                panic!("USART2 RX IDLE planner did not deliver a non-empty buffer");
            }
        };

        if delivered {
            let generation = uart.rx_generation();
            cx.shared
                .uart2_bridge
                .lock(|bridge| publish_uart2_owned(bridge, generation, timestamp(), invalidate));
        }
    }

    // ########### UART 4 / DJI O4 MSP OSD ###################################
    #[task(binds = DMA1_STREAM2, priority = 6, shared = [uart4_rx])]
    fn uart4_rx_dma_transfer(cx: uart4_rx_dma_transfer::Context) {
        let uart = cx.shared.uart4_rx;
        let delivered = match uart.service_dma_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered => true,
            stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
            stm32_uart::UartRxIrqOutcome::DmaError => {
                warn!("UART4 RX DMA error");
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::NoFreshBuffer,
            ) => {
                panic!("UART4 RX free-buffer pool exhausted");
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::TransferNotReady,
            ) => {
                warn!("UART4 DMA next_transfer failed");
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::FilledQueueFull,
            ) => {
                panic!("UART4 filled queue full; RX buffer ownership would be lost");
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::PlannerRejected,
            ) => false,
        };

        if delivered {
            let _ = osd_refresh::spawn();
        }
    }

    #[task(binds = UART4, priority = 6, shared = [uart4_rx])]
    fn uart4_rx_peripheral(cx: uart4_rx_peripheral::Context) {
        let uart = cx.shared.uart4_rx;

        let delivered = match uart.service_idle_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered => true,
            stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
            stm32_uart::UartRxIrqOutcome::DmaError => false,
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::NoFreshBuffer,
            ) => {
                warn!("UART4 RX free-buffer pool exhausted on IDLE");
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::TransferNotReady,
            ) => {
                warn!("UART4 IDLE next_transfer failed");
                false
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::FilledQueueFull,
            ) => {
                panic!("UART4 filled queue full; RX buffer ownership would be lost");
            }
            stm32_uart::UartRxIrqOutcome::DeliveryError(
                stm32_uart::UartRxDeliveryError::PlannerRejected,
            ) => {
                panic!("UART4 RX IDLE planner did not deliver a non-empty buffer");
            }
        };

        if delivered {
            let _ = osd_refresh::spawn();
        }
    }

    #[task(
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
    async fn osd_refresh(mut cx: osd_refresh::Context) {
        loop {
            let rates = cx.local.osd_rc_rates_reader.read();
            let throttle = cx.local.osd_rc_throttle_reader.read();
            let armed = cx.local.osd_safety_arm_reader.read();
            let battery_voltage_v10 = cx.shared.battery_voltage_v10.lock(|value| *value);
            let battery_cell_count = cx.shared.battery_cell_count.lock(|value| *value);
            let battery_cell_voltage_v100 =
                cx.shared.battery_cell_voltage_v100.lock(|value| *value);
            let amperage_ca = cx.shared.battery_current_ca.lock(|value| *value);
            let angles = cx.shared.imu_angles.lock(|angles| *angles);
            let imu_rates = cx.shared.imu_rates.lock(|rates| *rates);
            let imu_sequence = IMU_LATEST_SEQ.load(Ordering::Relaxed);
            let imu_raw = [
                IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed) as i16,
                IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed) as i16,
                IMU_LATEST_YAW_RAW.load(Ordering::Relaxed) as i16,
            ];
            let telemetry = mspv1::MspOsdTelemetry {
                armed,
                battery_voltage_v10,
                battery_cell_count,
                battery_cell_voltage_v100,
                amperage_ca,
                rc_roll: osd::map_rate_to_msp_rc(rates.roll),
                rc_pitch: osd::map_rate_to_msp_rc(rates.pitch),
                rc_yaw: osd::map_rate_to_msp_rc(rates.yaw),
                rc_throttle: osd::map_throttle_to_msp_rc(throttle),
                osd_throttle: throttle.min(2000) as u16,
                roll_deg10: (angles[0] * 10.0) as i16,
                pitch_deg10: (angles[1] * 10.0) as i16,
                yaw_deg: angles[2] as i16,
                imu_roll_dps: imu_rates[0] as i16,
                imu_pitch_dps: imu_rates[1] as i16,
                imu_yaw_dps: imu_rates[2] as i16,
                imu_roll_dps10: (imu_rates[0] * 10.0) as i16,
                imu_pitch_dps10: (imu_rates[1] * 10.0) as i16,
                imu_yaw_dps10: (imu_rates[2] * 10.0) as i16,
                imu_raw,
                imu_sequence,
                imu_stale: IMU_STALE.load(Ordering::Relaxed),
                control_isr_sequence: CONTROL_ISR_SEQ.load(Ordering::Relaxed),
                control_sequence: CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                control_raw: [
                    CONTROL_ROLL_RAW.load(Ordering::Relaxed) as i16,
                    CONTROL_PITCH_RAW.load(Ordering::Relaxed) as i16,
                    CONTROL_YAW_RAW.load(Ordering::Relaxed) as i16,
                ],
                control_dps10: [
                    CONTROL_ROLL_DPS10.load(Ordering::Relaxed) as i16,
                    CONTROL_PITCH_DPS10.load(Ordering::Relaxed) as i16,
                    CONTROL_YAW_DPS10.load(Ordering::Relaxed) as i16,
                ],
                ..mspv1::MspOsdTelemetry::default()
            };

            let menu_active = {
                let mut changed = false;
                let active = cx.shared.tuning_profile.lock(|profile| {
                    let before = *profile;
                    let active = cx.local.osd_task.update_menu(
                        armed,
                        osd::OsdStickRates {
                            roll: rates.roll,
                            pitch: rates.pitch,
                            yaw: rates.yaw,
                        },
                        throttle,
                        profile,
                    );
                    changed = before != *profile;
                    active
                });

                if changed {
                    cx.shared.tuning_request_seq.lock(|seq| {
                        *seq = seq.wrapping_add(1);
                    });
                }

                active
            };

            if let Some(uart) = cx.local.osd_uart.as_mut() {
                while let Some(filled) = uart.filled_consumer.dequeue() {
                    let len = filled.len.min(filled.buf.len());
                    let timestamp =
                        TimestampMicros(Mono::now().duration_since_epoch().to_micros() as u64);
                    let owned = RxChunk::from_slice(
                        &filled.buf[..len],
                        timestamp,
                        filled.completion,
                        filled.generation,
                        filled.uart_error_seen,
                    );
                    uart.free_producer.enqueue(filled.buf).ok();

                    let Ok(owned) = owned else {
                        warn!("UART4 produced an invalid RX chunk");
                        continue;
                    };
                    if cx.local.osd_rx_producer.try_send(owned).is_err() {
                        warn!("UART4 owned RX queue rejected a chunk");
                        continue;
                    }

                    let mut bytes = [0; stm32_uart::UART_RX_BUFFER_SIZE];
                    let Ok(read_len) = cx.local.osd_rx_reader.read(&mut bytes).await else {
                        warn!("UART4 owned RX reader failed");
                        continue;
                    };
                    for byte in &bytes[..read_len] {
                        if let Some(frame_len) =
                            cx.local
                                .osd_task
                                .ingest_byte(*byte, &telemetry, cx.local.osd_tx_buffer)
                        {
                            osd_write(
                                cx.local.osd_tx_writer,
                                cx.local.osd_tx_healthy,
                                &cx.local.osd_tx_buffer[..frame_len],
                            )
                            .await;
                        }
                    }
                }

                if let Some(event) = cx.local.osd_rx_discontinuities.take_new() {
                    warn!("UART4 RX discontinuity sequence {}", event.sequence);
                }
            }

            *cx.local.osd_refresh_tick = cx.local.osd_refresh_tick.wrapping_add(1);
            if *cx.local.osd_refresh_tick >= 10 {
                *cx.local.osd_refresh_tick = 0;

                if let Some(frame_len) = cx.local.osd_task.heartbeat_frame(cx.local.osd_tx_buffer) {
                    osd_write(
                        cx.local.osd_tx_writer,
                        cx.local.osd_tx_healthy,
                        &cx.local.osd_tx_buffer[..frame_len],
                    )
                    .await;
                }

                if menu_active {
                    let tuning = cx.shared.tuning_profile.lock(|profile| *profile);
                    if let Some(frame_len) = cx
                        .local
                        .osd_task
                        .next_menu_frame(&tuning, cx.local.osd_tx_buffer)
                    {
                        osd_write(
                            cx.local.osd_tx_writer,
                            cx.local.osd_tx_healthy,
                            &cx.local.osd_tx_buffer[..frame_len],
                        )
                        .await;
                    }
                } else if let Some(frame_len) = cx
                    .local
                    .osd_task
                    .next_overlay_frame(&telemetry, cx.local.osd_tx_buffer)
                {
                    osd_write(
                        cx.local.osd_tx_writer,
                        cx.local.osd_tx_healthy,
                        &cx.local.osd_tx_buffer[..frame_len],
                    )
                    .await;
                }
            }

            Mono::delay(10.millis()).await;
        }
    }

    #[task(priority = 4, local = [uart4_tx_owner], shared = [uart4_tx_dma])]
    async fn uart4_tx_worker(mut cx: uart4_tx_worker::Context) {
        loop {
            let chunk = match cx.local.uart4_tx_owner.next_chunk().await {
                Ok(chunk) => chunk,
                Err(_error) => {
                    warn!("UART4 TX worker stopped before DMA start");
                    return;
                }
            };

            let start_result = cx
                .shared
                .uart4_tx_dma
                .lock(|tx_dma| tx_dma.start_chunk(&chunk));
            if let Err(error) = start_result {
                let fault = match error {
                    stm32_uart::UartTxStartError::InvalidChunk => SerialFault::InvalidChunk,
                    stm32_uart::UartTxStartError::Busy
                    | stm32_uart::UartTxStartError::TransferMissing => SerialFault::InvalidState,
                };
                cx.local.uart4_tx_owner.fail(fault);
                warn!("UART4 TX DMA start failed");
                return;
            }

            if cx.local.uart4_tx_owner.wait_completion().await.is_err() {
                warn!("UART4 TX worker stopped after DMA start");
                return;
            }
        }
    }

    #[task(
        binds = DMA1_STREAM4,
        priority = 6,
        local = [uart4_tx_completion],
        shared = [uart4_tx_dma]
    )]
    fn uart4_tx_dma_transfer(mut cx: uart4_tx_dma_transfer::Context) {
        let outcome = cx
            .shared
            .uart4_tx_dma
            .lock(stm32_uart::Uart4TxDmaSide::service_irq);

        match outcome {
            stm32_uart::UartTxIrqOutcome::Ignored => {}
            stm32_uart::UartTxIrqOutcome::Completed => {
                if cx.local.uart4_tx_completion.complete().is_err() {
                    warn!("UART4 TX completion arrived without an in-flight chunk");
                }
            }
            stm32_uart::UartTxIrqOutcome::DmaError(error) => {
                cx.local.uart4_tx_completion.fail(SerialFault::DmaTransfer);
                match error {
                    stm32_uart::UartTxDmaError::Transfer => {
                        warn!("UART4 TX DMA transfer error")
                    }
                    stm32_uart::UartTxDmaError::DirectMode => {
                        warn!("UART4 TX DMA direct-mode error")
                    }
                }
            }
        }
    }

    #[task(
        priority = 10,
        local = [
            rc_rx_reader,
            rc_rx_discontinuities,
            sbus,
            arm_qualifier,
            rc_rates_writer,
            rc_throttle_writer,
            rc_arm_high_writer,
            rc_link_frame_writer,
            rc_link_reported_valid: bool = false,
            rc_link_reported_invalidation_seq: u32 = 0
        ],
        shared = [tuning_profile]
    )]
    async fn rc_input(mut cx: rc_input::Context) {
        loop {
            let mut bytes = [0; stm32_uart::UART_RX_BUFFER_SIZE];
            let read_len = match cx.local.rc_rx_reader.read(&mut bytes).await {
                Ok(read_len) => read_len,
                Err(_) => {
                    let _ = safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(
                        safety::RcLinkInvalidation::TransportDiscontinuity,
                    ));
                    neutralize_rc_input(
                        cx.local.arm_qualifier,
                        cx.local.rc_rates_writer,
                        cx.local.rc_throttle_writer,
                        cx.local.rc_arm_high_writer,
                    );
                    *cx.local.rc_link_reported_valid = false;
                    warn!("USART2 owned RX reader stopped");
                    return;
                }
            };

            if let Some(event) = cx.local.rc_rx_discontinuities.take_new() {
                let _ = safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                ));
                cx.local.sbus.reset();
                neutralize_rc_input(
                    cx.local.arm_qualifier,
                    cx.local.rc_rates_writer,
                    cx.local.rc_throttle_writer,
                    cx.local.rc_arm_high_writer,
                );
                *cx.local.rc_link_reported_valid = false;
                warn!("USART2 RX discontinuity sequence {}", event.sequence);
            }

            for packet in cx.local.sbus.push_bytes(&bytes[..read_len]) {
                let pkt = match packet {
                    Ok(packet) => packet,
                    Err(_) => {
                        let _ = safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(
                            safety::RcLinkInvalidation::ParserError,
                        ));
                        neutralize_rc_input(
                            cx.local.arm_qualifier,
                            cx.local.rc_rates_writer,
                            cx.local.rc_throttle_writer,
                            cx.local.rc_arm_high_writer,
                        );
                        *cx.local.rc_link_reported_valid = false;
                        continue;
                    }
                };

                if let Err(reason) =
                    safety::classify_rc_frame_flags(pkt.flags.failsafe, pkt.flags.frame_lost)
                {
                    let _ = safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(reason));
                    neutralize_rc_input(
                        cx.local.arm_qualifier,
                        cx.local.rc_rates_writer,
                        cx.local.rc_throttle_writer,
                        cx.local.rc_arm_high_writer,
                    );
                    *cx.local.rc_link_reported_valid = false;
                    continue;
                }

                let rc_rate_profile = cx.shared.tuning_profile.lock(|profile| profile.rc_rates);
                let rc_cmd = dt::remap_rc_channels_with_profile(
                    pkt.channels[0],
                    pkt.channels[1],
                    pkt.channels[3],
                    pkt.channels[2],
                    rc_rate_profile,
                );
                let arm_high = pkt.channels[8] > safety::ARM_THRESHOLD;
                let now_us = Mono::now().duration_since_epoch().to_micros();
                cx.local.rc_rates_writer.write(safety::RcRates {
                    roll: rc_cmd.roll_dps as i16,
                    pitch: rc_cmd.pitch_dps as i16,
                    yaw: rc_cmd.yaw_dps as i16,
                });
                cx.local.rc_throttle_writer.write(rc_cmd.throttle);
                cx.local.rc_arm_high_writer.write(arm_high);
                let link = cx
                    .local
                    .rc_link_frame_writer
                    .observe_healthy_frame(now_us, arm_high);
                if link.invalidation_sequence != *cx.local.rc_link_reported_invalidation_seq {
                    *cx.local.rc_link_reported_invalidation_seq = link.invalidation_sequence;
                    *cx.local.rc_link_reported_valid = false;
                }
                if link.valid && !*cx.local.rc_link_reported_valid {
                    *cx.local.rc_link_reported_valid = true;
                    info!("RC link valid after healthy-frame qualification");
                }

                if !link.valid || (arm_high && !link.armable) {
                    cx.local.arm_qualifier.reset();
                    continue;
                }

                if let Some(event) = cx.local.arm_qualifier.update(arm_high, now_us) {
                    match event {
                        safety::SafetyEvent::ArmRequested => info!("RC Requests ARM!"),
                        safety::SafetyEvent::DisarmRequested => info!("RC Requests Disarm!"),
                        safety::SafetyEvent::ActuatorIdling
                        | safety::SafetyEvent::ArmingAborted(_)
                        | safety::SafetyEvent::RcLinkInvalid(_) => {}
                    }

                    safety_master::spawn(event).ok();
                }
            }
        }
    }

    // ---- ADC1 ----
    #[task(
        binds = DMA2_STREAM4,
        shared = [
            adc1_transfer,
            battery_voltage_v10,
            battery_cell_count,
            battery_cell_voltage_v100,
            battery_current_ca
        ],
        local = [
            adc1_buffer,
            adc1_planner: stm32_adc::AdcDmaIrqPlanner = stm32_adc::AdcDmaIrqPlanner::new(),
            battery_cell_detector: osd::BatteryCellDetector =
                osd::BatteryCellDetector::new(
                    BATTERY_MAX_CELL_MV,
                    BATTERY_DETECT_CELL_MV,
                    BATTERY_MAX_CELLS,
                ),
            battery_voltage_init_logged: bool = false
        ]
    )]
    fn dma_adc1(mut cx: dma_adc1::Context) {
        let sample = match cx.shared.adc1_transfer.lock(|transfer| {
            stm32_adc::take_completed_adc1_sample_for(
                transfer,
                cx.local.adc1_buffer,
                cx.local.adc1_planner,
            )
        }) {
            Ok(Some(sample)) => sample,
            Ok(None) => return,
            Err(stm32_adc::AdcDmaDeliveryError::DmaFault) => {
                warn!("ADC1 DMA error");
                return;
            }
            Err(stm32_adc::AdcDmaDeliveryError::NoSpareBuffer) => {
                panic!("ADC1 spare buffer missing");
            }
            Err(stm32_adc::AdcDmaDeliveryError::TransferNotReady) => {
                warn!("ADC1 DMA next_transfer failed");
                return;
            }
        };

        // Pull the ADC data out of the buffer that the DMA transfer gave us
        let raw_temp = sample.buffer[0];

        // Now that we're finished with this buffer, put it back in `local.buffer` so it's ready for the next transfer
        // If we don't do this before the next transfer, we'll get a panic
        *cx.local.adc1_buffer = Some(sample.buffer);

        let cal30 = VtempCal30::get().read() as f32;
        let cal110 = VtempCal110::get().read() as f32;

        let _temperature = (110.0 - 30.0) * ((raw_temp as f32) - cal30) / (cal110 - cal30) + 30.0;
        let pack_mv = ((sample.voltage_mv as f32) * ADC_VBAT_DIVIDER_RATIO) as u32;
        let cell_count = cx.local.battery_cell_detector.update(pack_mv);
        let cell_voltage_v100 = osd::pack_millivolts_to_cell_centivolts(pack_mv, cell_count);
        let current_ca = if cell_count == 0 {
            0
        } else {
            osd::current_sample_to_centiamps_with_offset(
                sample.current_mv as u32,
                ADC_CURRENT_BETAFLIGHT_SCALE,
                ADC_CURRENT_OFFSET_MA,
            )
        };

        let battery_voltage_v10 = ((pack_mv + 50) / 100).min(u8::MAX as u32) as u8;
        BATTERY_VOLTAGE_V10_SNAPSHOT.store(u32::from(battery_voltage_v10), Ordering::Relaxed);
        BATTERY_CURRENT_CA_SNAPSHOT.store(i32::from(current_ca), Ordering::Relaxed);
        ADC_VOLTAGE_MV_SNAPSHOT.store(sample.voltage_mv as u32, Ordering::Relaxed);
        ADC_CURRENT_MV_SNAPSHOT.store(sample.current_mv as u32, Ordering::Relaxed);
        cx.shared
            .battery_voltage_v10
            .lock(|value| *value = battery_voltage_v10);
        cx.shared
            .battery_cell_count
            .lock(|battery_cell_count| *battery_cell_count = cell_count);
        cx.shared
            .battery_cell_voltage_v100
            .lock(|battery_cell_voltage_v100| *battery_cell_voltage_v100 = cell_voltage_v100);
        cx.shared
            .battery_current_ca
            .lock(|battery_current_ca| *battery_current_ca = current_ca);

        if !*cx.local.battery_voltage_init_logged {
            let pack_v10 = (pack_mv + 50) / 100;
            info!(
                "Initial battery voltage: {}.{}V",
                pack_v10 / 10,
                pack_v10 % 10
            );
            *cx.local.battery_voltage_init_logged = true;
        }

        let _ = (cell_count, current_ca);
    }

    #[task(shared = [adc1_transfer])]
    async fn adc1_polling(mut cx: adc1_polling::Context) {
        loop {
            cx.shared.adc1_transfer.lock(|transfer| {
                transfer.start(|adc| {
                    adc.start_conversion();
                });
            });

            Mono::delay(100.millis()).await;
        }
    }
}
