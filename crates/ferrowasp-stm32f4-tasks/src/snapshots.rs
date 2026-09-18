//! Latest-value snapshots shared between tasks and diagnostics.
//!
//! One static per value for the whole firmware: a task that writes one and a
//! task that reads it must name the same static, so a firmware re-exports
//! these by name and never defines its own.

#[cfg(feature = "imu_orientation_rtt")]
use core::sync::atomic::Ordering;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32};

pub static IMU_LATEST_SEQ: AtomicU32 = AtomicU32::new(0);
pub static IMU_LATEST_ROLL_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_LATEST_PITCH_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_LATEST_YAW_RAW: AtomicI32 = AtomicI32::new(0);
pub static IMU_STALE: AtomicBool = AtomicBool::new(true);
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
pub static IMU_BIAS_CALIBRATED: AtomicBool = AtomicBool::new(false);
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
