//! What is left of the latest-value snapshots once the observer's ledger
//! carries everything a reader outside the core sees: values between core
//! tasks, which become links, flags and init values; the inbound values the
//! coordinator will take; and the flash manager's own state.
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

pub static RC_ARM_HIGH: AtomicBool = AtomicBool::new(false);
pub static RC_THROTTLE: AtomicU32 = AtomicU32::new(0);
pub static SAFETY_ARMED: AtomicBool = AtomicBool::new(false);
/// The mixer's logical-to-physical motor map as four digits, published by the
/// control loop; `0` until it first runs. Bench motor requests use it so a
/// test spins the motor the mixer would.
pub static MOTOR_OUTPUT_MAP_SNAPSHOT: AtomicU16 = AtomicU16::new(0);
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
pub static IMU_TRANSPORT_READY: AtomicBool = AtomicBool::new(false);
pub static ACTIVE_IMU_KIND: AtomicU8 = AtomicU8::new(0);

pub static USB_DEBUG_DUE: AtomicBool = AtomicBool::new(false);
pub static FLASH_READY: AtomicBool = AtomicBool::new(false);
pub static FLASH_JEDEC_MANUFACTURER: AtomicU8 = AtomicU8::new(0);
pub static FLASH_JEDEC_MEMORY_TYPE: AtomicU8 = AtomicU8::new(0);
pub static FLASH_JEDEC_CAPACITY_CODE: AtomicU8 = AtomicU8::new(0);
pub static FLASH_CAPACITY_BYTES: AtomicU32 = AtomicU32::new(0);
pub static FLASH_LOG_RATE_DIVISOR: AtomicU32 = AtomicU32::new(1);
pub static FLASH_RECORDS_DROPPED: AtomicU32 = AtomicU32::new(0);
pub static FLASH_PAGES_WRITTEN: AtomicU32 = AtomicU32::new(0);
pub static FLASH_WRITE_FAULTS: AtomicU32 = AtomicU32::new(0);
