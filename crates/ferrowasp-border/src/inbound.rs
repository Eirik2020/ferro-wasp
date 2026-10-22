//! The value a non-critical task still hands the safety-critical side
//! directly, kept beside the coordinator, which will take it in through a
//! typed request (design §11, §16): the stored accelerometer level the
//! control loop subtracts.
//!
//! Requirements: BORDER-004
//! Safety-critical.

use core::sync::atomic::{AtomicI16, Ordering};

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

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_control::calibration::accel_level::AccelLevel;

    // One test: the statics are the firmware's single copies.
    #[test]
    fn the_level_reads_back_whole() {
        assert_eq!(accel_level().offset_mg, [0; 3]);
        publish_accel_level(AccelLevel {
            offset_mg: [12, -34, 56],
        });
        assert_eq!(accel_level().offset_mg, [12, -34, 56]);
    }
}
