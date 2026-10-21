// ####  SET-UP  ####
// Compiler directives
#![deny(unsafe_code)]
#![no_main]
#![no_std]

use ferrowasp_app_tbs_lucid_h7::internal::*;

ferroforge::app! {
    device = pac,
    peripherals = true,
    dispatchers = [FDCAN1_IT0, FDCAN2_IT0, FDCAN1_IT1, FDCAN2_IT1, FDCAN_CAL, SAI1, SAI2, LTDC, DMA2D, CEC],

    use super::*; // Import everything from parent module

    // SAFETY CRITICAL SECTION
    //------------------------------------------------------------------------
    //------------------------------------------------------------------------

    // Monotonicss
    systick_monotonic!(Mono, 1000); // Set mono timer to 1ms resolution

    #[shared]
    struct Shared {
        // UART ports. Each is started at boot for the function saved config
        // binds to it, or left off (`None`) when nothing is bound.
        #[lock_free]
        uart3_rx: Option<board::aliases::Uart3RxPort>,
        uart3_tx_dma: Option<board::aliases::Uart3TxDmaSide>,
        #[lock_free]
        uart6_rx: Option<board::aliases::Uart6RxPort>,
        uart6_tx_dma: Option<board::aliases::Uart6TxDmaSide>,
        #[lock_free]
        uart1_rx: Option<board::aliases::Uart1RxPort>,
        #[lock_free]
        uart8_rx: Option<board::aliases::Uart8RxPort>,

        // IMU
        tuning_profile: dt::TuningProfile,
        tuning_request_seq: u32,

        // ADC
        adc1_transfer: AdcTransfer,

        // SPI1
        spi1_owner: board::aliases::Spi1ImuOwner,
        io_timebase: IoTimebase,
        dshot_motors: DshotShared,

        // The observer's ledger, which its readers can only copy topics out
        // of (section 10).
        ledger: Ledger,
    }
    #[local]
    struct Local {
        // Safety
        arm_qualifier: safety::ArmQualifier,

        // UART
        rc_receiver: RcReceiver,
        rc_telemetry_writer: Option<UartOwnedWriter>,
        esc_telemetry_port: Option<SerialPortEndpoint>,
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
        blackbox_stream: BlackboxStream,
        flash_command_producer: FlashCommandProducer,
        // The Lucid has no configurator port yet, so USB is the only link.
        flash_commands: CommandSources,
        flash_responses: ResponseRouter,
        flash_response_consumer: FlashResponseConsumer,
        flash_rpc_command_producer: FlashRpcCommandProducer,
        flash_rpc_command_consumer: FlashRpcCommandConsumer,
        flash_rpc_response_producer: FlashRpcResponseProducer,
        flash_rpc_response_consumer: FlashRpcResponseConsumer,
        flash_rpc_device_info: flight_tasks::RpcDeviceInfoFn,

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

        // Serial functions: each owns the port config bound to it, if any.
        rc_port: Option<SerialPortEndpoint>,
        osd_port: Option<SerialPortEndpoint>,
        osd_tx_healthy: bool,
        uart3_tx_owner: Option<UartOwnedTxOwner>,
        uart3_tx_completion: Option<UartOwnedTxCompletion>,
        uart6_tx_owner: Option<UartOwnedTxOwner>,
        uart6_tx_completion: Option<UartOwnedTxCompletion>,
        osd_task: osd::OsdTask,
        osd_tx_buffer: [u8; mspv1::OSD_TX_BUFFER_LEN],
        osd_refresh_tick: u8,

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
        esc_safety_arm_reader: signals::SafetyArmReader,
        flash_safety_arm_reader: signals::SafetyArmReader,
        flash_probe: flight_tasks::FlashProbe,
        motor_map_writer: signals::MotorMapWriter,
        actuator_motor_map_reader: signals::MotorMapReader,
        dshot_release_writer: signals::DshotReleaseWriter,
        dshot_release_reader: signals::DshotReleaseReader,
        led_safety_arm_reader: signals::SafetyArmReader,

        // ----  STATUS LEDS  ----
        heartbeat_led: StatusLed,
        armed_led: StatusLed,
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

        // The IMU's sample to the control loop, and the control loop's IMU
        // status to each pre-arm check; and the IMU's boot facts, copied to
        // each task that needs them.
        imu_sample_link: ObserverLink<ImuSample>,
        imu_sample_consumer: ImuSampleConsumer,
        safety_prearm_link: ObserverLink<safety::prearm::ImuPreArmStatus>,
        actuator_prearm_link: ObserverLink<safety::prearm::ImuPreArmStatus>,
        safety_imu_prearm: safety::prearm::ImuPreArmReader<'static>,
        actuator_imu_prearm: safety::prearm::ImuPreArmReader<'static>,
        imu_transport_ready: bool,
        control_imu_ready: bool,
        control_imu_kind: u8,
        poll_imu_kind: u8,
        parser_imu_kind: u8,

        // The observer and each producer's link into it; every end is a
        // local resource of its one task.
        observer: ObserverState,
        control_tick_link: ObserverLink<observed::ControlTick>,
        imu_status_link: ObserverLink<observed::ImuStatus>,
        drdy_status_link: ObserverLink<observed::DrdyStatus>,
        rc_status_link: ObserverLink<observed::RcStatus>,
        safety_status_link: ObserverLink<observed::SafetyStatus>,
        output_status_link: ObserverLink<observed::OutputStatus>,
        esc_status_link: ObserverLink<observed::EscStatus>,
        battery_status_link: ObserverLink<observed::BatteryStatus>,
        storage_status_link: ObserverLink<observed::StorageStatus>,

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
        uart8_rx_buffers: stm32_storage::UartRxBufferBank =
            stm32_storage::new_uart_rx_buffer_bank(),
        uart8_free_queue: stm32_storage::UartRxFreeQueue =
            stm32_storage::UartRxFreeQueue::new(),
        uart8_filled_queue: stm32_storage::UartRxFilledQueue =
            stm32_storage::UartRxFilledQueue::new(),
        uart6_rx_buffers: stm32_storage::UartRxBufferBank =
            stm32_storage::new_uart_rx_buffer_bank(),
        uart6_free_queue: stm32_storage::UartRxFreeQueue =
            stm32_storage::UartRxFreeQueue::new(),
        uart6_filled_queue: stm32_storage::UartRxFilledQueue =
            stm32_storage::UartRxFilledQueue::new(),
        uart3_rx_buffers: stm32_storage::UartRxBufferBank =
            stm32_storage::new_uart_rx_buffer_bank(),
        uart3_free_queue: stm32_storage::UartRxFreeQueue =
            stm32_storage::UartRxFreeQueue::new(),
        uart3_filled_queue: stm32_storage::UartRxFilledQueue =
            stm32_storage::UartRxFilledQueue::new(),
        uart3_tx_buffer: stm32_storage::UartTxBuffer = [0; mspv1::OSD_TX_BUFFER_LEN],
        uart6_tx_buffer: stm32_storage::UartTxBuffer = [0; mspv1::OSD_TX_BUFFER_LEN],
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
        // Before anything is configured: a `reboot dfu` request ends here.
        stm32_bootloader::enter_rom_bootloader_if_requested();
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
        let leds = board::init::init_status_leds(board::init::StatusLedPins {
            heartbeat: gpioe.pe4,
            armed: gpioe.pe3,
        });

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

        let control_loop_scheduler =
            board::aliases::ControlScheduler::new(dp.TIM4, rec.TIM4, &clocks, scheduler_rate.to_Hz());
        let io_timebase = board::aliases::IoTimebase::new(dp.TIM2, rec.TIM2, &clocks);
        let io_watchdog =
            board::init::init_io_watchdog(dp.TIM6, rec.TIM6, &clocks);

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

        let flight_tasks::EscChannels {
            esc_manager_state,
            esc_request_producer,
            esc_request_consumer,
            esc_ack_producer,
            esc_ack_consumer,
            esc_telemetry_update_producer,
            esc_telemetry_update_consumer,
        } = flight_tasks::esc_channels();

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
                let _ = spi1_imu::spi1_owner_service::spawn();
            },
        ));
        let mut flash_probe = flight_tasks::FlashProbe::default();
        let mut flash_device = {
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
                    let capacity = id.capacity_bytes().unwrap_or(0);
                    let supported =
                        id.plausible() && flash_task::StorageLayout::new(capacity).is_some();
                    flash_probe = flight_tasks::FlashProbe {
                        ready: supported,
                        jedec: [id.manufacturer, id.memory_type, id.capacity_code],
                        capacity_bytes: capacity,
                    };
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
            flash
        };
        let flight_tasks::StorageChannels {
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
        } = flight_tasks::storage_channels();
        let flight_tasks::ObserverChannels {
            observer,
            control_tick_link,
            imu_status_link,
            drdy_status_link,
            rc_status_link,
            safety_status_link,
            output_status_link,
            esc_status_link,
            battery_status_link,
            storage_status_link,
            blackbox_stream,
        } = flight_tasks::observer_channels(
            cortex_m::singleton!(
                : BlackboxStreamQueue<BLACKBOX_STREAM_CAPACITY> = BlackboxStreamQueue::new()
            )
            .unwrap(),
        );
        let flight_tasks::ImuChannels {
            imu_sample_link,
            imu_sample_consumer,
            safety_prearm_link,
            safety_imu_prearm,
            actuator_prearm_link,
            actuator_imu_prearm,
        } = flight_tasks::imu_channels();
        let imu_ready = spi1_imu.bringup.is_ready();
        let imu_kind = spi1_imu.bringup.kind().map_or(0, |kind| kind as u8);
        let identity = Identity {
            imu_ready,
            imu_kind,
            flash_jedec: flash_probe.jedec,
            flash_capacity_bytes: flash_probe.capacity_bytes,
        };

        // ------------  UART ports, bound at boot  ------------
        // Pins and DMA streams are fixed per port. Saved config chooses each
        // port's function and line settings; it is read once, here, so a
        // changed binding applies after a reboot.
        let serial_bindings = boot_serial_bindings(&mut flash_device, &flash_probe);
        let board::init::UartPorts {
            uart3_rx,
            uart3_tx:
                stm32_port::UartTxPort {
                    dma: uart3_tx_dma,
                    owner: uart3_tx_owner,
                    completion: uart3_tx_completion,
                },
            uart6_rx,
            uart6_tx:
                stm32_port::UartTxPort {
                    dma: uart6_tx_dma,
                    owner: uart6_tx_owner,
                    completion: uart6_tx_completion,
                },
            uart8_rx,
            uart1_rx,
            functions: serial_functions,
        } = board::init::init_uart_ports(
            board::init::UartPortResources {
                uart1_rx_pin: gpioa.pa10,
                uart1: dp.USART1,
                uart1_prec: rec.USART1,
                uart1_rx_dma: dma2.4,
                uart3_tx_pin: gpiod.pd8,
                uart3_rx_pin: gpiod.pd9,
                uart3: dp.USART3,
                uart3_prec: rec.USART3,
                uart3_rx_dma: dma1.1,
                uart3_tx_dma: dma1.3,
                uart6_tx_pin: gpioc.pc6,
                uart6_rx_pin: gpioc.pc7,
                uart6: dp.USART6,
                uart6_prec: rec.USART6,
                uart6_rx_dma: dma1.0,
                uart6_tx_dma: dma1.6,
                uart8_rx_pin: gpioe.pe0,
                uart8: dp.UART8,
                uart8_prec: rec.UART8,
                uart8_rx_dma: dma1.2,
            },
            &clocks,
            board::init::UartPortStorage {
                uart1: stm32_port::UartRxPortStorage {
                    rx: stm32_storage::UartRxStorageResources {
                        buffers: cx.local.uart1_rx_buffers,
                        free_queue: cx.local.uart1_free_queue,
                        filled_queue: cx.local.uart1_filled_queue,
                    },
                    stream: cortex_m::singleton!(
                        : UartOwnedRxChannel = UartOwnedRxChannel::new()
                    )
                    .unwrap(),
                },
                uart3: stm32_port::UartRxTxPortStorage {
                    rx: stm32_storage::UartRxStorageResources {
                        buffers: cx.local.uart3_rx_buffers,
                        free_queue: cx.local.uart3_free_queue,
                        filled_queue: cx.local.uart3_filled_queue,
                    },
                    stream: cortex_m::singleton!(
                        : UartOwnedRxChannel = UartOwnedRxChannel::new()
                    )
                    .unwrap(),
                    tx_buffer: cx.local.uart3_tx_buffer,
                    tx_stream: cortex_m::singleton!(
                        : UartOwnedTxChannel = UartOwnedTxChannel::new()
                    )
                    .unwrap(),
                },
                uart6: stm32_port::UartRxTxPortStorage {
                    rx: stm32_storage::UartRxStorageResources {
                        buffers: cx.local.uart6_rx_buffers,
                        free_queue: cx.local.uart6_free_queue,
                        filled_queue: cx.local.uart6_filled_queue,
                    },
                    stream: cortex_m::singleton!(
                        : UartOwnedRxChannel = UartOwnedRxChannel::new()
                    )
                    .unwrap(),
                    tx_buffer: cx.local.uart6_tx_buffer,
                    tx_stream: cortex_m::singleton!(
                        : UartOwnedTxChannel = UartOwnedTxChannel::new()
                    )
                    .unwrap(),
                },
                uart8: stm32_port::UartRxPortStorage {
                    rx: stm32_storage::UartRxStorageResources {
                        buffers: cx.local.uart8_rx_buffers,
                        free_queue: cx.local.uart8_free_queue,
                        filled_queue: cx.local.uart8_filled_queue,
                    },
                    stream: cortex_m::singleton!(
                        : UartOwnedRxChannel = UartOwnedRxChannel::new()
                    )
                    .unwrap(),
                },
            },
            &serial_bindings,
        );
        // RC input reads the receiver; battery telemetry back to it, when the
        // protocol talks back, has its own low-priority task and writer.
        let rc_receiver = rc_receiver_for(serial_bindings.rc_protocol());
        let mut rc_port = serial_functions.rc_input;
        let rc_telemetry_writer = rc_port.as_mut().and_then(|port| port.writer.take());

        // Init rate controller
        let tuning_profile = DEFAULT_TUNING;
        let flight_controller = dt::FlightController::new(
            dt::FlightControllerConfig::default(),
            dt::RateController::new(tuning_profile.rate_gains, dt::RATE_CONTROLLER_OUTPUT_LIMIT),
        );

        // ############ SAFETY HANDLES ###########
        let (rc_arm_high_writer, rc_arm_high_reader) = signals::split_rc_arm_high(&RC_ARM_HIGH);
        let (rc_throttle_writer, rc_throttle_reader) = signals::split_rc_throttle(&RC_THROTTLE);
        let (safety_arm_writer, safety_arm_reader) = signals::split_safety_arm(&SAFETY_ARMED);
        let (motor_map_writer, motor_map_reader) = signals::split_motor_map(&MOTOR_OUTPUT_MAP);
        let (dshot_release_writer, dshot_release_reader) =
            signals::split_dshot_release(&DSHOT_LAST_RELEASE);
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
        info!("TBS Lucid H7 BLHeli telemetry-qualified DShot arming reads the port bound to esc_telemetry");
        #[cfg(feature = "bench_dshot_idle_output1_not_running")]
        warn!(
            "FAULT INJECTION ACTIVE: TBS Lucid H7 physical ESC output 1 (logical M1/rear-right) idle qualification eRPM forced to zero; arming must fail"
        );
        #[cfg(feature = "bench_prearm_imu_stale")]
        warn!("FAULT INJECTION ACTIVE: pre-arm IMU freshness forced stale; arming must fail");
        flight::heartbeat::spawn().unwrap();
        flight::observer_task::spawn().unwrap();
        status_leds::spawn().unwrap();
        flight::adc1_polling::spawn().ok();
        uart3_tx_worker::spawn().unwrap();
        uart6_tx_worker::spawn().unwrap();
        flight::rc_telemetry::spawn().unwrap();
        flight::rc_input::spawn().unwrap();
        flight::osd_refresh::spawn().ok();
        flight::dshot_fallback::spawn().unwrap();
        flight::esc_manager_task::spawn().unwrap();
        flight::flash_manager_task::spawn().unwrap();

        (
            Shared {
                uart1_rx,
                uart3_rx,
                uart3_tx_dma,
                uart6_rx,
                uart6_tx_dma,
                uart8_rx,

                // SPI1
                spi1_owner: spi1_imu.owner,
                io_timebase,
                dshot_motors,
                ledger: Ledger::new(identity),

                // IMU
                tuning_profile,
                tuning_request_seq: 0,

                // ADC
                adc1_transfer: adc1_battery.transfer,
                // SAFETY
            },
            Local {
                // Safety
                arm_qualifier: safety::ArmQualifier::default(),

                // UART
                rc_receiver,
                rc_telemetry_writer,
                esc_telemetry_port: serial_functions.esc_telemetry,
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
                blackbox_stream,
                flash_command_producer,
                flash_commands: CommandSources::new(flash_command_consumer, None),
                flash_responses: ResponseRouter::new(flash_response_producer, None),
                flash_response_consumer,
                flash_rpc_command_producer,
                flash_rpc_command_consumer,
                flash_rpc_response_producer,
                flash_rpc_response_consumer,
                #[cfg(feature = "mspv2_configurator")]
                flash_rpc_device_info: rpc_device_info,
                #[cfg(not(feature = "mspv2_configurator"))]
                flash_rpc_device_info: (),

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
                rc_port,
                osd_port: serial_functions.msp_display_port,
                osd_tx_healthy: true,
                uart3_tx_owner,
                uart3_tx_completion,
                uart6_tx_owner,
                uart6_tx_completion,
                osd_task: osd::OsdTask::new(),
                osd_tx_buffer: [0; mspv1::OSD_TX_BUFFER_LEN],
                osd_refresh_tick: 0,

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
                esc_safety_arm_reader: safety_arm_reader,
                flash_safety_arm_reader: safety_arm_reader,
                flash_probe,
                motor_map_writer,
                actuator_motor_map_reader: motor_map_reader,
                dshot_release_writer,
                dshot_release_reader,
                led_safety_arm_reader: safety_arm_reader,
                heartbeat_led: leds.heartbeat,
                armed_led: leds.armed,
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

                // IMU links and boot facts
                imu_sample_link,
                imu_sample_consumer,
                safety_prearm_link,
                actuator_prearm_link,
                safety_imu_prearm,
                actuator_imu_prearm,
                imu_transport_ready: imu_ready,
                control_imu_ready: imu_ready,
                control_imu_kind: imu_kind,
                poll_imu_kind: imu_kind,
                parser_imu_kind: imu_kind,

                // Observer
                observer,
                control_tick_link,
                imu_status_link,
                drdy_status_link,
                rc_status_link,
                safety_status_link,
                output_status_link,
                esc_status_link,
                battery_status_link,
                storage_status_link,

                // USB CDC serial
                usb_dev,
                usb_serial,
                usb_header_sent: false,
                configurator_usb: ConfiguratorUsbState::new(),
            },
        )
    }

    // The flight tasks every board runs the same way. Each member's own
    // resources and configuration are in its definition in
    // ferrowasp-stm32-tasks; this selection binds them once.
    #[group(
        from = flight_tasks::groups::flight,
        local = [
            // safety_master
            safety_rc_arm_high_reader, safety_rc_throttle_reader, safety_rc_link_reader,
            safety_arm_writer, rc_link_invalidator, actuator_arm_permit_writer,
            actuator_arm_done_reader, safety_status_link, safety_imu_prearm,
            // usb_fs
            usb_dev, usb_serial, usb_header_sent, flash_command_producer, flash_response_consumer,
            flash_rpc_command_producer, flash_rpc_response_consumer, configurator_usb,
            // flash_manager_task
            flash_device, flash_record_producer, flash_record_consumer, blackbox_stream,
            storage_status_link, flash_safety_arm_reader, flash_probe, flash_commands,
            flash_responses,
            flash_rpc_command_consumer, flash_rpc_response_producer, flash_rpc_device_info,
            stored_config: flash_task::StoredConfig = DEFAULT_STORED_CONFIG,
            // control_loop
            control_loop_cnt, samples_per_control_loop, flight_controller, control_loop_scheduler,
            imu_rate_filter, imu_angle_integrator, gyro_axis_map, gyro_bias_calibrator,
            imu_last_sequence, imu_stale_ticks, applied_tuning_seq, rc_rates_reader,
            control_throttle_reader, control_safety_arm_reader, control_rc_link_reader,
            control_arm_permit_reader, motor_cmd_writer, motor_cmd_seq, control_tick_link,
            imu_sample_consumer, safety_prearm_link, actuator_prearm_link, control_imu_ready,
            control_imu_kind, motor_map_writer,
            // dshot_service
            esc_request_consumer, esc_ack_producer, output_status_link, dshot_release_writer,
            // dshot_fallback
            dshot_release_reader,
            // esc_manager_task
            esc_telemetry_port, esc_manager_state, esc_request_producer, esc_ack_consumer,
            esc_telemetry_update_producer, esc_status_link, esc_safety_arm_reader,
            // actuator_output
            actuator_safety_arm_reader, actuator_arm_permit_reader, actuator_rc_arm_high_reader,
            actuator_rc_throttle_reader, actuator_rc_link_reader, actuator_arm_done_writer,
            motor_cmd_reader, esc_telemetry_update_consumer, actuator_imu_prearm,
            actuator_motor_map_reader,
            // rc_input
            rc_port, rc_receiver, arm_qualifier, rc_rates_writer, rc_throttle_writer,
            rc_arm_high_writer, rc_link_frame_writer, rc_status_link,
            // rc_telemetry
            rc_telemetry_writer,
            // osd_refresh
            osd_port, osd_tx_healthy, osd_task, osd_tx_buffer, osd_refresh_tick,
            osd_safety_arm_reader,
            // dma_adc1
            adc1_buffer, battery_status_link,
            battery_cell_detector: osd::BatteryCellDetector = osd::BatteryCellDetector::new(
                BATTERY_MAX_CELL_MV,
                BATTERY_DETECT_CELL_MV,
                BATTERY_MAX_CELLS,
            ),
            // observer_task
            observer
        ],
        shared = [
            tuning_profile,
            tuning_request_seq,
            dshot_motors,
            ledger,
            adc1_transfer
        ],
        config = [
            actuator_output_enabled: bool = ACTUATOR_OUTPUT_ENABLED,
            actuator_inhibit_reason: &'static str = ACTUATOR_INHIBIT_REASON,
            bench_actuator_validation_enabled: bool = BENCH_ACTUATOR_VALIDATION_ENABLED,
            validate_live_arming_guard: flight_tasks::ArmingGuard = validate_live_arming_guard,
            usb_debug_header: &'static [u8] = USB_DEBUG_HEADER,
            control_loop_hz: u32 = board::profiles::CONTROL_LOOP_RATE_HZ,
            msp_common_info: MspCommonInfoFn = MSP_COMMON_INFO,
            rpc_device_info: flight_tasks::RpcDeviceInfoFn = RPC_DEVICE_INFO,
            build_identity: flight_tasks::BuildIdentity = BUILD_IDENTITY,
            physical_imu_to_drone_rotation: dt::FrameRotation =
                IMU_CONTROL_AXIS_PROFILE.imu_to_drone_rotation(),
            control_imu_to_rate_controller_map: dt::FrameRotation =
                IMU_CONTROL_AXIS_PROFILE.imu_to_rate_controller_map(),
            default_stored_config: flash_task::StoredConfig = DEFAULT_STORED_CONFIG,
            default_serial_bindings: SerialBindings = board::serial::DEFAULT_SERIAL_BINDINGS,
            serial_routes: &'static [SerialRoute] = board::serial::SERIAL_ROUTES,
            imu_control_axis_profile: ferrowasp_control::frames::ImuControlAxisProfile =
                IMU_CONTROL_AXIS_PROFILE,
            imu_gyro_counts_per_dps: f32 = IMU_GYRO_COUNTS_PER_DPS,
            logical_to_physical_motor_output: [usize; 4] =
                board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
            control_loop_rate_hz: u32 = board::profiles::CONTROL_LOOP_RATE_HZ,
            dshot_motor_for_output: fn(esc::EscOutput) -> board::init::DshotMotor =
                dshot_motor_for_output,
            esc_output_to_logical_motor: [u8; 4] = [
                logical_motor_for_physical_index(0),
                logical_motor_for_physical_index(1),
                logical_motor_for_physical_index(2),
                logical_motor_for_physical_index(3),
            ],
            dshot_idle_command: f32 = BOARD_DSHOT_IDLE_COMMAND,
            adc_vbat_divider_ratio: f32 = ADC_VBAT_DIVIDER_RATIO,
            adc_current_betaflight_scale: u32 = ADC_CURRENT_BETAFLIGHT_SCALE,
            adc_current_offset_ma: i32 = ADC_CURRENT_OFFSET_MA,
        ]
    )]
    mod flight {
        #[task(priority = 16)]
        async fn safety_master;
        // USB CDC: command line, bench motor requests, status.
        #[task(binds = OTG_FS, priority = 5)]
        fn usb_fs;
        #[task(priority = 1)]
        async fn heartbeat;
        // The border band (6 and 7): below the core, above every task that
        // is not safety-critical. One bounded run a millisecond.
        #[task(priority = 7)]
        async fn observer_task;
        // Lowest priority: storage never preempts flight work.
        #[task(priority = 1)]
        async fn flash_manager_task;
        #[task(binds = TIM4, priority = 14)]
        fn control_loop;
        #[task(priority = 13)]
        async fn actuator_idle_notify;
        // Released by each accepted motor command, so frames follow the
        // control loop, and by the fallback when none arrives (ADR-0046).
        #[task(priority = 13)]
        async fn dshot_service;
        // Same priority as the service it releases, as the old 2 ms service
        // loop had: it only checks a timestamp and spawns, every 2 ms.
        #[task(priority = 13)]
        async fn dshot_fallback;
        // BLHeli legacy ESC telemetry, on the port bound to `esc_telemetry`.
        // Actuator Feedback, so core: above USB, the OSD and the border roles
        // (6-7), below the I/O watchdog. Each 2 ms run is bounded by the bytes
        // one period brings.
        #[task(priority = 8)]
        async fn esc_manager_task;
        #[task(priority = 15)]
        async fn actuator_output;
        #[task(priority = 10)]
        async fn rc_input;
        // Battery telemetry back through the RC receiver, when the RC protocol
        // talks back. Lowest priority: it only reads the ledger.
        #[task(priority = 1)]
        async fn rc_telemetry;
        #[task(priority = 3)]
        async fn osd_refresh;
        #[task(binds = ADC1_2, priority = 1)]
        fn dma_adc1;
        #[task(priority = 1)]
        async fn adc1_polling;
    }

    // Lowest priority, so a lit-and-blinking heartbeat shows that every
    // task above it leaves the core time to run.
    #[task(
        from = flight_tasks::status_leds,
        priority = 1,
        local = [heartbeat_led, armed_led, led_safety_arm_reader]
    )]
    async fn status_leds(cx: status_leds::Context);

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

    // ########### SPI 1 ###################################
    // Above every UART transport (11): a gyro sample published late makes
    // the next control tick stale, and one stale tick disarms. UART work at
    // the same level delayed it enough, with ESC telemetry and the OSD busy,
    // to disarm moments after arming. See mdbook/src/priority.md.
    #[group(
        from = flight_tasks::groups::spi1_imu,
        local = [imu_data_ready, spi1_device, io_watchdog, spi1_parser,
            imu_status_link, drdy_status_link, imu_sample_link, imu_transport_ready,
            poll_imu_kind, parser_imu_kind],
        shared = [io_timebase, spi1_owner],
        config = [spi1_imu_burst_register: fn(u8) -> Option<u8> = spi1_imu_burst_register],
    )]
    mod spi1_imu {
        #[task(binds = EXTI2, priority = 14)]
        fn imu_data_ready;
        #[task(priority = 12)]
        async fn spi1_poll;
        #[task(priority = 13)]
        async fn spi1_owner_service;
        #[task(binds = DMA1_STR4, priority = 13)]
        fn spi1_rx_dma;
        #[task(binds = TIM6_DAC, priority = 9)]
        fn io_watchdog;
        #[task(priority = 13)]
        async fn spi1_timeout;
        #[task(priority = 12)]
        async fn spi1_parser;
    }

    // ########### UART ports ##############################################
    // Every UART transport runs at priority 11, whatever function its port
    // serves; the function tasks reading the streams keep their own
    // priorities. SPI transports run at 13.
    #[task(from = flight_tasks::uart3_rx_dma, binds = DMA1_STR1, priority = 11, shared = [uart3_rx])]
    fn uart3_rx_dma(cx: uart3_rx_dma::Context);

    #[task(from = flight_tasks::uart3_rx_idle, binds = USART3, priority = 11, shared = [uart3_rx])]
    fn uart3_rx_idle(cx: uart3_rx_idle::Context);

    #[task(from = flight_tasks::uart6_rx_dma, binds = DMA1_STR0, priority = 11, shared = [uart6_rx])]
    fn uart6_rx_dma(cx: uart6_rx_dma::Context);

    #[task(from = flight_tasks::uart6_rx_idle, binds = USART6, priority = 11, shared = [uart6_rx])]
    fn uart6_rx_idle(cx: uart6_rx_idle::Context);

    #[task(from = flight_tasks::uart1_rx_dma, binds = DMA2_STR4, priority = 11, shared = [uart1_rx])]
    fn uart1_rx_dma(cx: uart1_rx_dma::Context);

    #[task(from = flight_tasks::uart1_rx_idle, binds = USART1, priority = 11, shared = [uart1_rx])]
    fn uart1_rx_idle(cx: uart1_rx_idle::Context);

    #[task(from = flight_tasks::uart8_rx_dma, binds = DMA1_STR2, priority = 11, shared = [uart8_rx])]
    fn uart8_rx_dma(cx: uart8_rx_dma::Context);

    #[task(from = flight_tasks::uart8_rx_idle, binds = UART8, priority = 11, shared = [uart8_rx])]
    fn uart8_rx_idle(cx: uart8_rx_idle::Context);

    #[task(
        from = flight_tasks::uart6_tx_worker,
        priority = 11,
        local = [uart6_tx_owner = uart6_tx_owner],
        shared = [uart6_tx_dma = uart6_tx_dma],
    )]
    async fn uart6_tx_worker(cx: uart6_tx_worker::Context);

    #[task(
        from = flight_tasks::uart6_tx_dma_complete,
        binds = DMA1_STR6,
        priority = 11,
        local = [uart6_tx_completion],
        shared = [uart6_tx_dma]
    )]
    fn uart6_tx_dma_complete(cx: uart6_tx_dma_complete::Context);

    #[task(
        from = flight_tasks::uart3_tx_worker,
        priority = 11,
        local = [uart3_tx_owner = uart3_tx_owner],
        shared = [uart3_tx_dma = uart3_tx_dma],
    )]
    async fn uart3_tx_worker(cx: uart3_tx_worker::Context);

    #[task(
        from = flight_tasks::uart3_tx_dma_complete,
        binds = DMA1_STR3,
        priority = 11,
        local = [uart3_tx_completion],
        shared = [uart3_tx_dma]
    )]
    fn uart3_tx_dma_complete(cx: uart3_tx_dma_complete::Context);

}
