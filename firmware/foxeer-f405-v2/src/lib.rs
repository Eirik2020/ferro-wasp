#![deny(unsafe_code)]
#![no_std]

#[cfg(any(
    feature = "bench_motor1_only",
    feature = "bench_motor2_only",
    feature = "bench_motor3_only",
    feature = "bench_motor4_only"
))]
compile_error!(
    "Foxeer DShot uses logical-motor selection; physical PWM selectors are not supported."
);
#[cfg(all(
    feature = "bench_actuator_validation",
    not(any(
        feature = "bench_equal_motors",
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
compile_error!(
    "Feature `bench_actuator_validation` requires a capped equal-motor, physical-motor, or logical-motor bench feature."
);
#[cfg(all(
    not(feature = "bench_actuator_validation"),
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
compile_error!(
    "Foxeer motor bench features require the explicit props-off `bench_actuator_validation` gate."
);
#[cfg(any(
    all(
        feature = "bench_motor1_only",
        any(
            feature = "bench_motor2_only",
            feature = "bench_motor3_only",
            feature = "bench_motor4_only",
            feature = "bench_logical_motor1_only",
            feature = "bench_logical_motor2_only",
            feature = "bench_logical_motor3_only",
            feature = "bench_logical_motor4_only"
        )
    ),
    all(
        feature = "bench_motor2_only",
        any(
            feature = "bench_motor3_only",
            feature = "bench_motor4_only",
            feature = "bench_logical_motor1_only",
            feature = "bench_logical_motor2_only",
            feature = "bench_logical_motor3_only",
            feature = "bench_logical_motor4_only"
        )
    ),
    all(
        feature = "bench_motor3_only",
        any(
            feature = "bench_motor4_only",
            feature = "bench_logical_motor1_only",
            feature = "bench_logical_motor2_only",
            feature = "bench_logical_motor3_only",
            feature = "bench_logical_motor4_only"
        )
    ),
    all(
        feature = "bench_motor4_only",
        any(
            feature = "bench_logical_motor1_only",
            feature = "bench_logical_motor2_only",
            feature = "bench_logical_motor3_only",
            feature = "bench_logical_motor4_only"
        )
    ),
    all(
        feature = "bench_logical_motor1_only",
        any(
            feature = "bench_logical_motor2_only",
            feature = "bench_logical_motor3_only",
            feature = "bench_logical_motor4_only"
        )
    ),
    all(
        feature = "bench_logical_motor2_only",
        any(
            feature = "bench_logical_motor3_only",
            feature = "bench_logical_motor4_only"
        )
    ),
    all(
        feature = "bench_logical_motor3_only",
        feature = "bench_logical_motor4_only"
    )
))]
compile_error!("Select at most one physical or logical Foxeer motor bench feature.");

pub use core::cell::RefCell;
pub use core::fmt::Write as CoreFmtWrite;
pub use core::sync::atomic::Ordering;
pub use critical_section::Mutex;
pub use defmt::{info, warn};
use defmt_rtt as _;
pub mod board;
pub use ferrowasp_core::actuator::{remap_motor_outputs, throttle_to_u16};
pub use ferrowasp_core::safety;
pub use ferrowasp_drivers::{icm42688p as icm, mpu6500 as imu};
pub use ferrowasp_io_core::serial::{UART2_CONSUMER, UART4_CONSUMER, route_uart_to_task};
pub use ferrowasp_io_core::spi::{
    AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiDeadlineUs,
    SpiRequestMailbox,
};
pub use ferrowasp_io_core::{
    serial::{Discontinuity, RxChunk, SerialFault},
    time::TimestampMicros,
};
pub use ferrowasp_mspv1 as mspv1;
#[cfg(feature = "mspv2_configurator")]
pub use ferrowasp_mspv2 as mspv2;
pub use ferrowasp_stm32f4::adc as stm32_adc;
pub use ferrowasp_stm32f4::app_config::{
    ARMING_GUARD_POLL_MS, BENCH_EQUAL_MOTOR_MAX_THROTTLE, DELAY_TIMER_HZ, ESC_MANAGER_PERIOD_MS,
};
pub use ferrowasp_stm32f4::app_storage as stm32_storage;
pub use ferrowasp_stm32f4::clocks as stm32_clocks;
pub use ferrowasp_stm32f4::hal_prelude::*;
pub use ferrowasp_stm32f4::memory as stm32_memory;
pub use ferrowasp_stm32f4::scheduler as stm32_scheduler;
pub use ferrowasp_stm32f4::spi_dma as stm32_spi;
pub use ferrowasp_stm32f4::spi_dma::*;
pub use ferrowasp_stm32f4::timebase as stm32_timebase;
pub use ferrowasp_stm32f4::uart_dma as stm32_uart;
pub use ferrowasp_stm32f4::usb_serial as stm32_usb;
pub use ferrowasp_stm32f4::watchdog as stm32_watchdog;
pub use ferrowasp_tasks::actuator as actuator_task;
#[cfg(feature = "mspv2_configurator")]
pub use ferrowasp_tasks::blackbox_storage as blackbox_task;
pub use ferrowasp_tasks::drone_toolbox as dt;
pub use ferrowasp_tasks::esc_manager as esc;
pub use ferrowasp_tasks::esc_manager::{
    DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_IDLE_THROTTLE_COMMAND, DSHOT_PREARM_STOP_HOLD_MS,
};
pub use ferrowasp_tasks::flash_storage as flash_task;
pub use ferrowasp_tasks::osd;
#[cfg(not(feature = "mspv2_configurator"))]
pub use ferrowasp_tasks::usb_debug;
pub use fugit::Rate;
use panic_probe as _;
pub use rtic_monotonics::systick::prelude::*;
pub use sbus_rs::StreamingParser;
pub use stm32_usb::UsbDeviceState;

pub type DshotShared = board::init::DshotMotorBank;

pub struct ActuatorHardware<'a, SharedDshot>
where
    SharedDshot: rtic::Mutex<T = DshotShared>,
{
    dshot: &'a mut SharedDshot,
}

impl<'a, SharedDshot> ActuatorHardware<'a, SharedDshot>
where
    SharedDshot: rtic::Mutex<T = DshotShared>,
{
    pub fn new(dshot: &'a mut SharedDshot) -> Self {
        Self { dshot }
    }

    pub fn force_off(&mut self) {
        self.dshot.lock(|dshot| dshot.command_stop());
    }

    pub fn apply(&mut self, values: [f32; 4], now_ms: u32) -> bool {
        self.apply_with_lease(values, now_ms, safety::MOTOR_CMD_MAX_AGE_MS)
    }

    pub fn apply_with_lease(&mut self, values: [f32; 4], now_ms: u32, lease_ms: u32) -> bool {
        let commands = values.map(throttle_to_u16);
        let result = self
            .dshot
            .lock(|dshot| dshot.command_throttles(commands, now_ms, lease_ms))
            .map_err(|_| ());

        if result.is_err() {
            warn!("Foxeer DShot command rejected");
            self.force_off();
            false
        } else {
            true
        }
    }

    pub fn abort_arming<Report>(
        &mut self,
        done: &signals::ActuatorArmDoneWriter,
        reason: safety::ArmingAbortReason,
        message: &str,
        report: Report,
    ) where
        Report: FnOnce(safety::ArmingAbortReason) -> bool,
    {
        done.clear();
        self.force_off();
        warn!("{}", message);
        if !report(reason) {
            warn!("Failed to report aborted DShot idle qualification");
        }
    }
}

pub type EscTelemetryUartIrq = stm32_uart::Uart1RxIrq;
pub type EscTelemetryUartParser = stm32_uart::UartRxParserSide;
pub type EscManagerState = esc::EscManager;
pub type EscRequestProducer = esc::EscRequestProducer;
pub type EscRequestConsumer = esc::EscRequestConsumer;
pub type EscAckProducer = esc::EscAckProducer;
pub type EscAckConsumer = esc::EscAckConsumer;
pub type EscTelemetryUpdateProducer = esc::EscTelemetryUpdateProducer;
pub type EscTelemetryUpdateConsumer = esc::EscTelemetryUpdateConsumer;

pub type AdcTransfer = board::aliases::Adc1ObservationTransfer;
pub type ControlScheduler = board::aliases::ControlScheduler;
pub type IoTimebase = stm32_timebase::MicrosecondTimebase<board::aliases::IoTimebaseTimer>;
pub type IoWatchdog = board::aliases::IoWatchdog;
pub type Uart4OwnedRxChannel = stm32_memory::UartOwnedRxChannel;
pub type Uart4OwnedRxProducer = stm32_memory::UartOwnedRxProducer<'static>;
pub type Uart4OwnedReader = stm32_memory::UartOwnedReader<'static>;
pub type Uart4Discontinuities = stm32_memory::UartOwnedDiscontinuities<'static>;
pub type Uart2OwnedRxChannel = stm32_memory::UartOwnedRxChannel;
pub type Uart2OwnedReader = stm32_memory::UartOwnedReader<'static>;
pub type Uart2Discontinuities = stm32_memory::UartOwnedDiscontinuities<'static>;
pub type Uart2OwnedRxBridge = stm32_uart::UartOwnedRxBridge<
    'static,
    { stm32_memory::UART_RX_BUFFER_BYTES },
    { stm32_memory::OWNED_UART_RX_QUEUE_DEPTH },
>;
pub type Uart4OwnedTxChannel = stm32_memory::UartOwnedTxChannel;
pub type Uart4OwnedWriter = stm32_memory::UartOwnedWriter<'static>;
pub type Uart4OwnedTxOwner = stm32_memory::UartOwnedTxOwner<'static>;
pub type Uart4OwnedTxCompletion = stm32_memory::UartOwnedTxCompletion<'static>;
pub type UsbDebugDevice = stm32_usb::UsbCdcDevice;
pub type UsbDebugSerial = stm32_usb::BufferedUsbCdcSerial;
pub type FlashDevice = board::aliases::Spi2Flash;
pub type FlashRecordProducer = flash_task::RecordProducer;
pub type FlashRecordConsumer = flash_task::RecordConsumer;
pub type FlashCommandProducer = flash_task::CommandProducer;
pub type FlashCommandConsumer = flash_task::CommandConsumer;
pub type FlashResponseProducer = flash_task::ResponseProducer;
pub type FlashResponseConsumer = flash_task::ResponseConsumer;

#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcCommandProducer = flash_task::RpcCommandProducer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcCommandProducer = ();
#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcCommandConsumer = flash_task::RpcCommandConsumer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcCommandConsumer = ();
#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcResponseProducer = flash_task::RpcResponseProducer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcResponseProducer = ();
#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcResponseConsumer = flash_task::RpcResponseConsumer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcResponseConsumer = ();

#[cfg(feature = "mspv2_configurator")]
pub struct ConfiguratorUsbState {
    pub parser: mspv2::MspParser,
    pub pending_tx: mspv2::EncodedFrame,
    pub rpc_payload: [u8; mspv2::MAX_PAYLOAD_LEN],
}

#[cfg(feature = "mspv2_configurator")]
impl ConfiguratorUsbState {
    pub const fn new() -> Self {
        Self {
            parser: mspv2::MspParser::new(),
            pending_tx: mspv2::EncodedFrame::new(),
            rpc_payload: [0; mspv2::MAX_PAYLOAD_LEN],
        }
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(not(feature = "mspv2_configurator"), allow(dead_code))]
pub enum ConfigRpcCompletion {
    Commit(u16),
    Defaults(u16),
}

#[cfg(not(feature = "mspv2_configurator"))]
pub struct ConfiguratorUsbState;

#[cfg(not(feature = "mspv2_configurator"))]
impl ConfiguratorUsbState {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ConfiguratorUsbState {
    fn default() -> Self {
        Self::new()
    }
}

pub use board::Spi1ImuKind;
pub use board::profiles::{
    ADC_OBSERVATION_PROFILE, ARMING_INHIBIT_REASON, FLIGHT_ARMING_ENABLED, IMU_CONTROL_AXIS_PROFILE,
};
pub const BENCH_ACTUATOR_VALIDATION_ENABLED: bool = cfg!(feature = "bench_actuator_validation");
pub const SMOKE_ACTUATOR_INHIBIT_ENABLED: bool = cfg!(feature = "smoke_actuator_inhibit");
pub const ACTUATOR_OUTPUT_ENABLED: bool =
    !SMOKE_ACTUATOR_INHIBIT_ENABLED && (FLIGHT_ARMING_ENABLED || BENCH_ACTUATOR_VALIDATION_ENABLED);
pub const ACTUATOR_INHIBIT_REASON: &str = if SMOKE_ACTUATOR_INHIBIT_ENABLED {
    "Foxeer smoke-test actuator lockout is active"
} else {
    ARMING_INHIBIT_REASON
};
pub use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32};
pub use embedded_hal::spi::Operation;
pub use embedded_hal_async::spi::SpiDevice;
pub use embedded_io_async::Read as AsyncRead;
pub use ferrowasp_core::safety::signals::{
    self, ActuatorArmPermitReader, ActuatorArmPermitWriter, RcRatesReader, RcRatesWriter,
};

pub static RC_ARM_HIGH: AtomicBool = AtomicBool::new(false);
pub static RC_THROTTLE: AtomicU32 = AtomicU32::new(0);
pub static SAFETY_ARMED: AtomicBool = AtomicBool::new(false);
pub static IMU_STALE: AtomicBool = AtomicBool::new(true);
pub static IMU_BIAS_CALIBRATED: AtomicBool = AtomicBool::new(false);
pub static CONTROL_RATE_SEQ: AtomicU32 = AtomicU32::new(0);
pub static CONTROL_ISR_SEQ: AtomicU32 = AtomicU32::new(0);
pub static CONTROL_ROLL_RAW: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_PITCH_RAW: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_YAW_RAW: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_ROLL_DPS10: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_PITCH_DPS10: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_YAW_DPS10: AtomicI32 = AtomicI32::new(0);
pub static IMU_LATEST_SEQ: AtomicU32 = AtomicU32::new(0);
pub static IMU_LATEST_ROLL_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_LATEST_PITCH_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_LATEST_YAW_RAW: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_ORIENTATION_VERSION: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_LATEST_ACCEL_X_MG: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_LATEST_ACCEL_Y_MG: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_LATEST_ACCEL_Z_MG: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_LATEST_GYRO_X_DPS10: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_LATEST_GYRO_Y_DPS10: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_LATEST_GYRO_Z_DPS10: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub static IMU_LATEST_TEMP_C10: AtomicI32 = AtomicI32::new(0);
pub static IMU_TRANSPORT_READY: AtomicBool = AtomicBool::new(false);
pub static IMU_DRDY_IRQ_COUNT: AtomicU32 = AtomicU32::new(0);
pub static IMU_DRDY_REJECTED_COUNT: AtomicU32 = AtomicU32::new(0);
pub static IMU_DRDY_LAST_US: AtomicU32 = AtomicU32::new(0);
pub static ESC_TELEMETRY_DISCONTINUITY: AtomicBool = AtomicBool::new(false);
pub static ACTIVE_IMU_KIND: AtomicU8 = AtomicU8::new(0);
pub static BATTERY_VOLTAGE_V10_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
pub static BATTERY_CURRENT_CA_SNAPSHOT: AtomicI32 = AtomicI32::new(0);
pub static ADC_VOLTAGE_MV_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
pub static ADC_CURRENT_MV_SNAPSHOT: AtomicU32 = AtomicU32::new(0);
pub static USB_DEBUG_DUE: AtomicBool = AtomicBool::new(false);
pub static USB_RC_VALID_SNAPSHOT: AtomicBool = AtomicBool::new(false);
pub static USB_RC_ARMABLE_SNAPSHOT: AtomicBool = AtomicBool::new(false);
pub static FLASH_READY: AtomicBool = AtomicBool::new(false);
pub static FLASH_JEDEC_MANUFACTURER: AtomicU8 = AtomicU8::new(0);
pub static FLASH_JEDEC_MEMORY_TYPE: AtomicU8 = AtomicU8::new(0);
pub static FLASH_JEDEC_CAPACITY_CODE: AtomicU8 = AtomicU8::new(0);
pub static FLASH_CAPACITY_BYTES: AtomicU32 = AtomicU32::new(0);
pub static FLASH_LOG_RATE_DIVISOR: AtomicU32 = AtomicU32::new(1);
pub static FLASH_RECORDS_DROPPED: AtomicU32 = AtomicU32::new(0);
pub static FLASH_PAGES_WRITTEN: AtomicU32 = AtomicU32::new(0);
pub static FLASH_WRITE_FAULTS: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "imu_orientation_rtt")]
pub fn imu_orientation_snapshot() -> Option<(u32, [i32; 3], [i32; 3], i32)> {
    for _ in 0..4 {
        let version_before = IMU_ORIENTATION_VERSION.load(Ordering::Acquire);
        if version_before & 1 != 0 {
            continue;
        }

        let accel_mg = [
            IMU_LATEST_ACCEL_X_MG.load(Ordering::Relaxed),
            IMU_LATEST_ACCEL_Y_MG.load(Ordering::Relaxed),
            IMU_LATEST_ACCEL_Z_MG.load(Ordering::Relaxed),
        ];
        let gyro_dps10 = [
            IMU_LATEST_GYRO_X_DPS10.load(Ordering::Relaxed),
            IMU_LATEST_GYRO_Y_DPS10.load(Ordering::Relaxed),
            IMU_LATEST_GYRO_Z_DPS10.load(Ordering::Relaxed),
        ];
        let temp_c10 = IMU_LATEST_TEMP_C10.load(Ordering::Relaxed);
        let version_after = IMU_ORIENTATION_VERSION.load(Ordering::Acquire);
        if version_before == version_after {
            return Some((version_after / 2, accel_mg, gyro_dps10, temp_c10));
        }
    }

    None
}

#[cfg(not(feature = "mspv2_configurator"))]
pub const USB_DEBUG_HEADER: &[u8] = b"FerroWasp Foxeer F405 V2 storage CLI v1; type help\r\n";
pub type Spi1Mailbox = SharedSpiRequestMailbox<SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub type Spi1Executor =
    CriticalSectionSpiExecutor<'static, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub type Spi1Device = AsyncSpiDevice<Spi1Executor, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub static SPI1_MAILBOX: Spi1Mailbox =
    critical_section::Mutex::new(core::cell::RefCell::new(SpiRequestMailbox::new()));
pub static RC_RATES: Mutex<RefCell<safety::RcRates>> = Mutex::new(RefCell::new(safety::RcRates {
    roll: 0,
    pitch: 0,
    yaw: 0,
}));
pub static RC_LINK: Mutex<RefCell<safety::RcLinkState>> =
    Mutex::new(RefCell::new(safety::RcLinkState::new()));
pub static ACTUATOR_ARM_DONE: AtomicBool = AtomicBool::new(false);
pub static ACTUATOR_ARM_PERMIT: AtomicBool = AtomicBool::new(false);
pub const ADC_VBAT_DIVIDER_RATIO: f32 = ADC_OBSERVATION_PROFILE.vbat_divider_ratio;
pub const ADC_CURRENT_BETAFLIGHT_SCALE: u32 = ADC_OBSERVATION_PROFILE.current_betaflight_scale;
pub const ADC_CURRENT_OFFSET_MA: i32 = ADC_OBSERVATION_PROFILE.current_offset_ma;
pub const BATTERY_MAX_CELL_MV: u16 = ADC_OBSERVATION_PROFILE.battery_max_cell_mv;
pub const BATTERY_DETECT_CELL_MV: u16 = ADC_OBSERVATION_PROFILE.battery_detect_cell_mv;
pub const BATTERY_MAX_CELLS: u8 = ADC_OBSERVATION_PROFILE.battery_max_cells;
pub const FOXEER_DSHOT_IDLE_COMMAND: f32 = DSHOT_IDLE_THROTTLE_COMMAND as f32;
const _: () = {
    assert!(DSHOT_IDLE_THROTTLE_COMMAND > 0);
    assert!(DSHOT_IDLE_QUALIFICATION_CONFIG.is_valid());
};

#[cfg(feature = "bench_dshot_idle_output1_not_running")]
pub fn inject_idle_qualification_fault(
    mut update: esc::EscTelemetryUpdate,
) -> esc::EscTelemetryUpdate {
    if update.output == esc::EscOutput::Output1 {
        update.observation.sample.erpm_div100 = 0;
    }
    update
}

#[cfg(not(feature = "bench_dshot_idle_output1_not_running"))]
pub const fn inject_idle_qualification_fault(
    update: esc::EscTelemetryUpdate,
) -> esc::EscTelemetryUpdate {
    update
}
pub const IMU_GYRO_RAW_TO_DPS: f32 = IMU_CONTROL_AXIS_PROFILE.gyro_raw_to_dps as f32 / 10.0;
#[cfg(feature = "imu_orientation_rtt")]
pub const PHYSICAL_IMU_TO_DRONE_ROTATION: dt::FrameRotation =
    IMU_CONTROL_AXIS_PROFILE.imu_to_drone_rotation();
pub const CONTROL_IMU_TO_RATE_CONTROLLER_MAP: dt::FrameRotation =
    IMU_CONTROL_AXIS_PROFILE.imu_to_rate_controller_map();
pub const GYRO_BIAS_CALIBRATION_SAMPLES: u32 = IMU_CONTROL_AXIS_PROFILE.bias_calibration_samples;
pub const GYRO_BIAS_CALIBRATION_MAX_RAW: i32 = IMU_CONTROL_AXIS_PROFILE.bias_calibration_max_raw;
pub fn warn_arming_abort(reason: safety::ArmingAbortReason) {
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
        safety::ArmingAbortReason::ImuUnavailable => {
            warn!("Arming aborted: IMU has not produced a valid sample")
        }
        safety::ArmingAbortReason::ImuBiasUncalibrated => {
            warn!("Arming aborted: gyro bias calibration is incomplete")
        }
        safety::ArmingAbortReason::ImuStale => {
            warn!("Arming aborted: IMU sample is stale")
        }
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

pub async fn osd_write(writer: &mut Uart4OwnedWriter, healthy: &mut bool, bytes: &[u8]) {
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

#[cfg(not(feature = "mspv2_configurator"))]
pub fn active_usb_debug_imu_kind() -> usb_debug::ImuKind {
    match Spi1ImuKind::from_discriminant(ACTIVE_IMU_KIND.load(Ordering::Relaxed)) {
        Some(Spi1ImuKind::Mpu6500) => usb_debug::ImuKind::Mpu6500,
        Some(Spi1ImuKind::Icm42688P) => usb_debug::ImuKind::Icm42688P,
        None => usb_debug::ImuKind::None,
    }
}

#[cfg(feature = "mspv2_configurator")]
pub fn device_uid() -> [u8; 12] {
    let uid = stm32f4xx_hal::signature::Uid::get();
    let mut bytes = [0u8; 12];
    bytes[..2].copy_from_slice(&uid.x().to_le_bytes());
    bytes[2..4].copy_from_slice(&uid.y().to_le_bytes());
    bytes[4] = uid.waf_num();
    let lot = uid.lot_num().as_bytes();
    let len = lot.len().min(7);
    bytes[5..5 + len].copy_from_slice(&lot[..len]);
    bytes
}

#[cfg(feature = "mspv2_configurator")]
pub fn fixed_bytes<const N: usize>(value: &[u8]) -> [u8; N] {
    let mut output = [0u8; N];
    let len = value.len().min(N);
    output[..len].copy_from_slice(&value[..len]);
    output
}

#[cfg(feature = "mspv2_configurator")]
pub fn msp_common_info() -> mspv2::commands::CommonInfo<'static> {
    mspv2::commands::CommonInfo {
        firmware_version: [0, 1, 0],
        board_identifier: *b"FXR2",
        board_name: b"Foxeer F405 V2",
        target_name: b"foxeer_f405_v2",
        build_date: fixed_bytes(env!("FWSP_BUILD_DATE").as_bytes()),
        build_time: fixed_bytes(env!("FWSP_BUILD_TIME").as_bytes()),
        git_revision: fixed_bytes(env!("FWSP_GIT_REV").as_bytes()),
        uid: device_uid(),
        cycle_time_us: 2_500,
        sensors: u16::from(IMU_TRANSPORT_READY.load(Ordering::Relaxed)),
        armed: SAFETY_ARMED.load(Ordering::Acquire),
    }
}

#[cfg(feature = "mspv2_configurator")]
pub fn rpc_device_info() -> mspv2::rpc::DeviceInfo {
    use mspv2::rpc::capabilities;

    let capabilities = capabilities::CONFIG_READ
        | capabilities::CONFIG_WRITE
        | capabilities::CONFIG_RESET
        | capabilities::BLACKBOX_LIST
        | capabilities::BLACKBOX_DOWNLOAD;
    let mut board_id = [0u8; 16];
    let id = b"foxeer_f405_v2";
    board_id[..id.len()].copy_from_slice(id);
    mspv2::rpc::DeviceInfo {
        protocol_version: mspv2::rpc::FWSP_RPC_VERSION,
        firmware_version: [0, 1, 0],
        git_revision: fixed_bytes(env!("FWSP_GIT_REV").as_bytes()),
        board_id,
        board_id_len: id.len() as u8,
        mcu: mspv2::rpc::McuKind::Stm32F405,
        device_serial: device_uid(),
        armed: SAFETY_ARMED.load(Ordering::Acquire),
        capabilities,
        max_blackbox_chunk: mspv2::rpc::MAX_BLACKBOX_CHUNK as u16,
        config_schema_version: mspv2::rpc::CONFIG_SCHEMA_VERSION,
    }
}

#[cfg(feature = "mspv2_configurator")]
pub fn stage_rpc_response(
    state: &mut ConfiguratorUsbState,
    response: &mspv2::rpc::RpcResponse,
) -> bool {
    let ConfiguratorUsbState {
        pending_tx,
        rpc_payload,
        ..
    } = state;
    let Ok(payload) = mspv2::rpc::encode_response(response, rpc_payload) else {
        return false;
    };
    pending_tx
        .set(
            mspv2::MspDirection::FromFlightController,
            0,
            mspv2::rpc::MSP2_FWSP_RPC,
            payload,
        )
        .is_ok()
}

#[cfg(feature = "mspv2_configurator")]
pub fn handle_msp_packet(
    packet: mspv2::MspPacket,
    state: &mut ConfiguratorUsbState,
    rpc_commands: &mut flash_task::RpcCommandProducer,
) {
    if packet.direction != mspv2::MspDirection::ToFlightController || state.pending_tx.is_pending()
    {
        return;
    }

    let mut common_payload = [0u8; 96];
    if let Some(len) =
        mspv2::commands::encode_common_payload(&packet, &msp_common_info(), &mut common_payload)
    {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::FromFlightController,
            packet.flags,
            packet.function,
            &common_payload[..len],
        );
        return;
    }

    if packet.function != mspv2::rpc::MSP2_FWSP_RPC {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::Error,
            packet.flags,
            packet.function,
            &[],
        );
        return;
    }

    let Ok(request) = mspv2::rpc::decode_request(packet.payload()) else {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::Error,
            packet.flags,
            packet.function,
            &[],
        );
        return;
    };
    if request.protocol_version != mspv2::rpc::FWSP_RPC_VERSION {
        let response = mspv2::rpc::error(
            request.request_id,
            mspv2::rpc::DeviceError::UnsupportedProtocolVersion,
        );
        let _ = stage_rpc_response(state, &response);
        return;
    }
    if request.operation == mspv2::rpc::Request::Hello {
        let response = mspv2::rpc::ok(
            request.request_id,
            mspv2::rpc::Response::Hello(rpc_device_info()),
        );
        let _ = stage_rpc_response(state, &response);
        return;
    }
    let request_id = request.request_id;
    if rpc_commands.enqueue(request).is_err() {
        let response = mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy);
        let _ = stage_rpc_response(state, &response);
    }
}

pub fn scan_flash_log(
    flash: &mut FlashDevice,
    layout: flash_task::StorageLayout,
) -> Result<(u32, u32, bool), flash_task::StorageReadError<board::aliases::Spi2FlashError>> {
    flash_task::scan_log(layout, |address, page| flash.read(address, page))
}

pub fn load_flash_config(
    flash: &mut FlashDevice,
    layout: flash_task::StorageLayout,
) -> Result<
    (flash_task::StoredConfig, u32, u8),
    flash_task::StorageReadError<board::aliases::Spi2FlashError>,
> {
    flash_task::load_config(
        layout,
        flash_task::StoredConfig::foxeer_f405_v2_default(),
        |address, page| flash.read(address, page),
    )
}

pub fn queue_storage_response(producer: &mut flash_task::ResponseProducer, text: &str) -> bool {
    let Some(frame) = flash_task::ResponseFrame::from_text(text) else {
        return false;
    };
    if producer.enqueue(frame).is_err() {
        return false;
    }
    cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
    true
}

#[cfg(feature = "mspv2_configurator")]
pub fn queue_rpc_response(
    producer: &mut flash_task::RpcResponseProducer,
    response: mspv2::rpc::RpcResponse,
) -> bool {
    if producer.enqueue(response).is_err() {
        return false;
    }
    cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
    true
}

#[cfg(feature = "mspv2_configurator")]
pub fn finish_config_rpc_error(
    producer: &mut flash_task::RpcResponseProducer,
    completion: &mut Option<ConfigRpcCompletion>,
    error: mspv2::rpc::DeviceError,
) -> bool {
    let Some(completion) = completion.take() else {
        return false;
    };
    let request_id = match completion {
        ConfigRpcCompletion::Commit(id) | ConfigRpcCompletion::Defaults(id) => id,
    };
    queue_rpc_response(producer, mspv2::rpc::error(request_id, error))
}

#[cfg(feature = "mspv2_configurator")]
pub fn blackbox_flight_id_at(
    flash: &mut FlashDevice,
    layout: flash_task::StorageLayout,
    page_index: u32,
) -> Result<u32, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::blackbox_flight_id_at(&mut read, layout, page_index)
}

#[cfg(feature = "mspv2_configurator")]
pub fn blackbox_bounds(
    flash: &mut FlashDevice,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
) -> Result<Option<(u32, u32)>, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::blackbox_bounds(&mut read, layout, used_pages, id)
}

#[cfg(feature = "mspv2_configurator")]
pub fn blackbox_info(
    flash: &mut FlashDevice,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
    active_id: Option<u32>,
) -> Result<Option<mspv2::rpc::BlackboxInfo>, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::blackbox_info(&mut read, layout, used_pages, id, active_id)
}

#[cfg(feature = "mspv2_configurator")]
pub fn list_blackboxes(
    flash: &mut FlashDevice,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    active_id: Option<u32>,
) -> Result<mspv2::rpc::BlackboxList, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::list_blackboxes(&mut read, layout, used_pages, active_id)
}

#[cfg(feature = "mspv2_configurator")]
pub fn read_blackbox_chunk(
    flash: &mut FlashDevice,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
    offset: u32,
    requested_length: u16,
) -> Result<mspv2::rpc::BlackboxChunk, mspv2::rpc::DeviceError> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::read_blackbox_chunk(&mut read, layout, used_pages, id, offset, requested_length)
}

pub fn queue_page_hex(
    producer: &mut flash_task::ResponseProducer,
    page_index: u32,
    page: &[u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN],
) -> bool {
    flash_task::emit_page_hex_lines(page_index, page, |line| {
        queue_storage_response(producer, line)
    })
}

pub fn flash_scratch_test_page() -> [u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN] {
    flash_task::scratch_test_page()
}

pub fn motor_command_timestamp(now_ms: u32, sequence: u32) -> u32 {
    #[cfg(feature = "bench_motor_cmd_stale_rejection")]
    if sequence == 1 {
        return now_ms.wrapping_sub(safety::MOTOR_CMD_MAX_AGE_MS + 1);
    }

    let _ = sequence;
    now_ms
}

pub fn take_fresh_motor_outputs(
    reader: &mut safety::signals::MotorCmdReader,
    now_ms: u32,
) -> Option<[f32; 4]> {
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

pub fn record_uart2_discontinuity<Invalidate>(
    bridge: &mut Uart2OwnedRxBridge,
    cause: Discontinuity,
    generation: ferrowasp_io_core::serial::StreamGeneration,
    timestamp: TimestampMicros,
    reason: safety::RcLinkInvalidation,
    mut invalidate: Invalidate,
) where
    Invalidate: FnMut(safety::RcLinkInvalidation) -> bool,
{
    bridge.record_discontinuity(cause, generation, timestamp);
    let _ = invalidate(reason);
}

pub fn publish_uart2_owned<Invalidate>(
    bridge: &mut Uart2OwnedRxBridge,
    generation: ferrowasp_io_core::serial::StreamGeneration,
    timestamp: TimestampMicros,
    mut invalidate: Invalidate,
) where
    Invalidate: FnMut(safety::RcLinkInvalidation) -> bool,
{
    match bridge.publish_next(timestamp) {
        stm32_uart::UartOwnedRxBridgeOutcome::Published => {}
        stm32_uart::UartOwnedRxBridgeOutcome::NoChunk => {
            warn!("USART2 delivered IRQ had no detached RX chunk");
            record_uart2_discontinuity(
                bridge,
                Discontinuity::TransportReset,
                generation,
                timestamp,
                safety::RcLinkInvalidation::TransportDiscontinuity,
                &mut invalidate,
            );
        }
        stm32_uart::UartOwnedRxBridgeOutcome::InvalidChunk => {
            warn!("USART2 produced an invalid owned RX chunk");
            record_uart2_discontinuity(
                bridge,
                Discontinuity::FramingError,
                generation,
                timestamp,
                safety::RcLinkInvalidation::TransportDiscontinuity,
                &mut invalidate,
            );
        }
        stm32_uart::UartOwnedRxBridgeOutcome::QueueOverflow => {
            warn!("USART2 owned RX queue overflowed");
            let _ = invalidate(safety::RcLinkInvalidation::TransportDiscontinuity);
        }
        stm32_uart::UartOwnedRxBridgeOutcome::Disabled => {
            warn!("USART2 owned RX channel is disabled");
            record_uart2_discontinuity(
                bridge,
                Discontinuity::TransportReset,
                generation,
                timestamp,
                safety::RcLinkInvalidation::TransportDiscontinuity,
                &mut invalidate,
            );
        }
        stm32_uart::UartOwnedRxBridgeOutcome::RecycleFailed => {
            panic!("USART2 detached DMA buffer could not be recycled");
        }
    }
}

pub fn record_uart2_dma_error<Invalidate>(
    bridge: &mut Uart2OwnedRxBridge,
    generation: ferrowasp_io_core::serial::StreamGeneration,
    timestamp: TimestampMicros,
    invalidate: Invalidate,
) where
    Invalidate: FnMut(safety::RcLinkInvalidation) -> bool,
{
    record_uart2_discontinuity(
        bridge,
        Discontinuity::DmaError,
        generation,
        timestamp,
        safety::RcLinkInvalidation::DmaError,
        invalidate,
    );
}

pub fn current_live_arming_guard(
    permit: &ActuatorArmPermitReader,
    rc_link: &signals::RcLinkReader,
    arm_high: &signals::RcArmHighReader,
    throttle: &signals::RcThrottleReader,
    now_us: u32,
) -> Result<(), safety::ArmingAbortReason> {
    validate_live_arming_guard(
        permit.read(),
        rc_link.is_armable(now_us),
        arm_high.read(),
        throttle.read(),
    )
}

pub async fn wait_live_arming_hold<Now, Delay, DelayFuture>(
    permit: &ActuatorArmPermitReader,
    rc_link: &signals::RcLinkReader,
    arm_high: &signals::RcArmHighReader,
    throttle: &signals::RcThrottleReader,
    hold_ms: u32,
    mut now_us: Now,
    delay_ms: Delay,
) -> Result<(), safety::ArmingAbortReason>
where
    Now: FnMut() -> u32,
    Delay: FnMut(u32) -> DelayFuture,
    DelayFuture: core::future::Future<Output = ()>,
{
    ferrowasp_tasks::arming::wait_hold(
        hold_ms,
        ARMING_GUARD_POLL_MS,
        || current_live_arming_guard(permit, rc_link, arm_high, throttle, now_us()),
        delay_ms,
    )
    .await
}

pub fn validate_live_arming_guard(
    permit: bool,
    rc_link_armable: bool,
    arm_high: bool,
    throttle: u32,
) -> Result<(), safety::ArmingAbortReason> {
    safety::validate_arming_guard(permit, rc_link_armable, arm_high, throttle)?;
    safety::validate_prearm_health(safety::PreArmHealth {
        imu_ready: IMU_TRANSPORT_READY.load(Ordering::Acquire)
            && Spi1ImuKind::from_discriminant(ACTIVE_IMU_KIND.load(Ordering::Acquire)).is_some()
            && IMU_LATEST_SEQ.load(Ordering::Acquire) != 0,
        imu_bias_calibrated: IMU_BIAS_CALIBRATED.load(Ordering::Acquire),
        imu_fresh: !cfg!(feature = "bench_prearm_imu_stale") && !IMU_STALE.load(Ordering::Acquire),
    })
}

pub const fn dshot_motor_for_output(output: esc::EscOutput) -> board::init::DshotMotor {
    match output {
        esc::EscOutput::Output1 => board::init::DshotMotor::Motor1,
        esc::EscOutput::Output2 => board::init::DshotMotor::Motor2,
        esc::EscOutput::Output3 => board::init::DshotMotor::Motor3,
        esc::EscOutput::Output4 => board::init::DshotMotor::Motor4,
    }
}

pub const fn logical_motor_for_physical_index(physical_index: usize) -> u8 {
    let physical_output = physical_index + 1;
    let mut logical_index = 0;
    while logical_index < board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT.len() {
        if board::profiles::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT[logical_index] == physical_output {
            return logical_index as u8 + 1;
        }
        logical_index += 1;
    }
    0
}

pub const fn logical_motor_for_esc_output(output: esc::EscOutput) -> u8 {
    logical_motor_for_physical_index(output.index())
}

pub fn service_dshot_dma_irq(
    bank: &mut impl rtic::Mutex<T = DshotShared>,
    motor: board::init::DshotMotor,
    stream: u8,
) {
    let event = bank.lock(|dshot| dshot.on_dma_interrupt(motor));
    if event == board::init::DshotInterruptEvent::Spurious {
        warn!(
            "Foxeer DShot received spurious DMA2 Stream{} interrupt",
            stream
        );
    }
}

pub struct ParsedImuSample {
    pub acc: [f32; 3],
    pub gyro: [f32; 3],
    pub gyro_raw: [i16; 3],
    pub temp: f32,
}
pub fn neutralize_rc_input(
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

pub mod internal {
    pub use crate::*;
}
