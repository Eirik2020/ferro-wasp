//! The values non-critical tasks hand the safety-critical side today, kept
//! together until the coordinator takes them in through its typed requests
//! (ADR-0056, section 11): the host-connected flags the arming guard reads,
//! and the stored accelerometer level the control loop subtracts.

use core::sync::atomic::{AtomicBool, AtomicI16, Ordering};

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
