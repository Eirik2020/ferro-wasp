//! Latest-value snapshots shared between tasks and diagnostics.
//!
//! One static per value for the whole firmware: a task that writes one and a
//! task that reads it must name the same static, so a firmware re-exports
//! these by name and never defines its own.

use core::sync::atomic::{
    AtomicBool, AtomicI16, AtomicI32, AtomicU8, AtomicU16, AtomicU32, Ordering,
};

pub static IMU_LATEST_SEQ: AtomicU32 = AtomicU32::new(0);
pub static IMU_LATEST_ROLL_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_LATEST_PITCH_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_LATEST_YAW_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_STALE: AtomicBool = AtomicBool::new(true);
/// Isolated stale control ticks the loop has ridden through since boot.
pub static IMU_TOLERATED_STALE_TICKS: AtomicU32 = AtomicU32::new(0);
pub static CONTROL_ISR_SEQ: AtomicU32 = AtomicU32::new(0);
pub static CONTROL_RATE_SEQ: AtomicU32 = AtomicU32::new(0);
pub static CONTROL_ROLL_RAW: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_PITCH_RAW: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_YAW_RAW: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_ROLL_DPS10: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_PITCH_DPS10: AtomicI32 = AtomicI32::new(0);
pub static CONTROL_YAW_DPS10: AtomicI32 = AtomicI32::new(0);

pub static RC_ARM_HIGH: AtomicBool = AtomicBool::new(false);
pub static RC_THROTTLE: AtomicU32 = AtomicU32::new(0);
pub static SAFETY_ARMED: AtomicBool = AtomicBool::new(false);
/// The mixer's logical-to-physical motor map as four digits, published by the
/// control loop; `0` until it first runs. Bench motor requests use it so a
/// test spins the motor the mixer would.
pub static MOTOR_OUTPUT_MAP_SNAPSHOT: AtomicU16 = AtomicU16::new(0);
/// Estimated attitude, tenths of a degree: roll, pitch, and yaw.
pub static ATTITUDE_ROLL_DEG10: AtomicI16 = AtomicI16::new(0);
pub static ATTITUDE_PITCH_DEG10: AtomicI16 = AtomicI16::new(0);
pub static ATTITUDE_YAW_DEG10: AtomicI16 = AtomicI16::new(0);
/// Bit per physical DShot lane currently commanded above zero.
pub static DSHOT_ACTIVE_LANES: AtomicU8 = AtomicU8::new(0);
/// When the DShot service last started a frame set, in wrapping
/// milliseconds. The fallback reads it to release frames only while nothing
/// else has.
pub static DSHOT_LAST_RELEASE_MS: AtomicU32 = AtomicU32::new(0);
pub static IMU_BIAS_CALIBRATED: AtomicBool = AtomicBool::new(false);
/// A USB host has the board configured, written by the USB task. The
/// Foxeer has no VBUS sense pin, so a power-only USB supply does not count.
pub static USB_HOST_CONNECTED: AtomicBool = AtomicBool::new(false);
/// The configurator link has sent a command within the last couple of
/// seconds, written by its task.
pub static CONFIGURATOR_LINK_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The stored accelerometer level, milli-g per drone axis, published by the
/// flash manager when it loads or saves the configuration and subtracted by
/// the control loop before the attitude estimate. Three words, so a reader
/// can see one update half done for a single tick; only the estimated
/// attitude reads them.
pub static ACCEL_LEVEL_X_MG: AtomicI16 = AtomicI16::new(0);
pub static ACCEL_LEVEL_Y_MG: AtomicI16 = AtomicI16::new(0);
pub static ACCEL_LEVEL_Z_MG: AtomicI16 = AtomicI16::new(0);

pub fn publish_accel_level(level: ferrowasp_control::calibration::accel_level::AccelLevel) {
    ACCEL_LEVEL_X_MG.store(level.offset_mg[0], Ordering::Relaxed);
    ACCEL_LEVEL_Y_MG.store(level.offset_mg[1], Ordering::Relaxed);
    ACCEL_LEVEL_Z_MG.store(level.offset_mg[2], Ordering::Relaxed);
}

/// The level last published; zero, no correction, until storage has loaded.
pub fn accel_level() -> ferrowasp_control::calibration::accel_level::AccelLevel {
    ferrowasp_control::calibration::accel_level::AccelLevel {
        offset_mg: [
            ACCEL_LEVEL_X_MG.load(Ordering::Relaxed),
            ACCEL_LEVEL_Y_MG.load(Ordering::Relaxed),
            ACCEL_LEVEL_Z_MG.load(Ordering::Relaxed),
        ],
    }
}

/// Whether a computer or a configurator is attached, which blocks arming.
pub fn host_connected() -> bool {
    USB_HOST_CONNECTED.load(Ordering::Acquire) || CONFIGURATOR_LINK_ACTIVE.load(Ordering::Acquire)
}
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
pub static ACTIVE_IMU_KIND: AtomicU8 = AtomicU8::new(0);
/// The last accepted receiver frame's channels in microseconds, for the USB
/// status line. Display only: nothing reads it to fly or arm.
pub static RC_CHANNELS_US: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];

/// Every channel of [`RC_CHANNELS_US`], read one at a time.
pub fn rc_channels_us() -> [u16; 16] {
    core::array::from_fn(|index| RC_CHANNELS_US[index].load(Ordering::Relaxed))
}

/// Each physical ESC output's latest legacy UART eRPM/100, `u32::MAX` until
/// one arrives, and when it did, in wrapping milliseconds.
pub static ESC_UART_ERPM_DIV100: [AtomicU32; 4] = [const { AtomicU32::new(u32::MAX) }; 4];
pub static ESC_UART_SEEN_MS: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
/// Each physical ESC output's latest valid bidirectional reply eRPM/100,
/// `u32::MAX` until one arrives, and its valid and invalid reply counts.
pub static DSHOT_REPLY_ERPM_DIV100: [AtomicU32; 4] = [const { AtomicU32::new(u32::MAX) }; 4];
pub static DSHOT_REPLIES_VALID: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
pub static DSHOT_REPLIES_INVALID: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];

/// What the `esc` command reports, on whichever link asked.
pub fn esc_snapshot(now_ms: u32) -> [ferrowasp_cli::usb_debug::EscLaneSnapshot; 4] {
    let known = |value: u32| (value != u32::MAX).then_some(value);
    core::array::from_fn(|index| ferrowasp_cli::usb_debug::EscLaneSnapshot {
        uart_erpm_div100: known(ESC_UART_ERPM_DIV100[index].load(Ordering::Relaxed))
            .map(|erpm| erpm as u16),
        uart_age_ms: now_ms.wrapping_sub(ESC_UART_SEEN_MS[index].load(Ordering::Relaxed)),
        reply_erpm_div100: known(DSHOT_REPLY_ERPM_DIV100[index].load(Ordering::Relaxed)),
        replies_valid: DSHOT_REPLIES_VALID[index].load(Ordering::Relaxed),
        replies_invalid: DSHOT_REPLIES_INVALID[index].load(Ordering::Relaxed),
    })
}

/// What the `live` command reports, on whichever link asked.
pub fn live_snapshot() -> ferrowasp_cli::usb_debug::LiveSnapshot {
    ferrowasp_cli::usb_debug::LiveSnapshot {
        armed: SAFETY_ARMED.load(Ordering::Relaxed),
        arm_switch: RC_ARM_HIGH.load(Ordering::Relaxed),
        attitude_deg10: [
            ATTITUDE_ROLL_DEG10.load(Ordering::Relaxed),
            ATTITUDE_PITCH_DEG10.load(Ordering::Relaxed),
            ATTITUDE_YAW_DEG10.load(Ordering::Relaxed),
        ],
        active_motor_lanes: DSHOT_ACTIVE_LANES.load(Ordering::Relaxed),
    }
}
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

/// The latest accelerometer, gyro and temperature reading, consistent as a set:
/// the version is odd while a writer is mid-update, so a read that sees it change
/// retries. `None` after four torn reads.
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
