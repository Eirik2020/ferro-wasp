// ####  SET-UP  ####
// Compiler directives
#![deny(unsafe_code)]
#![no_main]
#![no_std]

use core::cell::RefCell;
use core::sync::atomic::Ordering;
use critical_section::Mutex;
use defmt::{info, warn};
use defmt_rtt as _;
use ferrowasp_bsp::stm32f4::foxeer_f405_v2 as board;
use ferrowasp_core::actuator::throttle_to_u16;
use ferrowasp_core::safety;
use ferrowasp_drivers::{icm42688p as icm, mpu6500 as imu};
use ferrowasp_io_core::serial::{UART2_CONSUMER, UART4_CONSUMER, route_uart_to_task};
use ferrowasp_io_core::spi::{
    AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiDeadlineUs,
    SpiRequestMailbox,
};
use ferrowasp_io_core::{
    serial::{Discontinuity, RxChunk, SerialFault},
    time::TimestampMicros,
};
use ferrowasp_mspv1 as mspv1;
use ferrowasp_stm32f4::adc as stm32_adc;
use ferrowasp_stm32f4::hal_prelude::*;
use ferrowasp_stm32f4::memory as stm32_memory;
use ferrowasp_stm32f4::scheduler as stm32_scheduler;
use ferrowasp_stm32f4::spi_dma as stm32_spi;
use ferrowasp_stm32f4::spi_dma::*;
use ferrowasp_stm32f4::timebase as stm32_timebase;
use ferrowasp_stm32f4::uart_dma as stm32_uart;
use ferrowasp_stm32f4::watchdog as stm32_watchdog;
use ferrowasp_tasks::drone_toolbox as dt;
use ferrowasp_tasks::osd;
use ferrowasp_tasks::usb_debug;
use fugit::Rate;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;
use sbus_rs::StreamingParser;
#[cfg(feature = "usb_serial")]
use stm32f4xx_hal::otg_fs::USB;
use stm32f4xx_hal::otg_fs::UsbBusType;
use usb_device::device::{UsbDevice, UsbDeviceState};
#[cfg(feature = "usb_serial")]
use usb_device::{
    bus::UsbBusAllocator,
    device::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
};
use usbd_serial::SerialPort;

type AdcTransfer = board::aliases::Adc1ObservationTransfer;
type ControlScheduler = board::aliases::ControlScheduler;
type IoTimebase = stm32_timebase::MicrosecondTimebase<board::aliases::IoTimebaseTimer>;
type IoWatchdog = board::aliases::IoWatchdog;
type Uart4OwnedRxChannel = stm32_memory::UartOwnedRxChannel;
type Uart4OwnedRxProducer = stm32_memory::UartOwnedRxProducer<'static>;
type Uart4OwnedReader = stm32_memory::UartOwnedReader<'static>;
type Uart4Discontinuities = stm32_memory::UartOwnedDiscontinuities<'static>;
type Uart2OwnedRxChannel = stm32_memory::UartOwnedRxChannel;
type Uart2OwnedReader = stm32_memory::UartOwnedReader<'static>;
type Uart2Discontinuities = stm32_memory::UartOwnedDiscontinuities<'static>;
type Uart2OwnedRxBridge = stm32_uart::UartOwnedRxBridge<
    'static,
    { stm32_memory::UART_RX_BUFFER_BYTES },
    { stm32_memory::OWNED_UART_RX_QUEUE_DEPTH },
>;
type Uart4OwnedTxChannel = stm32_memory::UartOwnedTxChannel;
type Uart4OwnedWriter = stm32_memory::UartOwnedWriter<'static>;
type Uart4OwnedTxOwner = stm32_memory::UartOwnedTxOwner<'static>;
type Uart4OwnedTxCompletion = stm32_memory::UartOwnedTxCompletion<'static>;
type UsbDebugSerial = SerialPort<'static, UsbBusType, [u8; 64], [u8; 256]>;

use board::Spi1ImuKind;
use board::profiles::{
    ADC_OBSERVATION_PROFILE, ARMING_INHIBIT_REASON, FLIGHT_ARMING_ENABLED, IMU_CONTROL_AXIS_PROFILE,
};
#[rtic::app(device = pac, peripherals = true, dispatchers = [CAN1_TX, CAN2_TX, CAN1_RX0, CAN1_RX1, CAN1_SCE, CAN2_RX0, CAN2_RX1, OTG_HS_EP1_OUT, OTG_HS_EP1_IN])]
mod app {
    use super::*; // Import everything from parent module

    // SAFETY CRITICAL SECTION
    //------------------------------------------------------------------------
    use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32};
    use embedded_hal::spi::Operation;
    use embedded_hal_async::spi::SpiDevice;
    use ferrowasp_core::safety::signals::{
        self, ActuatorArmPermitReader, ActuatorArmPermitWriter, RcRatesReader, RcRatesWriter,
    };

    static RC_ARM_HIGH: AtomicBool = AtomicBool::new(false);
    static RC_THROTTLE: AtomicU32 = AtomicU32::new(0);
    static SAFETY_ARMED: AtomicBool = AtomicBool::new(false);
    static IMU_STALE: AtomicBool = AtomicBool::new(false);
    static CONTROL_RATE_SEQ: AtomicU32 = AtomicU32::new(0);
    static CONTROL_ISR_SEQ: AtomicU32 = AtomicU32::new(0);
    static CONTROL_ROLL_RAW: AtomicI32 = AtomicI32::new(0);
    static CONTROL_PITCH_RAW: AtomicI32 = AtomicI32::new(0);
    static CONTROL_YAW_RAW: AtomicI32 = AtomicI32::new(0);
    static CONTROL_ROLL_DPS10: AtomicI32 = AtomicI32::new(0);
    static CONTROL_PITCH_DPS10: AtomicI32 = AtomicI32::new(0);
    static CONTROL_YAW_DPS10: AtomicI32 = AtomicI32::new(0);
    static IMU_LATEST_SEQ: AtomicU32 = AtomicU32::new(0);
    static IMU_LATEST_ROLL_RAW: AtomicI32 = AtomicI32::new(0);
    static IMU_LATEST_PITCH_RAW: AtomicI32 = AtomicI32::new(0);
    static IMU_LATEST_YAW_RAW: AtomicI32 = AtomicI32::new(0);
    static IMU_TRANSPORT_READY: AtomicBool = AtomicBool::new(false);
    static ACTIVE_IMU_KIND: AtomicU8 = AtomicU8::new(0);
    static BATTERY_VOLTAGE_V10_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
    static BATTERY_CURRENT_CA_SNAPSHOT: AtomicI32 = AtomicI32::new(0);
    static USB_DEBUG_DUE: AtomicBool = AtomicBool::new(false);
    static USB_RC_VALID_SNAPSHOT: AtomicBool = AtomicBool::new(false);
    static USB_RC_ARMABLE_SNAPSHOT: AtomicBool = AtomicBool::new(false);
    const USB_DEBUG_HEADER: &[u8] = b"FerroWasp Foxeer F405 V2 USB debug v1 (read-only)\r\n";
    type Spi1Mailbox = SharedSpiRequestMailbox<SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
    type Spi1Executor =
        CriticalSectionSpiExecutor<'static, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
    type Spi1Device = AsyncSpiDevice<Spi1Executor, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
    static SPI1_MAILBOX: Spi1Mailbox =
        critical_section::Mutex::new(core::cell::RefCell::new(SpiRequestMailbox::new()));
    static RC_RATES: Mutex<RefCell<safety::RcRates>> = Mutex::new(RefCell::new(safety::RcRates {
        roll: 0,
        pitch: 0,
        yaw: 0,
    }));
    static RC_LINK: Mutex<RefCell<safety::RcLinkState>> =
        Mutex::new(RefCell::new(safety::RcLinkState::new()));
    static ACTUATOR_ARM_DONE: AtomicBool = AtomicBool::new(false);
    static ACTUATOR_ARM_PERMIT: AtomicBool = AtomicBool::new(false);
    const ADC_VBAT_DIVIDER_RATIO: f32 = ADC_OBSERVATION_PROFILE.vbat_divider_ratio;
    const ADC_CURRENT_BETAFLIGHT_SCALE: u32 = ADC_OBSERVATION_PROFILE.current_betaflight_scale;
    const BATTERY_CELL_COUNT: u8 = ADC_OBSERVATION_PROFILE.battery_cell_count;
    #[cfg(any(
        feature = "bench_equal_motors",
        feature = "bench_motor1_only",
        feature = "bench_motor2_only",
        feature = "bench_motor3_only",
        feature = "bench_motor4_only",
        feature = "bench_logical_motor1_only",
        feature = "bench_logical_motor2_only",
        feature = "bench_logical_motor3_only",
        feature = "bench_logical_motor4_only"
    ))]
    const BENCH_EQUAL_MOTOR_MAX_THROTTLE: f32 = 250.0;
    const IMU_GYRO_RAW_TO_DPS: f32 = IMU_CONTROL_AXIS_PROFILE.gyro_raw_to_dps as f32 / 10.0;
    const CONTROL_IMU_TO_DRONE_ROTATION: dt::FrameRotation =
        IMU_CONTROL_AXIS_PROFILE.imu_to_drone_rotation();
    const GYRO_BIAS_CALIBRATION_SAMPLES: u32 = IMU_CONTROL_AXIS_PROFILE.bias_calibration_samples;
    const GYRO_BIAS_CALIBRATION_MAX_RAW: i32 = IMU_CONTROL_AXIS_PROFILE.bias_calibration_max_raw;
    //------------------------------------------------------------------------

    // Monotonicss
    systick_monotonic!(Mono, 1000); // Set mono timer to 1ms resolution

    #[cfg(feature = "blackbox_defmt")]
    macro_rules! emit_compact_blackbox {
        (
            $seq:expr,
            $imu_seq:expr,
            $armed:expr,
            $imu_fresh:expr,
            $raw_gyro_dps:expr,
            $filtered_gyro_dps:expr,
            $command_dps:expr,
            $pid:expr,
            $throttle:expr,
            $motors:expr $(,)?
        ) => {
            dt::emit_rate_blackbox(dt::CompactRateBlackboxSample::from_fields(
                dt::CompactRateBlackboxFields {
                    seq: $seq,
                    imu_seq: $imu_seq,
                    armed: $armed,
                    imu_fresh: $imu_fresh,
                    raw_gyro_dps: $raw_gyro_dps,
                    filtered_gyro_dps: $filtered_gyro_dps,
                    command_dps: $command_dps,
                    pid: $pid,
                    throttle: $throttle,
                    motors: $motors,
                },
            ));
        };
    }

    #[shared]
    struct Shared {
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
    }
    #[local]
    struct Local {
        // Safety
        arm_qualifier: safety::ArmQualifier,

        // ESC PWM Control
        motors: board::pwm::EscPwmBank,

        // UART
        sbus: StreamingParser,

        // SPI1
        spi1_parser: SpiRxParserSide,
        spi1_device: Spi1Device,

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

        calibrated: bool,

        // USB CDC serial
        usb_dev: Option<UsbDevice<'static, UsbBusType>>,
        usb_serial: Option<UsbDebugSerial>,
        usb_header_sent: bool,
    }
    #[init(local = [
        uart2_rx_buffers: board::storage::UartRxBufferBank =
            board::storage::new_uart_rx_buffer_bank(),
        uart2_free_queue: board::storage::UartRxFreeQueue =
            board::storage::UartRxFreeQueue::new(),
        uart2_filled_queue: board::storage::UartRxFilledQueue =
            board::storage::UartRxFilledQueue::new(),
        uart4_rx_buffers: board::storage::UartRxBufferBank =
            board::storage::new_uart_rx_buffer_bank(),
        uart4_free_queue: board::storage::UartRxFreeQueue =
            board::storage::UartRxFreeQueue::new(),
        uart4_filled_queue: board::storage::UartRxFilledQueue =
            board::storage::UartRxFilledQueue::new(),
        uart4_tx_buffer: board::storage::Uart4TxBuffer = [0; mspv1::OSD_TX_BUFFER_LEN],
        spi1_dma_buffers: board::storage::SpiDmaBufferBank =
            board::storage::new_spi_dma_buffer_bank(),
        spi1_free_queue: board::storage::SpiFreeQueue =
            board::storage::SpiFreeQueue::new(),
        spi1_filled_queue: board::storage::SpiFilledQueue =
            board::storage::SpiFilledQueue::new(),
        adc1_buffers: board::storage::AdcBufferBank =
            board::storage::new_adc_buffer_bank(),
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

        // Poll the IMU at 800 Hz and run the PID/motor update at 400 Hz.
        let sampling_rate = dt::IMU_POLL_RATE_HZ.Hz();
        let control_loop_rate: Rate<u32, 1, 1> = dt::CONTROL_LOOP_RATE_HZ.Hz();
        let samples_per_control_loop = sampling_rate.to_Hz() / control_loop_rate.to_Hz();

        let adc1_battery = board::init::init_adc1_battery(
            board::init::Adc1BatteryResourcesFoxeer {
                adc: dp.ADC1,
                voltage_pin: gpioc.pc0,
                current_pin: gpioc.pc1,
                dma: dma2.4,
            },
            &mut rcc,
            board::storage::AdcStorageResources {
                buffers: cx.local.adc1_buffers,
            },
        );

        // Configure Clocks and start monotimer.
        let system_clock_frequency: Rate<u32, 1, 1> = board::SYSTEM_CLOCK_HZ.Hz();
        let hse_frequency: Rate<u32, 1, 1> = board::HSE_FREQUENCY_HZ.Hz();
        const DELAY_HZ: u32 = 1_000_000;
        #[cfg(feature = "usb_serial")]
        let mut clocks = rcc.freeze(
            rcc_cfg::hse(hse_frequency)
                .sysclk(system_clock_frequency)
                .require_pll48clk(),
        );
        #[cfg(not(feature = "usb_serial"))]
        let mut clocks = rcc.freeze(rcc_cfg::hse(hse_frequency).sysclk(system_clock_frequency));
        #[cfg(feature = "usb_serial")]
        info!(
            "PLL48 valid: {}, PLL48: {} Hz",
            clocks.clocks.is_pll48clk_valid(),
            clocks.clocks.pll48clk().map(|clk| clk.raw()).unwrap_or(0)
        );
        Mono::start(cx.core.SYST, system_clock_frequency.to_Hz());
        let mut delay = dp.TIM5.delay::<DELAY_HZ>(&mut clocks);
        //let mut syscfg = dp.SYSCFG.constrain(&mut clocks);

        let control_loop_scheduler =
            stm32_scheduler::init_control_scheduler(dp.TIM4, &mut clocks, sampling_rate).unwrap();
        let io_timebase = stm32_timebase::MicrosecondTimebase::new(dp.TIM2, &mut clocks).unwrap();
        let io_watchdog = stm32_watchdog::init_io_watchdog(dp.TIM6, &mut clocks).unwrap();

        #[cfg(feature = "usb_serial")]
        let (usb_dev, usb_serial) = {
            let usb = USB::new(
                (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
                (gpioa.pa11, gpioa.pa12),
                &clocks.clocks,
            );
            let usb_bus = cortex_m::singleton!(
                : UsbBusAllocator<UsbBusType> = UsbBusType::new(
                    usb,
                    cortex_m::singleton!(: [u32; 1024] = [0; 1024]).unwrap()
                )
            )
            .unwrap();
            let usb_serial = SerialPort::new_with_store(usb_bus, [0; 64], [0; 256]);
            let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x16c0, 0x27dd))
                .strings(&[StringDescriptors::default()
                    .manufacturer("FerroWasp")
                    .product("FerroWasp Foxeer Debug")
                    .serial_number("FW-FOX-F405V2")])
                .unwrap()
                .device_class(usbd_serial::USB_CLASS_CDC)
                .build();

            (Some(usb_dev), Some(usb_serial))
        };
        #[cfg(not(feature = "usb_serial"))]
        let (usb_dev, usb_serial) = (None, None);

        // Set-up Routing
        let mut rc_input_uart = None;
        let mut osd_uart = None;
        let mut tele_uart = None;
        let mut gps_uart = None;

        // ------------  USART2 / SBUS RC  ------------
        let uart2 = board::init::init_usart2_sbus(
            board::init::Usart2SbusResources {
                tx_pin: gpioa.pa2,
                rx_pin: gpioa.pa3,
                usart: dp.USART2,
                rx_dma: dma1.5,
            },
            &mut clocks,
            board::storage::UartRxStorageResources {
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
        let uart4 = board::init::init_uart4_msp_osd(
            board::init::Uart4MspResources {
                tx_pin: gpioa.pa0,
                rx_pin: gpioa.pa1,
                uart: dp.UART4,
                rx_dma: dma1.2,
                tx_dma: dma1.4,
            },
            &mut clocks,
            board::storage::UartRxStorageResources {
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
            .expect("BSP must route USART2 to the RC input task");
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

        let motors = board::pwm::init_esc_pwm(
            board::pwm::EscPwmResources {
                tim1: dp.TIM1,
                tim8: dp.TIM8,
                motor1_pin: gpioa.pa8,
                motor2_pin: gpioc.pc9,
                motor3_pin: gpioc.pc8,
                motor4_pin: gpiob.pb15,
            },
            &mut clocks,
        )
        .expect("Foxeer TIM1/TIM8 RC PWM configuration must be valid");

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
            board::storage::SpiDmaStorageResources {
                buffers: cx.local.spi1_dma_buffers,
                free_queue: cx.local.spi1_free_queue,
                filled_queue: cx.local.spi1_filled_queue,
            },
        );
        let spi1_device = AsyncSpiDevice::new(CriticalSectionSpiExecutor::new(
            &SPI1_MAILBOX,
            SpiDeadlineUs(SPI1_IMU_DEADLINE_US),
            pend_spi1_owner,
        ));
        if let Some(kind) = spi1_imu.bringup.kind() {
            ACTIVE_IMU_KIND.store(kind as u8, Ordering::Relaxed);
        }
        IMU_TRANSPORT_READY.store(spi1_imu.bringup.is_ready(), Ordering::Relaxed);

        // Init rate controller
        let tuning_profile = dt::TuningProfile::default_first_hop();
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
        if !FLIGHT_ARMING_ENABLED {
            warn!("Flight arming inhibited: {}", ARMING_INHIBIT_REASON);
        }
        heartbeat::spawn().unwrap();
        adc1_polling::spawn().ok();
        uart4_tx_worker::spawn().unwrap();
        rc_input::spawn().unwrap();
        osd_refresh::spawn().ok();
        #[cfg(feature = "pwm_cal")]
        actuator_output::spawn(safety::ActuatorCmd::Calibrate).ok();

        (
            Shared {
                uart2_rx: uart2.irq,
                uart2_bridge,
                uart4_rx: uart4.rx_irq,
                uart4_tx_dma: osd_tx_dma,

                // SPI1
                spi1_owner: spi1_imu.owner,
                io_timebase,

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

                // ESC PWM Control
                motors,

                // UART
                sbus: StreamingParser::new(),

                // SPI1
                spi1_parser: spi1_imu.parser,
                spi1_device,

                // ADC
                adc1_buffer: Some(adc1_battery.spare_buffer),

                // Control Loop
                control_loop_cnt: 0,
                samples_per_control_loop,
                flight_controller,
                imu_rate_filter: dt::ImuRateLowPassFilter::new(dt::IMU_GYRO_LPF_ALPHA),
                imu_angle_integrator: dt::GyroAngleIntegrator::new(),
                gyro_axis_map: CONTROL_IMU_TO_DRONE_ROTATION,
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

                calibrated: false,

                // USB CDC serial
                usb_dev,
                usb_serial,
                usb_header_sent: false,
            },
        )
    }

    fn warn_arming_abort(reason: safety::ArmingAbortReason) {
        match reason {
            safety::ArmingAbortReason::PermitRevoked => {
                warn!("Arming aborted: actuator permission revoked")
            }
            safety::ArmingAbortReason::RcLinkInvalid => {
                warn!("Arming aborted: RC link is not armable")
            }
            safety::ArmingAbortReason::ArmSwitchLow => {
                warn!("Arming aborted: arm switch is low")
            }
            safety::ArmingAbortReason::ThrottleHigh => warn!(
                "Arming aborted: throttle exceeds {}",
                safety::ARMING_MAX_THROTTLE
            ),
            safety::ArmingAbortReason::EscIdleTelemetryTimeout => {
                warn!("Arming aborted: ESC idle telemetry qualification timed out")
            }
            safety::ArmingAbortReason::EscIdleRpmOutOfRange => {
                warn!("Arming aborted: ESC idle eRPM outside the permitted range")
            }
            safety::ArmingAbortReason::EscIdleQualificationInvalid => {
                warn!("Arming aborted: invalid ESC idle qualification profile")
            }
            safety::ArmingAbortReason::CompletionDeliveryFailed => {
                warn!("Arming aborted: idle completion delivery failed")
            }
        }
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

                if !FLIGHT_ARMING_ENABLED {
                    arm_permit.revoke();
                    warn!("Arming inhibited: {}", ARMING_INHIBIT_REASON);
                    return;
                }

                let guard = safety::validate_arming_guard(
                    true,
                    rc_link.is_armable(now_us),
                    rc_arm_high.read(),
                    rc_throttle.read(),
                );
                if guard.is_ok() {
                    arm_permit.allow();
                    info!("Attempting BLHeli PWM arming!");

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
                if FLIGHT_ARMING_ENABLED
                    && safety::validate_arming_guard(
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
                    info!("SYSTEM ARMED");
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

    async fn osd_write(writer: &mut Uart4OwnedWriter, healthy: &mut bool, bytes: &[u8]) {
        use embedded_io_async::Write;

        if !*healthy {
            return;
        }

        if let Err(error) = writer.write_all(bytes).await {
            *healthy = false;
            match error {
                SerialFault::DmaTransfer => warn!("UART4 TX writer stopped after DMA fault"),
                SerialFault::Disabled => {
                    warn!("UART4 TX writer stopped because stream is disabled")
                }
                SerialFault::InvalidChunk => warn!("UART4 TX writer rejected invalid MSP frame"),
                SerialFault::InvalidState => warn!("UART4 TX writer found invalid transport state"),
                SerialFault::QueueOverflow => warn!("UART4 TX writer queue overflowed"),
                SerialFault::Timeout => warn!("UART4 TX writer timed out"),
                SerialFault::UnsupportedProtocol => {
                    warn!("UART4 TX writer rejected unsupported protocol")
                }
            }
        }
    }

    fn active_usb_debug_imu_kind() -> usb_debug::ImuKind {
        match Spi1ImuKind::from_discriminant(ACTIVE_IMU_KIND.load(Ordering::Relaxed)) {
            Some(Spi1ImuKind::Mpu6500) => usb_debug::ImuKind::Mpu6500,
            Some(Spi1ImuKind::Icm42688P) => usb_debug::ImuKind::Icm42688P,
            None => usb_debug::ImuKind::None,
        }
    }

    // ---- USB CDC READ-ONLY DEBUG ----
    #[task(
        binds = OTG_FS,
        priority = 5,
        local = [usb_dev, usb_serial, usb_header_sent]
    )]
    fn usb_fs(cx: usb_fs::Context) {
        let Some(usb_dev) = cx.local.usb_dev.as_mut() else {
            return;
        };
        let Some(serial) = cx.local.usb_serial.as_mut() else {
            return;
        };

        let _ = usb_dev.poll(&mut [serial]);

        // USB diagnostics are deliberately read-only. Draining one bounded
        // packet prevents endpoint backpressure without creating a command path.
        let mut rx_buf = [0u8; 64];
        let _ = serial.read(&mut rx_buf);

        if usb_dev.state() != UsbDeviceState::Configured {
            *cx.local.usb_header_sent = false;
            return;
        }

        if serial.flush().is_err() {
            return;
        }

        if !*cx.local.usb_header_sent {
            if matches!(
                serial.write(USB_DEBUG_HEADER),
                Ok(written) if written == USB_DEBUG_HEADER.len()
            ) {
                *cx.local.usb_header_sent = true;
            }
            return;
        }

        if !USB_DEBUG_DUE.swap(false, Ordering::AcqRel) {
            return;
        }

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
        };
        let Ok(line) = usb_debug::format_status(snapshot) else {
            USB_DEBUG_DUE.store(true, Ordering::Release);
            return;
        };

        if !matches!(
            serial.write(line.as_bytes()),
            Ok(written) if written == line.len()
        ) {
            USB_DEBUG_DUE.store(true, Ordering::Release);
        }
    }

    // IDLE TASK

    #[task(priority = 1, local = [usb_rc_link_reader])]
    async fn heartbeat(cx: heartbeat::Context) {
        info!("Running heartbeat!");

        loop {
            let now_us = Mono::now().duration_since_epoch().to_micros();
            let rc_link = cx.local.usb_rc_link_reader.status(now_us);
            USB_RC_VALID_SNAPSHOT.store(rc_link.valid, Ordering::Relaxed);
            USB_RC_ARMABLE_SNAPSHOT.store(rc_link.armable, Ordering::Relaxed);
            #[cfg(feature = "usb_serial")]
            {
                USB_DEBUG_DUE.store(true, Ordering::Release);
                cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
            }

            info!(
                "IMU raw gyro [{}, {}, {}], seq {}",
                IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed),
                IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed),
                IMU_LATEST_YAW_RAW.load(Ordering::Relaxed),
                IMU_LATEST_SEQ.load(Ordering::Relaxed)
            );
            Mono::delay(2000.millis()).await;
        }
    }

    #[cfg(not(feature = "pwm_cal"))]
    fn motor_command_timestamp(now_ms: u32, _sequence: u32) -> u32 {
        #[cfg(feature = "bench_motor_cmd_stale_rejection")]
        if _sequence == 1 {
            return now_ms.wrapping_sub(safety::MOTOR_CMD_MAX_AGE_MS + 1);
        }

        now_ms
    }

    #[cfg(not(feature = "pwm_cal"))]
    fn publish_motor_command(
        writer: &mut safety::signals::MotorCmdWriter,
        sequence: &mut u32,
        motors: [f32; 4],
        wake: safety::ActuatorCmd,
    ) {
        let next_sequence = sequence.wrapping_add(1);
        let now_ms = Mono::now().duration_since_epoch().to_millis();
        let command = safety::MotorCmd {
            motors,
            seq: next_sequence,
            issued_at_ms: motor_command_timestamp(now_ms, next_sequence),
        };

        if writer.enqueue(command).is_err() {
            warn!("Motor command queue full; requesting disarm");
            if safety_master::spawn(safety::SafetyEvent::DisarmRequested).is_err() {
                warn!("Failed to report motor command queue overflow");
            }
            return;
        }

        *sequence = next_sequence;
        if actuator_output::spawn(wake).is_err() {
            warn!("Actuator command wake rejected; requesting disarm");
            if safety_master::spawn(safety::SafetyEvent::DisarmRequested).is_err() {
                warn!("Failed to report rejected actuator command wake");
            }
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

        // Incremet Counter
        *cnt += 1;
        CONTROL_ISR_SEQ.fetch_add(1, Ordering::Relaxed);

        if !IMU_TRANSPORT_READY.load(Ordering::Relaxed) {
            *cnt = 0;
            IMU_STALE.store(true, Ordering::Relaxed);
            return;
        }

        // ---- SAMPLE IMU ----
        let _ = spi1_poll::spawn();

        // ---- CONTROL LOOP ----
        // Check if required samples per control loop is reached
        if cnt >= samples_per_control_loop {
            *cnt = 0; // Reset sampling counter

            let (acc_x, acc_y, acc_z) = cx
                .shared
                .imu_data
                .lock(|imu| (imu.acc[0], imu.acc[1], imu.acc[2]));
            let gyro_raw = [
                IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed) as i16,
                IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed) as i16,
                IMU_LATEST_YAW_RAW.load(Ordering::Relaxed) as i16,
            ];
            let imu_sequence = IMU_LATEST_SEQ.load(Ordering::Relaxed);

            let control_armed = cx.local.control_safety_arm_reader.read();
            let gyro_bias_update = cx
                .local
                .gyro_bias_calibrator
                .update(control_armed, cx.local.gyro_axis_map.map_raw(gyro_raw));
            if gyro_bias_update.newly_calibrated {
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
            let imu_fresh =
                imu::classify_sample_freshness(*cx.local.imu_last_sequence, imu_sequence)
                    == imu::SampleFreshness::Fresh;
            *cx.local.imu_last_sequence = imu_sequence;

            let (imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered) = cx
                .local
                .imu_rate_filter
                .update(imu_roll_raw, imu_pitch_raw, imu_yaw_raw);
            CONTROL_ROLL_DPS10.store((imu_roll_filtered * 10.0) as i32, Ordering::Relaxed);
            CONTROL_PITCH_DPS10.store((imu_pitch_filtered * 10.0) as i32, Ordering::Relaxed);
            CONTROL_YAW_DPS10.store((imu_yaw_filtered * 10.0) as i32, Ordering::Relaxed);
            CONTROL_RATE_SEQ.fetch_add(1, Ordering::Relaxed);
            cx.shared.imu_rates.lock(|rates| {
                *rates = [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered];
            });
            if !control_armed {
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
                    emit_compact_blackbox!(
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

                IMU_STALE.store(true, Ordering::Relaxed);
                return;
            }

            *cx.local.imu_stale_ticks = 0;
            IMU_STALE.store(false, Ordering::Relaxed);

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
                [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                [acc_x, acc_y, acc_z],
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
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    let motor_commands =
                        board::profiles::remap_motor_outputs([bench_throttle, 0.0, 0.0, 0.0]);

                    #[cfg(feature = "blackbox_defmt")]
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    let motor_commands =
                        board::profiles::remap_motor_outputs([0.0, bench_throttle, 0.0, 0.0]);

                    #[cfg(feature = "blackbox_defmt")]
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    let motor_commands =
                        board::profiles::remap_motor_outputs([0.0, 0.0, bench_throttle, 0.0]);

                    #[cfg(feature = "blackbox_defmt")]
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    let motor_commands =
                        board::profiles::remap_motor_outputs([0.0, 0.0, 0.0, bench_throttle]);

                    #[cfg(feature = "blackbox_defmt")]
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    emit_compact_blackbox!(
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

                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        publish_motor_command(
                            cx.local.motor_cmd_writer,
                            cx.local.motor_cmd_seq,
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
                    #[cfg(any(feature = "blackbox_defmt", not(feature = "pwm_cal")))]
                    fc.update_motor_commands();
                    #[cfg(not(feature = "pwm_cal"))]
                    let motor_commands =
                        board::profiles::remap_motor_outputs(fc.get_logical_motor_commands());

                    #[cfg(feature = "blackbox_defmt")]
                    {
                        let mut sample = fc.blackbox_sample();
                        sample.motors =
                            board::profiles::remap_motor_outputs(fc.get_logical_motor_commands());
                        dt::emit_rate_blackbox(dt::CompactRateBlackboxSample::from_rate_sample(
                            CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                            imu_sequence,
                            control_armed,
                            imu_fresh,
                            [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                            sample,
                        ));
                    }

                    // Apply throttle
                    #[cfg(not(any(feature = "pwm_cal")))]
                    {
                        if control_armed {
                            publish_motor_command(
                                cx.local.motor_cmd_writer,
                                cx.local.motor_cmd_seq,
                                motor_commands,
                                safety::ActuatorCmd::ApplyLatestThrottle,
                            );
                        }
                    }
                }
            }
        }
    }

    const ARMING_GUARD_POLL_MS: u32 = 10;

    fn current_arming_guard(
        permit: &ActuatorArmPermitReader,
        rc_link: &signals::RcLinkReader,
        arm_high: &signals::RcArmHighReader,
        throttle: &signals::RcThrottleReader,
    ) -> Result<(), safety::ArmingAbortReason> {
        let now_us = Mono::now().duration_since_epoch().to_micros();
        safety::validate_arming_guard(
            permit.read(),
            rc_link.is_armable(now_us),
            arm_high.read(),
            throttle.read(),
        )
    }

    async fn wait_arming_hold(
        permit: &ActuatorArmPermitReader,
        rc_link: &signals::RcLinkReader,
        arm_high: &signals::RcArmHighReader,
        throttle: &signals::RcThrottleReader,
        hold_ms: u32,
    ) -> Result<(), safety::ArmingAbortReason> {
        let mut remaining_ms = hold_ms;

        while remaining_ms != 0 {
            current_arming_guard(permit, rc_link, arm_high, throttle)?;
            let delay_ms = remaining_ms.min(ARMING_GUARD_POLL_MS);
            Mono::delay(delay_ms.millis()).await;
            remaining_ms -= delay_ms;
        }

        current_arming_guard(permit, rc_link, arm_high, throttle)
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

    fn take_fresh_motor_outputs(reader: &mut safety::signals::MotorCmdReader) -> Option<[f32; 4]> {
        let now_ms = Mono::now().duration_since_epoch().to_millis();

        match reader.take_latest_fresh(now_ms, safety::MOTOR_CMD_MAX_AGE_MS) {
            Ok(command) => Some(command.motors),
            Err(safety::MotorCmdReadError::Missing) => {
                warn!("Actuator command refused: motor command queue empty");
                None
            }
            Err(safety::MotorCmdReadError::Stale { seq, age_ms }) => {
                warn!(
                    "Actuator command refused: stale motor command seq {}, age {} ms",
                    seq, age_ms
                );
                None
            }
        }
    }

    #[task(
    priority = 15,
    local = [
        motors,
        actuator_safety_arm_reader,
        actuator_arm_permit_reader,
        actuator_rc_arm_high_reader,
        actuator_rc_throttle_reader,
        actuator_rc_link_reader,
        actuator_arm_done_writer,
        motor_cmd_reader,
        calibrated,

    ]
    )]
    async fn actuator_output(cx: actuator_output::Context, cmd: safety::ActuatorCmd) {
        if !FLIGHT_ARMING_ENABLED {
            cx.local.motors.force_fully_off();
            if !matches!(cmd, safety::ActuatorCmd::Disarm) {
                warn!("Actuator command inhibited: {}", ARMING_INHIBIT_REASON);
            }
            return;
        }

        macro_rules! apply_all {
            ($values:expr) => {{
                let values = $values;
                let commands = [
                    throttle_to_u16(values[0]),
                    throttle_to_u16(values[1]),
                    throttle_to_u16(values[2]),
                    throttle_to_u16(values[3]),
                ];
                if cx.local.motors.set_throttles(commands).is_err() {
                    warn!("Motor output batch rejected");
                    cx.local.motors.force_fully_off();
                    return;
                }
            }};
        }

        #[cfg(feature = "pwm_cal")]
        macro_rules! selected_motor_pulse_width_us {
            () => {
                match safety::PWM_CAL_MOTOR {
                    1..=4 => cx.local.motors.last_pulse_width_us(safety::PWM_CAL_MOTOR),
                    _ => None,
                }
            };
        }

        #[cfg(feature = "pwm_cal")]
        fn selected_cal_motor_outputs(throttle: f32) -> [f32; 4] {
            let mut outputs = [safety::ESC_LOW_THROTTLE; 4];

            if safety::PWM_CAL_MOTOR >= 1 && safety::PWM_CAL_MOTOR <= 4 {
                outputs[safety::PWM_CAL_MOTOR - 1] = throttle;
            }

            outputs
        }

        let safety_armed = cx.local.actuator_safety_arm_reader.read();
        let output = match cmd {
            safety::ActuatorCmd::Disarm => {
                cx.local.motor_cmd_reader.discard_all();
                cx.local.actuator_arm_done_writer.clear();
                cx.local.motors.force_fully_off();
                return;
            }

            safety::ActuatorCmd::EnterIdle => {
                cx.local.motor_cmd_reader.discard_all();
                if let Err(reason) = current_arming_guard(
                    cx.local.actuator_arm_permit_reader,
                    cx.local.actuator_rc_link_reader,
                    cx.local.actuator_rc_arm_high_reader,
                    cx.local.actuator_rc_throttle_reader,
                ) {
                    cx.local.actuator_arm_done_writer.clear();
                    cx.local.motors.force_fully_off();
                    if safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason)).is_err() {
                        warn!("Failed to report rejected BLHeli arming sequence");
                    }
                    return;
                }

                info!("Arming BLHeli ESCs with PWM low throttle");
                cx.local.actuator_arm_done_writer.clear();

                info!("Applying low throttle");
                apply_all!([safety::ESC_LOW_THROTTLE; 4]);
                if let Err(reason) = wait_arming_hold(
                    cx.local.actuator_arm_permit_reader,
                    cx.local.actuator_rc_link_reader,
                    cx.local.actuator_rc_arm_high_reader,
                    cx.local.actuator_rc_throttle_reader,
                    safety::BLHELI_ARM_LOW_HOLD_MS,
                )
                .await
                {
                    cx.local.actuator_arm_done_writer.clear();
                    cx.local.motors.force_fully_off();
                    if safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason)).is_err() {
                        warn!("Failed to report aborted BLHeli low-throttle hold");
                    }
                    return;
                }

                info!("Applying idle throttle");
                apply_all!([safety::ESC_IDLE_THROTTLE; 4]);
                if let Err(reason) = wait_arming_hold(
                    cx.local.actuator_arm_permit_reader,
                    cx.local.actuator_rc_link_reader,
                    cx.local.actuator_rc_arm_high_reader,
                    cx.local.actuator_rc_throttle_reader,
                    safety::BLHELI_ARM_IDLE_HOLD_MS,
                )
                .await
                {
                    cx.local.actuator_arm_done_writer.clear();
                    cx.local.motors.force_fully_off();
                    if safety_master::spawn(safety::SafetyEvent::ArmingAborted(reason)).is_err() {
                        warn!("Failed to report aborted BLHeli idle hold");
                    }
                    return;
                }

                info!("BLHeli ESCs idling");
                cx.local.actuator_arm_done_writer.set_done();

                if actuator_idle_notify::spawn().is_err() {
                    cx.local.actuator_arm_done_writer.clear();
                    cx.local.motors.force_fully_off();
                    if safety_master::spawn(safety::SafetyEvent::ArmingAborted(
                        safety::ArmingAbortReason::CompletionDeliveryFailed,
                    ))
                    .is_err()
                    {
                        warn!("Failed to report BLHeli completion delivery failure");
                    }
                    return;
                }

                [safety::ESC_IDLE_THROTTLE; 4]
            }

            safety::ActuatorCmd::ApplyLatestThrottle if safety_armed => {
                match take_fresh_motor_outputs(cx.local.motor_cmd_reader) {
                    Some(throttles) => match safety::validate_active_motor_outputs(throttles) {
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

                if let Some(throttles) = take_fresh_motor_outputs(cx.local.motor_cmd_reader) {
                    for index in 0..4 {
                        if !throttles[index].is_finite() {
                            warn!("Bench selected motor command refused: invalid motor output");
                            outputs = [safety::ESC_LOW_THROTTLE; 4];
                            break;
                        }

                        if throttles[index] > 0.0 {
                            outputs[index] = throttles[index]
                                .clamp(safety::ESC_IDLE_THROTTLE, safety::ESC_MAX_THROTTLE);
                        }
                    }
                }

                outputs
            }

            #[cfg(feature = "pwm_cal")]
            safety::ActuatorCmd::Calibrate => {
                if !*cx.local.calibrated {
                    info!("PWM ESC calibration mode");
                    info!("PROPS OFF. Keep ESC battery disconnected.");
                    cx.local.actuator_arm_done_writer.clear();

                    if safety::PWM_CAL_MOTOR < 1 || safety::PWM_CAL_MOTOR > 4 {
                        warn!(
                            "Invalid PWM_CAL_MOTOR: {}. Use 1, 2, 3, or 4.",
                            safety::PWM_CAL_MOTOR
                        );
                        return apply_all!([safety::ESC_LOW_THROTTLE; 4]);
                    }

                    info!("Calibration target: motor {}", safety::PWM_CAL_MOTOR);
                    info!(
                        "Only motor {} will receive MAX throttle. Other motors stay at MIN.",
                        safety::PWM_CAL_MOTOR
                    );
                    info!("Calibration: setting selected motor to MAX throttle now");
                    apply_all!(selected_cal_motor_outputs(safety::ESC_MAX_THROTTLE));
                    if let Some(pulse_width_us) = selected_motor_pulse_width_us!() {
                        info!(
                            "Motor {} MAX pulse width: {} us",
                            safety::PWM_CAL_MOTOR,
                            pulse_width_us
                        );
                    }
                    info!(
                        "PLUG IN ESC BATTERY FOR MOTOR {} NOW. Waiting for ESC calibration tones.",
                        safety::PWM_CAL_MOTOR
                    );
                    let mut max_hold_remaining_s = safety::PWM_CAL_MAX_HOLD_MS / 1_000;
                    while max_hold_remaining_s > 0 {
                        info!("MAX throttle hold: {}s remaining", max_hold_remaining_s);
                        Mono::delay(1000.millis()).await;
                        max_hold_remaining_s -= 1;
                    }

                    info!("Calibration: switching selected motor to MIN throttle now");
                    apply_all!([safety::ESC_LOW_THROTTLE; 4]);
                    if let Some(pulse_width_us) = selected_motor_pulse_width_us!() {
                        info!(
                            "Motor {} MIN pulse width: {} us",
                            safety::PWM_CAL_MOTOR,
                            pulse_width_us
                        );
                    }
                    info!(
                        "Keep ESC battery connected. Waiting for low-throttle confirmation tones."
                    );
                    Mono::delay(safety::PWM_CAL_LOW_HOLD_MS.millis()).await;

                    info!("PWM ESC calibration complete. Outputs are held at MIN throttle.");
                    info!("Disconnect ESC battery, then reboot without the pwm_cal feature.");
                    *cx.local.calibrated = true;
                }

                [safety::ESC_LOW_THROTTLE; 4]
            }

            _ => {
                warn!("Actuator command refused");
                cx.local.motors.force_fully_off();
                return;
            }
        };

        apply_all!(output);
    }

    // ########### SPI 1 ###################################
    #[task(
    priority = 12,
    shared = [io_timebase],
    local = [spi1_device, unavailable_logged: bool = false]
    )]
    async fn spi1_poll(mut cx: spi1_poll::Context) {
        let Some(kind) = Spi1ImuKind::from_discriminant(ACTIVE_IMU_KIND.load(Ordering::Relaxed))
        else {
            return;
        };
        let request = kind.dma_burst_register();
        let now = cx.shared.io_timebase.lock(|timebase| timebase.now());
        cx.local.spi1_device.executor_mut().set_start(now);

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

    fn pend_spi1_owner() {
        let _ = spi1_owner_service::spawn();
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

    struct ParsedImuSample {
        acc: [f32; 3],
        gyro: [f32; 3],
        gyro_raw: [i16; 3],
        temp: f32,
    }

    fn record_uart2_discontinuity(
        bridge: &mut Uart2OwnedRxBridge,
        cause: Discontinuity,
        generation: ferrowasp_io_core::serial::StreamGeneration,
        reason: safety::RcLinkInvalidation,
    ) {
        let timestamp = TimestampMicros(Mono::now().duration_since_epoch().to_micros() as u64);
        bridge.record_discontinuity(cause, generation, timestamp);
        let _ = safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(reason));
    }

    fn publish_uart2_owned(
        bridge: &mut Uart2OwnedRxBridge,
        generation: ferrowasp_io_core::serial::StreamGeneration,
    ) {
        let timestamp = TimestampMicros(Mono::now().duration_since_epoch().to_micros() as u64);
        match bridge.publish_next(timestamp) {
            stm32_uart::UartOwnedRxBridgeOutcome::Published => {}
            stm32_uart::UartOwnedRxBridgeOutcome::NoChunk => {
                warn!("USART2 delivered IRQ had no detached RX chunk");
                record_uart2_discontinuity(
                    bridge,
                    Discontinuity::TransportReset,
                    generation,
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                );
            }
            stm32_uart::UartOwnedRxBridgeOutcome::InvalidChunk => {
                warn!("USART2 produced an invalid owned RX chunk");
                record_uart2_discontinuity(
                    bridge,
                    Discontinuity::FramingError,
                    generation,
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                );
            }
            stm32_uart::UartOwnedRxBridgeOutcome::QueueOverflow => {
                warn!("USART2 owned RX queue overflowed");
                // The portable producer records QueueOverflow before returning.
                let _ = safety_master::spawn(safety::SafetyEvent::RcLinkInvalid(
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                ));
            }
            stm32_uart::UartOwnedRxBridgeOutcome::Disabled => {
                warn!("USART2 owned RX channel is disabled");
                record_uart2_discontinuity(
                    bridge,
                    Discontinuity::TransportReset,
                    generation,
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                );
            }
            stm32_uart::UartOwnedRxBridgeOutcome::RecycleFailed => {
                panic!("USART2 detached DMA buffer could not be recycled");
            }
        }
    }

    fn record_uart2_dma_error(
        bridge: &mut Uart2OwnedRxBridge,
        generation: ferrowasp_io_core::serial::StreamGeneration,
    ) {
        record_uart2_discontinuity(
            bridge,
            Discontinuity::DmaError,
            generation,
            safety::RcLinkInvalidation::DmaError,
        );
    }

    // ########### UART 2 ###################################
    #[task(
        binds = DMA1_STREAM5,
        priority = 11,
        shared = [uart2_rx, uart2_bridge]
    )]
    fn usart2_rx_dma_transfer(mut cx: usart2_rx_dma_transfer::Context) {
        let uart = cx.shared.uart2_rx;
        let delivered = match uart.service_dma_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered => true,
            stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
            stm32_uart::UartRxIrqOutcome::DmaError => {
                warn!("USART2 RX DMA error");
                cx.shared
                    .uart2_bridge
                    .lock(|bridge| record_uart2_dma_error(bridge, uart.rx_generation()));
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
                        safety::RcLinkInvalidation::TransportDiscontinuity,
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
                        safety::RcLinkInvalidation::TransportDiscontinuity,
                    )
                });
                false
            }
        };

        if delivered {
            let generation = uart.rx_generation();
            cx.shared
                .uart2_bridge
                .lock(|bridge| publish_uart2_owned(bridge, generation));
        }
    }

    #[task(
        binds = USART2,
        priority = 11,
        shared = [uart2_rx, uart2_bridge]
    )]
    fn usart2_rx_peripheral(mut cx: usart2_rx_peripheral::Context) {
        let uart = cx.shared.uart2_rx;

        let delivered = match uart.service_idle_irq() {
            stm32_uart::UartRxIrqOutcome::Delivered => true,
            stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
            stm32_uart::UartRxIrqOutcome::DmaError => {
                cx.shared
                    .uart2_bridge
                    .lock(|bridge| record_uart2_dma_error(bridge, uart.rx_generation()));
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
                        safety::RcLinkInvalidation::TransportDiscontinuity,
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
                        safety::RcLinkInvalidation::TransportDiscontinuity,
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
                .lock(|bridge| publish_uart2_owned(bridge, generation));
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
        use embedded_io_async::Read;

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

    fn neutralize_rc_input(
        arm_qualifier: &mut safety::ArmQualifier,
        rates: &signals::RcRatesWriter,
        throttle: &signals::RcThrottleWriter,
        arm_high: &signals::RcArmHighWriter,
    ) {
        arm_qualifier.reset();
        rates.write(safety::RcRates::default());
        throttle.write(0);
        arm_high.write(false);
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
        ]
    )]
    async fn rc_input(cx: rc_input::Context) {
        use embedded_io_async::Read;

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

                let rc_cmd = dt::remap_rc_channels(
                    pkt.channels[0],
                    pkt.channels[1],
                    pkt.channels[3],
                    pkt.channels[2],
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
        let cell_count = BATTERY_CELL_COUNT;
        let cell_voltage_v100 = osd::pack_millivolts_to_cell_centivolts(pack_mv, cell_count);
        let current_ca = osd::current_sample_to_centiamps(
            sample.current_mv as u32,
            ADC_CURRENT_BETAFLIGHT_SCALE,
        );

        let battery_voltage_v10 = ((pack_mv + 50) / 100).min(u8::MAX as u32) as u8;
        BATTERY_VOLTAGE_V10_SNAPSHOT.store(u32::from(battery_voltage_v10), Ordering::Relaxed);
        BATTERY_CURRENT_CA_SNAPSHOT.store(i32::from(current_ca), Ordering::Relaxed);
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
