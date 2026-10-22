//! What tasks send the observer (ADR-0056): the safety-critical tasks, and
//! the non-critical sources the ledger also carries. Each message travels
//! [`Stamped`](ferrowasp_io_contracts::link::Stamped) over its producer's own
//! link, so a change here changes that producer.
//!
//! The control tick travels over an `event` link, because the blackbox logs
//! every tick marked for it; every other message over a `state` link, whose
//! newest message is all the observer keeps.
//!
//! Requirements: BORDER-002
//! Safety-critical.

/// One control-loop tick: what the loop measured, commanded and decided.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ControlTick {
    /// The blackbox records this tick: an armed tick that reached the
    /// mixer, or any tick of a bench image that logs while disarmed.
    pub logged: bool,
    /// The rate loop's tick count, the blackbox's control sequence.
    pub control_seq: u32,
    /// Control-timer interrupts since boot, whether or not the loop ran.
    pub isr_seq: u32,
    /// The IMU sample the tick used.
    pub imu_seq: u32,
    pub imu_fresh: bool,
    /// Stale beyond the tolerance, the rule that disarms.
    pub imu_stale: bool,
    /// Isolated stale ticks the loop has ridden through since boot.
    pub tolerated_stale_ticks: u32,
    pub bias_calibrated: bool,
    pub armed: bool,
    /// The gyro in the rate controller's axes, raw sensor counts.
    pub gyro_raw: [i32; 3],
    /// Roll, pitch and yaw rates before and after the gyro filters, degrees
    /// per second.
    pub raw_gyro_dps: [f32; 3],
    pub filtered_gyro_dps: [f32; 3],
    /// The filtered rates in the body's axes, degrees per second.
    pub body_rates_dps: [f32; 3],
    /// The commanded rates, degrees per second.
    pub command_dps: [f32; 3],
    pub pid: [f32; 3],
    pub throttle: f32,
    /// Each physical motor's command.
    pub motors: [f32; 4],
    /// Estimated roll, pitch and yaw, degrees.
    pub attitude_deg: [f32; 3],
}

/// The IMU sample the parser last decoded, in the sensor's axes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImuStatus {
    pub sample_seq: u32,
    pub gyro_raw: [i16; 3],
    /// Acceleration in g, and rates in degrees per second.
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
    pub temperature_c: f32,
}

/// The IMU data-ready interrupt's counts since boot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrdyStatus {
    pub interrupts: u32,
    /// Interrupts whose sample read could not be started.
    pub rejected: u32,
    /// When the last one arrived, in the I/O timebase's wrapping
    /// microseconds.
    pub last_us: u32,
}

/// Command Input's latest frame and link state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RcStatus {
    /// Every channel of the last accepted frame, microseconds.
    pub channels_us: [u16; 16],
    pub valid: bool,
    pub armable: bool,
    pub arm_high: bool,
    pub throttle: u32,
    /// Commanded roll, pitch and yaw rates, degrees per second.
    pub rates_dps: [i16; 3],
}

/// The safety master's state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SafetyStatus {
    pub armed: bool,
}

/// The DShot outputs, by physical lane.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutputStatus {
    /// Bit per lane currently commanded above zero.
    pub active_lanes: u8,
    /// The latest valid bidirectional reply, eRPM/100; `None` until one
    /// arrives.
    pub reply_erpm_div100: [Option<u32>; 4],
    pub replies_valid: [u32; 4],
    pub replies_invalid: [u32; 4],
}

/// Legacy UART ESC telemetry, by physical output.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EscStatus {
    /// The latest eRPM/100; `None` until one arrives.
    pub erpm_div100: [Option<u32>; 4],
    /// When each arrived, in wrapping milliseconds.
    pub seen_ms: [u32; 4],
}

/// The battery as the ADC measures it. Non-critical source.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatteryStatus {
    pub voltage_dv: u16,
    pub cell_count: u8,
    pub cell_voltage_cv: u16,
    pub current_ca: i16,
    /// The sense pins' voltages before scaling, millivolts.
    pub voltage_pin_mv: u32,
    pub current_pin_mv: u32,
}

/// The onboard log's storage. Non-critical source.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageStatus {
    pub ready: bool,
    pub pages_written: u32,
    pub write_faults: u32,
    pub records_dropped: u32,
    pub log_rate_divisor: u8,
}
