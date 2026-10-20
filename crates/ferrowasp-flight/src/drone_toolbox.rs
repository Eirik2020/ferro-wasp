//! The controller's former home: re-exports `ferrowasp-flight-control`'s
//! drone_toolbox under its old path, and holds the compact rate log record
//! until it moves to the protocol crate.

#![allow(dead_code)]
pub use ferrowasp_flight_control::drone_toolbox::*;

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
pub fn emit_compact_blackbox(
    seq: u32,
    imu_seq: u32,
    armed: bool,
    imu_fresh: bool,
    raw_gyro_dps: [f32; 3],
    filtered_gyro_dps: [f32; 3],
    command_dps: [f32; 3],
    pid: [f32; 3],
    throttle: f32,
    motors: [f32; 4],
) {
    emit_rate_blackbox(CompactRateBlackboxSample::from_fields(
        CompactRateBlackboxFields {
            seq,
            imu_seq,
            armed,
            imu_fresh,
            raw_gyro_dps,
            filtered_gyro_dps,
            command_dps,
            pid,
            throttle,
            motors,
        },
    ));
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
