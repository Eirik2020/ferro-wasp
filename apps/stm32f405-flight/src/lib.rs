#![deny(unsafe_code)]
#![no_std]

#[cfg(any(
    feature = "bench_motor1_only",
    feature = "bench_motor2_only",
    feature = "bench_motor3_only",
    feature = "bench_motor4_only"
))]
compile_error!(
    "The FCU3 DShot bench image supports logical-motor or unequal-vector validation only; remove physical selected-motor features."
);
#[cfg(all(
    feature = "bench_dshot_unequal_motors",
    any(
        feature = "bench_logical_motor1_only",
        feature = "bench_logical_motor2_only",
        feature = "bench_logical_motor3_only",
        feature = "bench_logical_motor4_only"
    )
))]
compile_error!("The DShot unequal-vector test cannot be combined with a logical-motor selection.");
#[cfg(all(
    feature = "bench_dshot_unequal_motors",
    not(feature = "bench_equal_motors")
))]
compile_error!("Feature `bench_dshot_unequal_motors` requires `bench_equal_motors`.");
#[cfg(all(
    not(feature = "bench_equal_motors"),
    any(
        feature = "bench_logical_motor1_only",
        feature = "bench_logical_motor2_only",
        feature = "bench_logical_motor3_only",
        feature = "bench_logical_motor4_only"
    )
))]
compile_error!("A DShot logical-motor selection requires the capped `bench_equal_motors` gate.");
#[cfg(any(
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
compile_error!("Select at most one `bench_logical_motorN_only` feature for DShot validation.");

pub use core::cell::RefCell;
pub use core::sync::atomic::Ordering;
pub use critical_section::Mutex;
pub use defmt::{info, warn};
use defmt_rtt as _;
pub use embedded_io_async::Read as AsyncRead;
pub mod board;
pub use ferrowasp_core::actuator::throttle_to_u16;
pub use ferrowasp_core::safety;
pub use ferrowasp_drivers::mpu6500 as imu;
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
pub use ferrowasp_tasks::drone_toolbox as dt;
pub use ferrowasp_tasks::esc_manager as esc;
pub use ferrowasp_tasks::esc_manager::{
    DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_IDLE_THROTTLE_COMMAND, DSHOT_PREARM_STOP_HOLD_MS,
};
pub use ferrowasp_tasks::osd;
pub use fugit::Rate;
use panic_probe as _;
pub use rtic_monotonics::systick::prelude::*;
pub use sbus_rs::StreamingParser;
pub use stm32_usb::UsbError;

pub type AdcTransfer = stm32_adc::Adc1ObservationTransfer;
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

    pub fn apply(&mut self, values: [f32; 4], now_ms: u32) {
        self.apply_with_lease(values, now_ms, safety::MOTOR_CMD_MAX_AGE_MS);
    }

    pub fn apply_with_lease(&mut self, values: [f32; 4], now_ms: u32, lease_ms: u32) {
        let invalid = values.iter().any(|value| {
            !value.is_finite()
                || *value < safety::ESC_LOW_THROTTLE
                || *value > safety::ESC_MAX_THROTTLE
        });
        let commands = values.map(throttle_to_u16);

        if invalid {
            warn!("DShot motor command refused: invalid throttle vector");
        }

        self.dshot.lock(|dshot| {
            if invalid || commands.iter().all(|command| *command == 0) {
                dshot.command_stop();
            } else {
                match dshot.command_throttles(commands, now_ms, lease_ms) {
                    Ok(()) => {}
                    Err(board::init::DshotCommandError::ThrottleOutOfRange) => {
                        warn!("DShot motor command refused: throttle out of range");
                        dshot.command_stop();
                    }
                    Err(board::init::DshotCommandError::Faulted) => dshot.command_stop(),
                }
            }
        });
    }

    pub fn abort_arming<Report>(
        &mut self,
        done: &signals::ActuatorArmDoneWriter,
        reason: safety::ArmingAbortReason,
        message: &str,
        report: Report,
        now_ms: u32,
    ) where
        Report: FnOnce(safety::ArmingAbortReason) -> bool,
    {
        done.clear();
        self.apply([safety::ESC_LOW_THROTTLE; 4], now_ms);
        warn!("{}", message);
        if !report(reason) {
            warn!("Failed to report aborted DShot idle qualification");
        }
    }
}
pub type EscTelemetryUartIrq = stm32_uart::Uart1RxIrq;
pub type EscTelemetryUartParser = stm32_uart::UartRxParserSide;

pub use board::profiles::{ADC_OBSERVATION_PROFILE, IMU_CONTROL_AXIS_PROFILE};
pub use board::profiles::{DSHOT_FOUR_MOTOR_PROFILE, DSHOT_IDLE_TUNING_MAX_COMMAND};
pub use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32};
pub use embedded_hal::spi::Operation;
pub use embedded_hal_async::spi::SpiDevice;
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
pub static ESC_TELEMETRY_DISCONTINUITY: AtomicBool = AtomicBool::new(false);
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
pub const BATTERY_CELL_COUNT: u8 = ADC_OBSERVATION_PROFILE.battery_cell_count;
pub const ACTUATOR_IDLE_THROTTLE: f32 = DSHOT_IDLE_THROTTLE_COMMAND as f32;
/// Inverse of the FCU3 logical-to-physical `MOTOR_OUTPUT_MAP`.
pub const ESC_OUTPUT_TO_LOGICAL_MOTOR: [u8; 4] = [4, 3, 1, 2];
const _: () = {
    assert!(DSHOT_PREARM_STOP_HOLD_MS > 0);
    assert!(DSHOT_IDLE_THROTTLE_COMMAND > 0);
    assert!(DSHOT_IDLE_THROTTLE_COMMAND <= DSHOT_IDLE_TUNING_MAX_COMMAND);
    assert!(DSHOT_IDLE_QUALIFICATION_CONFIG.is_valid());
    assert!(dt::MOTOR_OUTPUT_MAP[0] == 3);
    assert!(dt::MOTOR_OUTPUT_MAP[1] == 4);
    assert!(dt::MOTOR_OUTPUT_MAP[2] == 2);
    assert!(dt::MOTOR_OUTPUT_MAP[3] == 1);
};

pub const fn logical_motor_for_esc_output(output: esc::EscOutput) -> u8 {
    ESC_OUTPUT_TO_LOGICAL_MOTOR[output.index()]
}

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

#[cfg(feature = "bench_dshot_unequal_motors")]
pub const _: () = {
    assert!(DSHOT_IDLE_THROTTLE_COMMAND <= dt::DSHOT_UNEQUAL_BENCH_MIN_COMMAND);
};
pub const IMU_GYRO_RAW_TO_DPS: f32 = IMU_CONTROL_AXIS_PROFILE.gyro_raw_to_dps as f32 / 10.0;
pub const CONTROL_IMU_TO_DRONE_ROTATION: dt::FrameRotation =
    IMU_CONTROL_AXIS_PROFILE.imu_to_drone_rotation();
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

// ----  SAFETY MASTER  ----
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

const _: () = assert!(ARMING_GUARD_POLL_MS < safety::MOTOR_CMD_MAX_AGE_MS);

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
        imu_ready: IMU_LATEST_SEQ.load(Ordering::Acquire) != 0,
        imu_bias_calibrated: IMU_BIAS_CALIBRATED.load(Ordering::Acquire),
        imu_fresh: !cfg!(feature = "bench_prearm_imu_stale") && !IMU_STALE.load(Ordering::Acquire),
    })
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
