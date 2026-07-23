#![allow(dead_code)]
pub use ferrowasp_core::frames::{DroneBodyFrame, FrameRotation};
pub use ferrowasp_pid::{ControlOutput, Pid};

pub const RC_CHANNEL_MIN: u16 = 192;
pub const RC_CHANNEL_CENTER: u16 = 992;
pub const RC_CHANNEL_MAX: u16 = 1792;
pub const RC_RATE_MAX_DPS: f32 = 1000.0;
pub const RC_THROTTLE_MAX: u32 = 2000;
pub const RC_RATE_DEADBAND: i32 = 8;
pub const RC_INVERT_ROLL: bool = false;
pub const RC_INVERT_PITCH: bool = false;
pub const RC_INVERT_YAW: bool = false;
pub const RC_INVERT_THROTTLE: bool = false;
pub const RC_ROLL_CHANNEL_INDEX: usize = 0;
pub const RC_PITCH_CHANNEL_INDEX: usize = 1;
pub const RC_YAW_CHANNEL_INDEX: usize = 2;
pub const IMU_GYRO_LPF_ALPHA: f32 = 0.55;
pub const IMU_POLL_RATE_HZ: u32 = 800;
pub const CONTROL_LOOP_RATE_HZ: u32 = 400;
pub const CONTROL_LOOP_DT_SECONDS: f32 = 1.0 / CONTROL_LOOP_RATE_HZ as f32;
pub const IMU_COMPLEMENTARY_GYRO_WEIGHT: f32 = 0.98;
pub const RATE_CONTROLLER_D_FILTER_ALPHA: f32 = 0.25;
pub const RATE_CONTROLLER_I_RELAX_SETPOINT_RATE_DPS: f32 = 400.0;
pub const RATE_CONTROLLER_OUTPUT_LIMIT: f32 = 2000.0;
const RAD_TO_DEG: f32 = 57.295_78;

pub type GyroAxisMap = FrameRotation;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GyroBiasUpdate {
    pub corrected_raw: [i32; 3],
    pub newly_calibrated: bool,
    pub bias_raw: [i32; 3],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GyroBiasCalibrator {
    sum: [i64; 3],
    samples: u32,
    bias_raw: [i32; 3],
    ready: bool,
    required_samples: u32,
    max_motion_raw: i32,
}

impl GyroBiasCalibrator {
    pub const fn new(required_samples: u32, max_motion_raw: i32) -> Self {
        Self {
            sum: [0; 3],
            samples: 0,
            bias_raw: [0; 3],
            ready: false,
            required_samples,
            max_motion_raw,
        }
    }

    pub const fn ready(self) -> bool {
        self.ready
    }

    pub const fn bias_raw(self) -> [i32; 3] {
        self.bias_raw
    }

    pub fn update(&mut self, armed: bool, raw: [i32; 3]) -> GyroBiasUpdate {
        let mut newly_calibrated = false;

        if !armed && !self.ready {
            let calibrating_motion = raw.iter().any(|value| value.abs() > self.max_motion_raw);

            if calibrating_motion {
                self.sum = [0; 3];
                self.samples = 0;
            } else {
                for (axis, value) in raw.iter().enumerate() {
                    self.sum[axis] += *value as i64;
                }

                self.samples = self.samples.saturating_add(1);
                if self.samples >= self.required_samples {
                    for axis in 0..3 {
                        self.bias_raw[axis] =
                            (self.sum[axis] / self.required_samples as i64) as i32;
                    }
                    self.ready = true;
                    newly_calibrated = true;
                }
            }
        }

        let mut corrected_raw = raw;
        if self.ready {
            for (axis, value) in corrected_raw.iter_mut().enumerate() {
                *value -= self.bias_raw[axis];
            }
        }

        GyroBiasUpdate {
            corrected_raw,
            newly_calibrated,
            bias_raw: self.bias_raw,
        }
    }

    /// Advances calibration only for a newly received IMU sample.
    ///
    /// Repeated samples may still be bias-corrected for observation, but they
    /// must never count toward the stationary startup calibration window.
    pub fn update_if_fresh(&mut self, armed: bool, fresh: bool, raw: [i32; 3]) -> GyroBiasUpdate {
        if fresh {
            return self.update(armed, raw);
        }

        let mut corrected_raw = raw;
        if self.ready {
            for (axis, value) in corrected_raw.iter_mut().enumerate() {
                *value -= self.bias_raw[axis];
            }
        }

        GyroBiasUpdate {
            corrected_raw,
            newly_calibrated: false,
            bias_raw: self.bias_raw,
        }
    }
}

/// Maps logical mixer motors to physical actuator outputs.
///
/// Logical motor order follows Betaflight Quad X:
/// 1 = rear-right
/// 2 = front-right
/// 3 = rear-left
/// 4 = front-left
///
/// The values are one-based physical output numbers. For example, `[2, 1, 3, 4]`
/// sends logical motor 1 to physical output 2 and logical motor 2 to physical
/// output 1.
///
/// Bench mapping measured on 2026-07-12:
/// physical output 1 = front-left
/// physical output 2 = rear-left
/// physical output 3 = rear-right
/// physical output 4 = front-right
pub const MOTOR_OUTPUT_MAP: [usize; 4] = [3, 4, 2, 1];
pub const DSHOT_UNEQUAL_BENCH_TRIGGER_THROTTLE: f32 = 100.0;
pub const DSHOT_UNEQUAL_BENCH_LOGICAL_PATTERN: [f32; 4] = [140.0, 120.0, 100.0, 80.0];
pub const DSHOT_UNEQUAL_BENCH_MIN_COMMAND: u16 = 80;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RcCommand {
    pub roll_dps: f32,
    pub pitch_dps: f32,
    pub yaw_dps: f32,
    pub throttle: u32,
}

pub fn remap_rc_rate_channel(channel: u16) -> f32 {
    let channel = channel.clamp(RC_CHANNEL_MIN, RC_CHANNEL_MAX);
    let centered = channel as i32 - RC_CHANNEL_CENTER as i32;

    if centered.abs() <= RC_RATE_DEADBAND {
        return 0.0;
    }

    centered as f32 * RC_RATE_MAX_DPS / (RC_CHANNEL_MAX - RC_CHANNEL_CENTER) as f32
}

pub fn maybe_invert_rate(value: f32, invert: bool) -> f32 {
    if invert { -value } else { value }
}

pub fn remap_rc_throttle_channel(channel: u16) -> u32 {
    let channel = channel.clamp(RC_CHANNEL_MIN, RC_CHANNEL_MAX);
    let offset = if RC_INVERT_THROTTLE {
        (RC_CHANNEL_MAX - channel) as u32
    } else {
        (channel - RC_CHANNEL_MIN) as u32
    };
    let span = (RC_CHANNEL_MAX - RC_CHANNEL_MIN) as u32;

    offset * RC_THROTTLE_MAX / span
}

pub fn remap_rc_channels(roll: u16, pitch: u16, yaw: u16, throttle: u16) -> RcCommand {
    let channels = [roll, pitch, yaw];

    RcCommand {
        roll_dps: maybe_invert_rate(
            remap_rc_rate_channel(channels[RC_ROLL_CHANNEL_INDEX]),
            RC_INVERT_ROLL,
        ),
        pitch_dps: maybe_invert_rate(
            remap_rc_rate_channel(channels[RC_PITCH_CHANNEL_INDEX]),
            RC_INVERT_PITCH,
        ),
        yaw_dps: maybe_invert_rate(
            remap_rc_rate_channel(channels[RC_YAW_CHANNEL_INDEX]),
            RC_INVERT_YAW,
        ),
        throttle: remap_rc_throttle_channel(throttle),
    }
}

pub fn remap_motor_outputs(logical: [f32; 4]) -> [f32; 4] {
    let mut physical = [0.0; 4];
    let mut logical_index = 0;

    while logical_index < 4 {
        let physical_output = MOTOR_OUTPUT_MAP[logical_index];

        if (1..=4).contains(&physical_output) {
            physical[physical_output - 1] = logical[logical_index];
        }

        logical_index += 1;
    }

    physical
}

pub fn dshot_unequal_bench_motor_outputs(requested_throttle: f32) -> [f32; 4] {
    if !requested_throttle.is_finite() || requested_throttle < DSHOT_UNEQUAL_BENCH_TRIGGER_THROTTLE
    {
        return [0.0; 4];
    }

    remap_motor_outputs(DSHOT_UNEQUAL_BENCH_LOGICAL_PATTERN)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LowPassFilter {
    alpha: f32,
    value: f32,
    initialized: bool,
}

impl LowPassFilter {
    pub const fn new(alpha: f32) -> Self {
        Self {
            alpha,
            value: 0.0,
            initialized: false,
        }
    }

    pub fn update(&mut self, input: f32) -> f32 {
        if !self.initialized {
            self.value = input;
            self.initialized = true;
            return self.value;
        }

        self.value += self.alpha * (input - self.value);
        self.value
    }

    pub fn set_alpha(&mut self, alpha: f32) {
        self.alpha = clamp_unit_interval(alpha);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuRateLowPassFilter {
    roll: LowPassFilter,
    pitch: LowPassFilter,
    yaw: LowPassFilter,
}

impl ImuRateLowPassFilter {
    pub const fn new(alpha: f32) -> Self {
        Self {
            roll: LowPassFilter::new(alpha),
            pitch: LowPassFilter::new(alpha),
            yaw: LowPassFilter::new(alpha),
        }
    }

    pub fn update(&mut self, roll: f32, pitch: f32, yaw: f32) -> (f32, f32, f32) {
        (
            self.roll.update(roll),
            self.pitch.update(pitch),
            self.yaw.update(yaw),
        )
    }

    pub fn set_alpha(&mut self, alpha: f32) {
        self.roll.set_alpha(alpha);
        self.pitch.set_alpha(alpha);
        self.yaw.set_alpha(alpha);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GyroAngleIntegrator {
    roll: f32,
    pitch: f32,
    yaw: f32,
}

impl GyroAngleIntegrator {
    pub const fn new() -> Self {
        Self {
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
        }
    }

    pub fn update(
        &mut self,
        roll_rate: f32,
        pitch_rate: f32,
        yaw_rate: f32,
        dt_seconds: f32,
    ) -> [f32; 3] {
        self.roll += roll_rate * dt_seconds;
        self.pitch += pitch_rate * dt_seconds;
        self.yaw += yaw_rate * dt_seconds;

        [self.roll, self.pitch, self.yaw]
    }

    pub fn update_with_accel(
        &mut self,
        rates_dps: [f32; 3],
        accel_g: [f32; 3],
        dt_seconds: f32,
    ) -> [f32; 3] {
        let (acc_roll, acc_pitch) = accel_roll_pitch_degrees(accel_g[0], accel_g[1], accel_g[2]);

        self.roll += rates_dps[0] * dt_seconds;
        self.pitch += rates_dps[1] * dt_seconds;
        self.yaw += rates_dps[2] * dt_seconds;

        self.roll = self.roll * IMU_COMPLEMENTARY_GYRO_WEIGHT
            + acc_roll * (1.0 - IMU_COMPLEMENTARY_GYRO_WEIGHT);
        self.pitch = self.pitch * IMU_COMPLEMENTARY_GYRO_WEIGHT
            + acc_pitch * (1.0 - IMU_COMPLEMENTARY_GYRO_WEIGHT);

        [self.roll, self.pitch, self.yaw]
    }
}

impl Default for GyroAngleIntegrator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn accel_roll_pitch_degrees(acc_x: f32, acc_y: f32, acc_z: f32) -> (f32, f32) {
    let roll = fast_atan2(acc_y, acc_z) * RAD_TO_DEG;
    let pitch = fast_atan2(-acc_x, fast_sqrt(acc_y * acc_y + acc_z * acc_z)) * RAD_TO_DEG;

    (roll, pitch)
}

fn fast_sqrt(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }

    let mut x = value;
    let mut i = 0;
    while i < 5 {
        x = 0.5 * (x + value / x);
        i += 1;
    }

    x
}

fn fast_atan2(y: f32, x: f32) -> f32 {
    const PI: f32 = core::f32::consts::PI;
    const FRAC_PI_2: f32 = core::f32::consts::FRAC_PI_2;
    const FRAC_PI_4: f32 = core::f32::consts::FRAC_PI_4;

    if x == 0.0 {
        if y > 0.0 {
            return FRAC_PI_2;
        }
        if y < 0.0 {
            return -FRAC_PI_2;
        }
        return 0.0;
    }

    let abs_y = if y < 0.0 { -y } else { y };
    let (angle, r) = if x >= 0.0 {
        (FRAC_PI_4, (x - abs_y) / (x + abs_y))
    } else {
        (3.0 * FRAC_PI_4, (x + abs_y) / (abs_y - x))
    };
    let angle = angle - FRAC_PI_4 * r;

    if y < 0.0 { -angle } else { angle.min(PI) }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RcInput {
    roll: u16,
    pitch: u16,
    yaw: u16,
    arm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RateSetpoint {
    roll: f32,
    pitch: f32,
    yaw: f32,
}
impl Default for RateSetpoint {
    fn default() -> Self {
        Self {
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RateMeasured {
    roll: f32,
    pitch: f32,
    yaw: f32,
}
impl Default for RateMeasured {
    fn default() -> Self {
        Self {
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RateControllerOutput {
    roll: f32,
    pitch: f32,
    yaw: f32,
}
impl Default for RateControllerOutput {
    fn default() -> Self {
        Self {
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateAxisContributions {
    pub p: f32,
    pub i: f32,
    pub d: f32,
    pub ff: f32,
    pub total: f32,
    pub error: f32,
    pub setpoint_rate: f32,
    pub measurement_rate: f32,
}
impl RateAxisContributions {
    const fn zero() -> Self {
        Self {
            p: 0.0,
            i: 0.0,
            d: 0.0,
            ff: 0.0,
            total: 0.0,
            error: 0.0,
            setpoint_rate: 0.0,
            measurement_rate: 0.0,
        }
    }

    fn from_control_output(output: ControlOutput) -> Self {
        Self {
            p: output.p,
            i: output.i,
            d: output.d,
            ff: output.ff,
            total: output.output,
            error: output.error,
            setpoint_rate: output.setpoint_rate,
            measurement_rate: output.measurement_rate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateControllerContributions {
    pub roll: RateAxisContributions,
    pub pitch: RateAxisContributions,
    pub yaw: RateAxisContributions,
}
impl Default for RateControllerContributions {
    fn default() -> Self {
        Self {
            roll: RateAxisContributions::zero(),
            pitch: RateAxisContributions::zero(),
            yaw: RateAxisContributions::zero(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MotorCommands {
    /// Logical motor 1, rear-right.
    pub motor1: f32,
    /// Logical motor 2, front-right.
    pub motor2: f32,
    /// Logical motor 3, rear-left.
    pub motor3: f32,
    /// Logical motor 4, front-left.
    pub motor4: f32,
}
impl Default for MotorCommands {
    /// Default throttle values for motors.
    fn default() -> Self {
        Self {
            motor1: 0.0,
            motor2: 0.0,
            motor3: 0.0,
            motor4: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PidGains {
    pub p: f32,
    pub i: f32,
    pub d: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TuningProfile {
    pub rate_gains: RateControllerGains,
    pub imu_lpf_alpha: f32,
}

impl TuningProfile {
    pub const fn default_first_hop() -> Self {
        Self {
            rate_gains: RateControllerGains {
                roll: PidGains {
                    p: 0.2,
                    i: 0.0,
                    d: 0.0,
                },
                pitch: PidGains {
                    p: 0.25,
                    i: 0.0,
                    d: 0.0,
                },
                yaw: PidGains {
                    p: 0.3,
                    i: 0.04,
                    d: 0.0,
                },
            },
            imu_lpf_alpha: IMU_GYRO_LPF_ALPHA,
        }
    }

    pub const fn default_bench() -> Self {
        Self {
            rate_gains: RateControllerGains {
                roll: PidGains {
                    p: 10.0,
                    i: 0.0,
                    d: 0.0,
                },
                pitch: PidGains {
                    p: 10.0,
                    i: 0.0,
                    d: 0.0,
                },
                yaw: PidGains {
                    p: 10.0,
                    i: 0.0,
                    d: 0.0,
                },
            },
            imu_lpf_alpha: IMU_GYRO_LPF_ALPHA,
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.rate_gains.roll = sanitize_pid_gains(self.rate_gains.roll);
        self.rate_gains.pitch = sanitize_pid_gains(self.rate_gains.pitch);
        self.rate_gains.yaw = sanitize_pid_gains(self.rate_gains.yaw);
        self.imu_lpf_alpha = clamp_unit_interval(self.imu_lpf_alpha);
        self
    }
}

fn sanitize_pid_gains(gains: PidGains) -> PidGains {
    PidGains {
        p: clamp_gain(gains.p, 0.0, 20.0),
        i: clamp_gain(gains.i, 0.0, 20.0),
        d: clamp_gain(gains.d, 0.0, 20.0),
    }
}

fn clamp_gain(value: f32, min: f32, max: f32) -> f32 {
    if !value.is_finite() {
        min
    } else {
        value.clamp(min, max)
    }
}

fn clamp_unit_interval(value: f32) -> f32 {
    if !value.is_finite() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RateControllerFeedforwardGains {
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
}
impl Default for RateControllerFeedforwardGains {
    fn default() -> Self {
        Self {
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateControllerGains {
    pub roll: PidGains,
    pub pitch: PidGains,
    pub yaw: PidGains,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateController {
    roll: Pid,
    pitch: Pid,
    yaw: Pid,
}
impl RateController {
    pub fn new(gains: RateControllerGains, pid_output_limit: f32) -> Self {
        Self::new_with_feedforward(
            gains,
            RateControllerFeedforwardGains::default(),
            pid_output_limit,
        )
    }

    pub fn new_with_feedforward(
        gains: RateControllerGains,
        feedforward: RateControllerFeedforwardGains,
        pid_output_limit: f32,
    ) -> Self {
        let mut roll = Pid::new(0.0_f32, pid_output_limit);
        roll.p(gains.roll.p, pid_output_limit)
            .i(gains.roll.i, pid_output_limit)
            .d(gains.roll.d, pid_output_limit)
            .ff(feedforward.roll, pid_output_limit)
            .d_filter_alpha(RATE_CONTROLLER_D_FILTER_ALPHA)
            .i_relax_setpoint_rate(RATE_CONTROLLER_I_RELAX_SETPOINT_RATE_DPS);

        let mut pitch = Pid::new(0.0_f32, pid_output_limit);
        pitch
            .p(gains.pitch.p, pid_output_limit)
            .i(gains.pitch.i, pid_output_limit)
            .d(gains.pitch.d, pid_output_limit)
            .ff(feedforward.pitch, pid_output_limit)
            .d_filter_alpha(RATE_CONTROLLER_D_FILTER_ALPHA)
            .i_relax_setpoint_rate(RATE_CONTROLLER_I_RELAX_SETPOINT_RATE_DPS);

        let mut yaw = Pid::new(0.0_f32, pid_output_limit);
        yaw.p(gains.yaw.p, pid_output_limit)
            .i(gains.yaw.i, pid_output_limit)
            .d(gains.yaw.d, pid_output_limit)
            .ff(feedforward.yaw, pid_output_limit)
            .d_filter_alpha(RATE_CONTROLLER_D_FILTER_ALPHA)
            .i_relax_setpoint_rate(RATE_CONTROLLER_I_RELAX_SETPOINT_RATE_DPS);

        Self { roll, pitch, yaw }
    }
}

// --- FLIGHT CONTROLLER ---
#[derive(Debug, Clone, Copy)]
pub struct FlightControllerConfig {
    /// Maximum allowed motor command, usually 1.0
    pub max_throttle: f32,
    /// If true, reduce roll/pitch/yaw contribution so no motor exceeds max_throttle
    pub rescale_throttles: bool,
    /// If true, clamp negative motor commands to 0.0
    pub clamp_negative_to_zero: bool,
}
impl Default for FlightControllerConfig {
    fn default() -> Self {
        Self {
            max_throttle: 2000.0,
            rescale_throttles: true,
            clamp_negative_to_zero: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FlightController {
    throttle: f32,
    rate_setpoint: RateSetpoint,
    rate_measured: RateMeasured,
    rate_controllers: RateController,
    rate_controller_output: RateControllerOutput,
    rate_controller_contributions: RateControllerContributions,
    motor_commands: MotorCommands,
    config: FlightControllerConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateBlackboxSample {
    pub setpoint: [f32; 3],
    pub measurement: [f32; 3],
    pub error: [f32; 3],
    pub p: [f32; 3],
    pub i: [f32; 3],
    pub d: [f32; 3],
    pub ff: [f32; 3],
    pub pid: [f32; 3],
    pub motors: [f32; 4],
    pub throttle: f32,
}

pub const RATE_BLACKBOX_SCHEMA_VERSION: u8 = 2;
pub const RATE_BLACKBOX_FLAG_ARMED: u8 = 1 << 0;
pub const RATE_BLACKBOX_FLAG_IMU_FRESH: u8 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactRateBlackboxSample {
    pub seq: u32,
    pub imu_seq: u32,
    pub flags: u8,
    /// Unfiltered gyro rates in 0.1 deg/s units: roll, pitch, yaw.
    pub raw_gyro_dps10: [i16; 3],
    /// Filtered gyro rates in 0.1 deg/s units: roll, pitch, yaw.
    pub gyro_dps10: [i16; 3],
    /// Commanded rates in 0.1 deg/s units: roll, pitch, yaw.
    pub command_dps10: [i16; 3],
    /// Total PID output in controller output units: roll, pitch, yaw.
    pub pid: [i16; 3],
    /// Current throttle setpoint in PWM-style command units.
    pub throttle: u16,
    /// Mixed motor commands in PWM-style command units.
    pub motors: [u16; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactRateBlackboxFields {
    pub seq: u32,
    pub imu_seq: u32,
    pub armed: bool,
    pub imu_fresh: bool,
    pub raw_gyro_dps: [f32; 3],
    pub filtered_gyro_dps: [f32; 3],
    pub command_dps: [f32; 3],
    pub pid: [f32; 3],
    pub throttle: f32,
    pub motors: [f32; 4],
}

impl CompactRateBlackboxSample {
    pub fn from_fields(fields: CompactRateBlackboxFields) -> Self {
        let mut flags = 0;
        if fields.armed {
            flags |= RATE_BLACKBOX_FLAG_ARMED;
        }
        if fields.imu_fresh {
            flags |= RATE_BLACKBOX_FLAG_IMU_FRESH;
        }

        Self {
            seq: fields.seq,
            imu_seq: fields.imu_seq,
            flags,
            raw_gyro_dps10: [
                f32_to_i16_scaled(fields.raw_gyro_dps[0], 10.0),
                f32_to_i16_scaled(fields.raw_gyro_dps[1], 10.0),
                f32_to_i16_scaled(fields.raw_gyro_dps[2], 10.0),
            ],
            gyro_dps10: [
                f32_to_i16_scaled(fields.filtered_gyro_dps[0], 10.0),
                f32_to_i16_scaled(fields.filtered_gyro_dps[1], 10.0),
                f32_to_i16_scaled(fields.filtered_gyro_dps[2], 10.0),
            ],
            command_dps10: [
                f32_to_i16_scaled(fields.command_dps[0], 10.0),
                f32_to_i16_scaled(fields.command_dps[1], 10.0),
                f32_to_i16_scaled(fields.command_dps[2], 10.0),
            ],
            pid: [
                f32_to_i16_scaled(fields.pid[0], 1.0),
                f32_to_i16_scaled(fields.pid[1], 1.0),
                f32_to_i16_scaled(fields.pid[2], 1.0),
            ],
            throttle: f32_to_u16_scaled(fields.throttle, 1.0),
            motors: [
                f32_to_u16_scaled(fields.motors[0], 1.0),
                f32_to_u16_scaled(fields.motors[1], 1.0),
                f32_to_u16_scaled(fields.motors[2], 1.0),
                f32_to_u16_scaled(fields.motors[3], 1.0),
            ],
        }
    }

    pub fn from_rate_sample(
        seq: u32,
        imu_seq: u32,
        armed: bool,
        imu_fresh: bool,
        raw_gyro_dps: [f32; 3],
        sample: RateBlackboxSample,
    ) -> Self {
        Self::from_fields(CompactRateBlackboxFields {
            seq,
            imu_seq,
            armed,
            imu_fresh,
            raw_gyro_dps,
            filtered_gyro_dps: sample.measurement,
            command_dps: sample.setpoint,
            pid: sample.pid,
            throttle: sample.throttle,
            motors: sample.motors,
        })
    }
}

#[cfg(feature = "blackbox_defmt")]
pub fn emit_rate_blackbox(sample: CompactRateBlackboxSample) {
    defmt::info!(
        "BB{} seq {} imu {} flags {} raw10 [{}, {}, {}] gyro10 [{}, {}, {}] cmd10 [{}, {}, {}] pid [{}, {}, {}] thr {} motors [{}, {}, {}, {}]",
        RATE_BLACKBOX_SCHEMA_VERSION,
        sample.seq,
        sample.imu_seq,
        sample.flags,
        sample.raw_gyro_dps10[0],
        sample.raw_gyro_dps10[1],
        sample.raw_gyro_dps10[2],
        sample.gyro_dps10[0],
        sample.gyro_dps10[1],
        sample.gyro_dps10[2],
        sample.command_dps10[0],
        sample.command_dps10[1],
        sample.command_dps10[2],
        sample.pid[0],
        sample.pid[1],
        sample.pid[2],
        sample.throttle,
        sample.motors[0],
        sample.motors[1],
        sample.motors[2],
        sample.motors[3]
    );
}

fn f32_to_i16_scaled(value: f32, scale: f32) -> i16 {
    let scaled = value * scale;
    if scaled >= i16::MAX as f32 {
        i16::MAX
    } else if scaled <= i16::MIN as f32 {
        i16::MIN
    } else if scaled >= 0.0 {
        (scaled + 0.5) as i16
    } else {
        (scaled - 0.5) as i16
    }
}

fn f32_to_u16_scaled(value: f32, scale: f32) -> u16 {
    let scaled = value * scale;
    if scaled >= u16::MAX as f32 {
        u16::MAX
    } else if scaled <= 0.0 {
        0
    } else {
        (scaled + 0.5) as u16
    }
}

impl FlightController {
    /// Creates new flight controller object.
    pub fn new(config: FlightControllerConfig, rate_controllers: RateController) -> Self {
        Self {
            throttle: 0.0,
            rate_setpoint: RateSetpoint::default(),
            rate_measured: RateMeasured::default(),
            rate_controller_output: RateControllerOutput::default(),
            rate_controller_contributions: RateControllerContributions::default(),
            motor_commands: MotorCommands::default(),
            rate_controllers,
            config,
        }
    }

    pub fn apply_tuning_profile(&mut self, profile: TuningProfile) {
        let profile = profile.sanitized();
        self.rate_controllers =
            RateController::new(profile.rate_gains, RATE_CONTROLLER_OUTPUT_LIMIT);
        self.rate_controller_output = RateControllerOutput::default();
        self.rate_controller_contributions = RateControllerContributions::default();
    }
    /// Updates setpoint for the roll, pitch and yaw rate.
    pub fn update_attitude_rate_setpoint(&mut self, roll: f32, pitch: f32, yaw: f32) {
        // Update struct
        self.rate_setpoint = RateSetpoint { roll, pitch, yaw };
        // Update PID controllers
        self.rate_controllers.roll.set_setpoint(roll);
        self.rate_controllers.pitch.set_setpoint(pitch);
        self.rate_controllers.yaw.set_setpoint(yaw);
    }
    /// Update setpoint for the throttle.
    pub fn update_throttle_setpoint(&mut self, setpoint: f32) {
        self.throttle = setpoint;
    }
    /// Update rate measurements.
    pub fn update_rate_measured(&mut self, roll: f32, pitch: f32, yaw: f32) {
        self.rate_measured = RateMeasured { roll, pitch, yaw };
    }
    /// Updates motor commands based on setpoints and measured values.
    pub fn update_motor_commands(&mut self) {
        self.update_motor_commands_dt(CONTROL_LOOP_DT_SECONDS);
    }

    pub fn update_motor_commands_dt(&mut self, dt_seconds: f32) {
        // Calcualte roll, pitch, yaw controller commands
        self.next_control_output(dt_seconds);

        // Get motor commands from mixer.
        self.quad_motor_mixer();
    }
    /// Gets current motor commands from the motor_commands struct and returns them as a tuple (motor1, motor2, motor3, motor4).
    pub fn get_motor_commands(&mut self) -> [f32; 4] {
        remap_motor_outputs(self.get_logical_motor_commands())
    }

    pub fn get_logical_motor_commands(&self) -> [f32; 4] {
        [
            self.motor_commands.motor1,
            self.motor_commands.motor2,
            self.motor_commands.motor3,
            self.motor_commands.motor4,
        ]
    }

    pub fn get_rate_controller_output(&self) -> [f32; 3] {
        [
            self.rate_controller_output.roll,
            self.rate_controller_output.pitch,
            self.rate_controller_output.yaw,
        ]
    }

    pub fn get_rate_controller_contributions(&self) -> RateControllerContributions {
        self.rate_controller_contributions
    }

    pub fn blackbox_sample(&self) -> RateBlackboxSample {
        let motors = remap_motor_outputs([
            self.motor_commands.motor1,
            self.motor_commands.motor2,
            self.motor_commands.motor3,
            self.motor_commands.motor4,
        ]);

        RateBlackboxSample {
            setpoint: [
                self.rate_setpoint.roll,
                self.rate_setpoint.pitch,
                self.rate_setpoint.yaw,
            ],
            measurement: [
                self.rate_measured.roll,
                self.rate_measured.pitch,
                self.rate_measured.yaw,
            ],
            error: [
                self.rate_controller_contributions.roll.error,
                self.rate_controller_contributions.pitch.error,
                self.rate_controller_contributions.yaw.error,
            ],
            p: [
                self.rate_controller_contributions.roll.p,
                self.rate_controller_contributions.pitch.p,
                self.rate_controller_contributions.yaw.p,
            ],
            i: [
                self.rate_controller_contributions.roll.i,
                self.rate_controller_contributions.pitch.i,
                self.rate_controller_contributions.yaw.i,
            ],
            d: [
                self.rate_controller_contributions.roll.d,
                self.rate_controller_contributions.pitch.d,
                self.rate_controller_contributions.yaw.d,
            ],
            ff: [
                self.rate_controller_contributions.roll.ff,
                self.rate_controller_contributions.pitch.ff,
                self.rate_controller_contributions.yaw.ff,
            ],
            pid: self.get_rate_controller_output(),
            motors,
            throttle: self.throttle,
        }
    }

    // Helper functions
    /// Calculates the control output for roll, pitch and yaw.
    fn next_control_output(&mut self, dt_seconds: f32) {
        let roll = self
            .rate_controllers
            .roll
            .next_control_output_dt(self.rate_measured.roll, dt_seconds);
        let pitch = self
            .rate_controllers
            .pitch
            .next_control_output_dt(self.rate_measured.pitch, dt_seconds);
        let yaw = self
            .rate_controllers
            .yaw
            .next_control_output_dt(self.rate_measured.yaw, dt_seconds);

        self.rate_controller_output = RateControllerOutput {
            roll: roll.output,
            pitch: pitch.output,
            yaw: yaw.output,
        };
        self.rate_controller_contributions = RateControllerContributions {
            roll: RateAxisContributions::from_control_output(roll),
            pitch: RateAxisContributions::from_control_output(pitch),
            yaw: RateAxisContributions::from_control_output(yaw),
        };
    }
    /// Rescales individual motor throttle values to be equal or less than global throttle.
    fn rescale_axes(
        &self,
        mut throttle: f32,
        max_throttle: f32,
        a1: f32,
        a2: f32,
        a3: f32,
        a4: f32,
    ) -> (f32, f32, f32, f32, f32) {
        // If max throttle is enabled, limit throttle
        if self.config.rescale_throttles && max_throttle < throttle {
            throttle = max_throttle;
        }

        let max_axis = a1.max(a2).max(a3).max(a4);
        let min_axis = a1.min(a2).min(a3).min(a4);

        let up_room = max_throttle - throttle;
        let down_room = throttle;

        let mut scale: f32 = 1.0;

        if max_axis > 0.0 {
            scale = scale.min(up_room / max_axis);
        }

        if min_axis < 0.0 {
            scale = scale.min(down_room / -min_axis);
        }

        let scale = scale.clamp(0.0, 1.0);

        (throttle, a1 * scale, a2 * scale, a3 * scale, a4 * scale)
    }
    /// Takes attitude commands and translats to motor throttle commands.
    fn quad_motor_mixer(&mut self) {
        // Aliases
        let roll = self.rate_controller_output.roll;
        let pitch = self.rate_controller_output.pitch;
        let yaw = self.rate_controller_output.yaw;
        let mut throttle = self.throttle;

        // Quad X axis mix
        let mut a1 = -roll + pitch - yaw; // motor1 rear-right
        let mut a2 = -roll - pitch + yaw; // motor2 front-right
        let mut a3 = roll + pitch + yaw; // motor3 rear-left
        let mut a4 = roll - pitch - yaw; // motor4 front-left

        // If rescaling is enabled, rescale motor commands
        (throttle, a1, a2, a3, a4) =
            self.rescale_axes(throttle, self.config.max_throttle, a1, a2, a3, a4);

        // Store motor commands.
        self.motor_commands = MotorCommands {
            motor1: throttle + a1,
            motor2: throttle + a2,
            motor3: throttle + a3,
            motor4: throttle + a4,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "actual {actual} != expected {expected}"
        );
    }

    fn test_controller() -> FlightController {
        FlightController::new(
            FlightControllerConfig::default(),
            RateController::new(
                RateControllerGains {
                    roll: PidGains {
                        p: 1.0,
                        i: 0.0,
                        d: 0.0,
                    },
                    pitch: PidGains {
                        p: 1.0,
                        i: 0.0,
                        d: 0.0,
                    },
                    yaw: PidGains {
                        p: 1.0,
                        i: 0.0,
                        d: 0.0,
                    },
                },
                500.0,
            ),
        )
    }

    #[test]
    fn rc_mapping_clamps_centers_and_applies_axis_inversion() {
        assert_close(remap_rc_rate_channel(RC_CHANNEL_CENTER), 0.0);
        assert_close(remap_rc_rate_channel(RC_CHANNEL_CENTER + 5), 0.0);
        assert_close(remap_rc_rate_channel(RC_CHANNEL_CENTER - 5), 0.0);
        assert_close(remap_rc_rate_channel(RC_CHANNEL_MAX), RC_RATE_MAX_DPS);
        assert_close(remap_rc_rate_channel(RC_CHANNEL_MIN), -RC_RATE_MAX_DPS);
        assert_close(remap_rc_rate_channel(u16::MAX), RC_RATE_MAX_DPS);
        assert_eq!(remap_rc_throttle_channel(RC_CHANNEL_MIN), 0);
        assert_eq!(remap_rc_throttle_channel(RC_CHANNEL_MAX), RC_THROTTLE_MAX);

        let cmd = remap_rc_channels(
            RC_CHANNEL_CENTER + 80,
            RC_CHANNEL_CENTER + 160,
            RC_CHANNEL_CENTER + 240,
            RC_CHANNEL_CENTER,
        );

        assert_close(cmd.roll_dps, 100.0);
        assert_close(cmd.pitch_dps, 200.0);
        assert_close(cmd.yaw_dps, 300.0);
        assert_eq!(cmd.throttle, 1000);
    }

    #[test]
    fn rc_stick_commands_drive_expected_physical_motor_pairs() {
        let mut controller = test_controller();

        let roll_right = remap_rc_channels(
            RC_CHANNEL_CENTER + 80,
            RC_CHANNEL_CENTER,
            RC_CHANNEL_CENTER,
            RC_CHANNEL_CENTER,
        );
        controller.update_throttle_setpoint(1000.0);
        controller.update_attitude_rate_setpoint(roll_right.roll_dps, 0.0, 0.0);
        controller.update_rate_measured(0.0, 0.0, 0.0);
        controller.update_motor_commands();
        assert_eq!(
            controller.get_motor_commands(),
            [1100.0, 1100.0, 900.0, 900.0]
        );

        let pitch_forward = remap_rc_channels(
            RC_CHANNEL_CENTER,
            RC_CHANNEL_CENTER + 80,
            RC_CHANNEL_CENTER,
            RC_CHANNEL_CENTER,
        );
        controller.update_attitude_rate_setpoint(0.0, pitch_forward.pitch_dps, 0.0);
        controller.update_motor_commands();
        assert_eq!(
            controller.get_motor_commands(),
            [900.0, 1100.0, 1100.0, 900.0]
        );

        let yaw_right = remap_rc_channels(
            RC_CHANNEL_CENTER,
            RC_CHANNEL_CENTER,
            RC_CHANNEL_CENTER + 80,
            RC_CHANNEL_CENTER,
        );
        controller.update_attitude_rate_setpoint(0.0, 0.0, yaw_right.yaw_dps);
        controller.update_motor_commands();
        assert_eq!(
            controller.get_motor_commands(),
            [900.0, 1100.0, 900.0, 1100.0]
        );
    }

    #[test]
    fn low_pass_filter_initializes_to_first_sample_then_smooths() {
        let mut filter = LowPassFilter::new(0.5);

        assert_close(filter.update(10.0), 10.0);
        assert_close(filter.update(20.0), 15.0);
        assert_close(filter.update(15.0), 15.0);
    }

    #[test]
    fn imu_rate_filter_tracks_axes_independently() {
        let mut filter = ImuRateLowPassFilter::new(0.25);

        assert_eq!(filter.update(4.0, 8.0, 12.0), (4.0, 8.0, 12.0));
        let (roll, pitch, yaw) = filter.update(8.0, 0.0, 20.0);

        assert_close(roll, 5.0);
        assert_close(pitch, 6.0);
        assert_close(yaw, 14.0);
    }

    #[test]
    fn gyro_integrator_accumulates_rates_and_uses_accel_correction() {
        let mut integrator = GyroAngleIntegrator::new();

        assert_eq!(
            integrator.update(100.0, -50.0, 25.0, 0.01),
            [1.0, -0.5, 0.25]
        );

        let corrected = integrator.update_with_accel([0.0; 3], [0.0, 1.0, 0.0], 0.01);

        assert_close(corrected[0], 2.78);
        assert_close(corrected[1], -0.49);
        assert_close(corrected[2], 0.25);
    }

    #[test]
    fn gyro_axis_map_preserves_current_control_convention() {
        let map = GyroAxisMap::new([1, 0, 2], [1, -1, -1]);

        assert_eq!(map.map_raw([10, 20, -30]), [20, -10, 30]);
    }

    #[test]
    fn gyro_bias_calibrator_resets_on_motion_and_subtracts_bias() {
        let mut calibrator = GyroBiasCalibrator::new(3, 1000);

        assert_eq!(
            calibrator.update(false, [10, -20, 30]).corrected_raw,
            [10, -20, 30]
        );
        calibrator.update(false, [2000, 0, 0]);
        assert!(!calibrator.ready());

        calibrator.update(false, [12, -18, 33]);
        calibrator.update(false, [14, -16, 36]);
        let update = calibrator.update(false, [16, -14, 39]);

        assert!(update.newly_calibrated);
        assert_eq!(update.bias_raw, [14, -16, 36]);
        assert_eq!(update.corrected_raw, [2, 2, 3]);
        assert_eq!(
            calibrator.update(true, [20, -10, 40]).corrected_raw,
            [6, 6, 4]
        );
    }

    #[test]
    fn stale_samples_do_not_advance_gyro_bias_calibration() {
        let mut calibrator = GyroBiasCalibrator::new(2, 1000);

        for _ in 0..10 {
            let update = calibrator.update_if_fresh(false, false, [10, -20, 30]);
            assert!(!update.newly_calibrated);
        }
        assert!(!calibrator.ready());

        assert!(
            !calibrator
                .update_if_fresh(false, true, [10, -20, 30])
                .newly_calibrated
        );
        assert!(
            calibrator
                .update_if_fresh(false, true, [12, -18, 32])
                .newly_calibrated
        );
        assert_eq!(calibrator.bias_raw(), [11, -19, 31]);
    }

    #[test]
    fn accelerometer_roll_pitch_have_expected_static_orientations() {
        let (roll, pitch) = accel_roll_pitch_degrees(0.0, 0.0, 1.0);
        assert_close(roll, 0.0);
        assert_close(pitch, 0.0);

        let (roll, pitch) = accel_roll_pitch_degrees(0.0, 1.0, 0.0);
        assert_close(roll, 90.0);
        assert_close(pitch, 0.0);
    }

    #[test]
    fn first_hop_profile_uses_conservative_yaw_integral() {
        let profile = TuningProfile::default_first_hop();

        assert_close(profile.rate_gains.roll.p, 0.2);
        assert_close(profile.rate_gains.pitch.p, 0.25);
        assert_close(profile.rate_gains.yaw.p, 0.3);
        assert_close(profile.rate_gains.roll.i, 0.0);
        assert_close(profile.rate_gains.pitch.i, 0.0);
        assert_close(profile.rate_gains.yaw.i, 0.04);
        assert_close(profile.rate_gains.roll.d, 0.0);
        assert_close(profile.rate_gains.pitch.d, 0.0);
        assert_close(profile.rate_gains.yaw.d, 0.0);
    }

    #[test]
    fn pid_zero_error_keeps_all_motors_at_throttle() {
        let mut controller = test_controller();

        controller.update_throttle_setpoint(1000.0);
        controller.update_attitude_rate_setpoint(0.0, 0.0, 0.0);
        controller.update_rate_measured(0.0, 0.0, 0.0);
        controller.update_motor_commands();

        assert_eq!(controller.get_rate_controller_output(), [0.0, 0.0, 0.0]);
        assert_eq!(controller.get_motor_commands(), [1000.0; 4]);
    }

    #[test]
    fn motor_output_map_matches_measured_bench_order() {
        assert_eq!(
            remap_motor_outputs([10.0, 20.0, 30.0, 40.0]),
            [40.0, 30.0, 10.0, 20.0]
        );
    }

    #[test]
    fn dshot_unequal_bench_pattern_is_bounded_remapped_and_triggered() {
        assert_eq!(
            DSHOT_UNEQUAL_BENCH_LOGICAL_PATTERN
                .into_iter()
                .fold(f32::INFINITY, f32::min),
            DSHOT_UNEQUAL_BENCH_MIN_COMMAND as f32
        );
        assert_eq!(dshot_unequal_bench_motor_outputs(f32::NAN), [0.0; 4]);
        assert_eq!(
            dshot_unequal_bench_motor_outputs(DSHOT_UNEQUAL_BENCH_TRIGGER_THROTTLE - 1.0),
            [0.0; 4]
        );
        assert_eq!(
            dshot_unequal_bench_motor_outputs(DSHOT_UNEQUAL_BENCH_TRIGGER_THROTTLE),
            [80.0, 100.0, 140.0, 120.0]
        );
        assert!(
            dshot_unequal_bench_motor_outputs(f32::MAX)
                .into_iter()
                .all(|value| value <= 140.0)
        );
    }

    #[test]
    fn pid_roll_correction_mixes_to_opposite_motor_pairs() {
        let mut controller = test_controller();

        controller.update_throttle_setpoint(1000.0);
        controller.update_attitude_rate_setpoint(100.0, 0.0, 0.0);
        controller.update_rate_measured(0.0, 0.0, 0.0);
        controller.update_motor_commands();

        assert_eq!(controller.get_rate_controller_output(), [100.0, 0.0, 0.0]);
        assert_eq!(
            controller.get_logical_motor_commands(),
            [900.0, 900.0, 1100.0, 1100.0]
        );
        assert_eq!(
            controller.get_motor_commands(),
            [1100.0, 1100.0, 900.0, 900.0]
        );
    }

    #[test]
    fn measured_nose_up_pitch_rate_commands_rear_motors_up() {
        let mut controller = test_controller();

        controller.update_throttle_setpoint(1000.0);
        controller.update_attitude_rate_setpoint(0.0, 0.0, 0.0);
        // The board-level gyro mapping must present a physical nose-up
        // disturbance as negative pitch rate to this controller convention.
        controller.update_rate_measured(0.0, -100.0, 0.0);
        controller.update_motor_commands();

        assert_eq!(controller.get_rate_controller_output(), [0.0, 100.0, 0.0]);
        assert_eq!(
            controller.get_logical_motor_commands(),
            [1100.0, 900.0, 1100.0, 900.0]
        );
        assert_eq!(
            controller.get_motor_commands(),
            [900.0, 1100.0, 1100.0, 900.0]
        );
    }

    #[test]
    fn measured_nose_down_pitch_rate_commands_front_motors_up() {
        let mut controller = test_controller();

        controller.update_throttle_setpoint(1000.0);
        controller.update_attitude_rate_setpoint(0.0, 0.0, 0.0);
        controller.update_rate_measured(0.0, 100.0, 0.0);
        controller.update_motor_commands();

        assert_eq!(controller.get_rate_controller_output(), [0.0, -100.0, 0.0]);
        assert_eq!(
            controller.get_logical_motor_commands(),
            [900.0, 1100.0, 900.0, 1100.0]
        );
    }

    #[test]
    fn pid_contributions_are_exposed_per_axis() {
        let mut controller = test_controller();

        controller.update_attitude_rate_setpoint(100.0, -50.0, 25.0);
        controller.update_rate_measured(25.0, 25.0, -25.0);
        controller.update_motor_commands();

        let terms = controller.get_rate_controller_contributions();

        assert_eq!(terms.roll.p, 75.0);
        assert_eq!(terms.roll.i, 0.0);
        assert_eq!(terms.roll.d, 0.0);
        assert_eq!(terms.roll.total, 75.0);
        assert_eq!(terms.roll.error, 75.0);
        assert_eq!(terms.pitch.total, -75.0);
        assert_eq!(terms.yaw.total, 50.0);
    }

    #[test]
    fn feedforward_and_blackbox_sample_track_rate_setpoint_changes() {
        let mut controller = FlightController::new(
            FlightControllerConfig::default(),
            RateController::new_with_feedforward(
                RateControllerGains {
                    roll: PidGains {
                        p: 0.0,
                        i: 0.0,
                        d: 0.0,
                    },
                    pitch: PidGains {
                        p: 0.0,
                        i: 0.0,
                        d: 0.0,
                    },
                    yaw: PidGains {
                        p: 0.0,
                        i: 0.0,
                        d: 0.0,
                    },
                },
                RateControllerFeedforwardGains {
                    roll: 0.5,
                    pitch: 0.0,
                    yaw: 0.0,
                },
                500.0,
            ),
        );

        controller.update_throttle_setpoint(1000.0);
        controller.update_attitude_rate_setpoint(0.0, 0.0, 0.0);
        controller.update_rate_measured(0.0, 0.0, 0.0);
        controller.update_motor_commands_dt(0.1);
        controller.update_attitude_rate_setpoint(20.0, 0.0, 0.0);
        controller.update_motor_commands_dt(0.1);

        let sample = controller.blackbox_sample();

        assert_eq!(sample.setpoint, [20.0, 0.0, 0.0]);
        assert_eq!(sample.measurement, [0.0, 0.0, 0.0]);
        assert_eq!(sample.ff, [100.0, 0.0, 0.0]);
        assert_eq!(sample.pid, [100.0, 0.0, 0.0]);
        assert_eq!(sample.throttle, 1000.0);
    }

    #[test]
    fn mixer_rescales_axes_to_preserve_actuator_bounds() {
        let mut controller = test_controller();

        controller.update_throttle_setpoint(1900.0);
        controller.update_attitude_rate_setpoint(500.0, 0.0, 0.0);
        controller.update_rate_measured(0.0, 0.0, 0.0);
        controller.update_motor_commands();

        assert_eq!(
            controller.get_motor_commands(),
            [2000.0, 2000.0, 1800.0, 1800.0]
        );
    }
}
