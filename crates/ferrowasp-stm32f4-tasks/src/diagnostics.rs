//! The Foxeer-shaped heartbeat: USB RC-link snapshots every two seconds, and
//! the storage and optional IMU diagnostics logged beside them.

use crate::prelude::*;
use crate::snapshots::*;

/// Publish the USB RC-link status for the USB task and wake it, then log
/// storage health - and, with `imu_transport_rtt` or `imu_orientation_rtt`,
/// IMU transport and orientation - every two seconds.
///
/// The IMU's orientation is the board's, so it is configuration: the mapping
/// from the physical IMU to the drone body, and to the rate controller.
#[ferroforge::task(
    local = [
        usb_rc_link_reader: signals::RcLinkReader,
        previous_drdy_count: u32 = 0,
        previous_drdy_rejected: u32 = 0,
    ],
    config = [
        physical_imu_to_drone_rotation: dt::FrameRotation,
        control_imu_to_rate_controller_map: dt::FrameRotation,
    ],
    monotonic = Mono,
)]
pub async fn heartbeat(cx: heartbeat::Context) {
    info!("Running heartbeat!");

    loop {
        let now_us = Mono::now().duration_since_epoch().to_micros();
        let rc_link = cx.local.usb_rc_link_reader.status(now_us);
        USB_RC_VALID_SNAPSHOT.store(rc_link.valid, Ordering::Relaxed);
        USB_RC_ARMABLE_SNAPSHOT.store(rc_link.armable, Ordering::Relaxed);
        USB_DEBUG_DUE.store(true, Ordering::Release);
        cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);

        #[cfg(feature = "imu_transport_rtt")]
        info!(
            "IMU raw gyro [{}, {}, {}], seq {}",
            IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed),
            IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed),
            IMU_LATEST_YAW_RAW.load(Ordering::Relaxed),
            IMU_LATEST_SEQ.load(Ordering::Relaxed)
        );
        #[cfg(feature = "imu_orientation_rtt")]
        if let Some((sequence, accel_mg, gyro_dps10, temp_c10)) = imu_orientation_snapshot() {
            let body_gyro_dps10 = CONFIG::PHYSICAL_IMU_TO_DRONE_ROTATION.map_i32(gyro_dps10);
            let control_gyro_dps10 = CONFIG::CONTROL_IMU_TO_RATE_CONTROLLER_MAP.map_i32(gyro_dps10);
            let body_specific_force_mg = CONFIG::PHYSICAL_IMU_TO_DRONE_ROTATION.map_i32(accel_mg);
            let body_gravity_mg = [
                -body_specific_force_mg[0],
                -body_specific_force_mg[1],
                -body_specific_force_mg[2],
            ];
            info!(
                "IMU ORIENT sensor seq {} acc_mg [{}, {}, {}] gyro_dps10 [{}, {}, {}] temp_c10 {}",
                sequence,
                accel_mg[0],
                accel_mg[1],
                accel_mg[2],
                gyro_dps10[0],
                gyro_dps10[1],
                gyro_dps10[2],
                temp_c10
            );
            info!(
                "IMU ORIENT body seq {} gravity_mg [{}, {}, {}] gyro_dps10 [{}, {}, {}]",
                sequence,
                body_gravity_mg[0],
                body_gravity_mg[1],
                body_gravity_mg[2],
                body_gyro_dps10[0],
                body_gyro_dps10[1],
                body_gyro_dps10[2]
            );
            info!(
                "IMU ORIENT control seq {} gyro_dps10 [{}, {}, {}]",
                sequence, control_gyro_dps10[0], control_gyro_dps10[1], control_gyro_dps10[2]
            );
        }
        #[cfg(feature = "imu_transport_rtt")]
        {
            let drdy_count = IMU_DRDY_IRQ_COUNT.load(Ordering::Relaxed);
            let drdy_rejected = IMU_DRDY_REJECTED_COUNT.load(Ordering::Relaxed);
            info!(
                "IMU DRDY IRQ {}, delta {}, rejected {}, delta {}, last {} us",
                drdy_count,
                drdy_count.wrapping_sub(*cx.local.previous_drdy_count),
                drdy_rejected,
                drdy_rejected.wrapping_sub(*cx.local.previous_drdy_rejected),
                IMU_DRDY_LAST_US.load(Ordering::Relaxed)
            );
            *cx.local.previous_drdy_count = drdy_count;
            *cx.local.previous_drdy_rejected = drdy_rejected;
        }
        info!(
            "SPI2 flash ready {}, JEDEC {:02x}:{:02x}:{:02x}, capacity {} bytes",
            FLASH_READY.load(Ordering::Relaxed),
            FLASH_JEDEC_MANUFACTURER.load(Ordering::Relaxed),
            FLASH_JEDEC_MEMORY_TYPE.load(Ordering::Relaxed),
            FLASH_JEDEC_CAPACITY_CODE.load(Ordering::Relaxed),
            FLASH_CAPACITY_BYTES.load(Ordering::Relaxed)
        );
        info!(
            "SPI2 blackbox pages {}, dropped records {}, write faults {}, divisor {}",
            FLASH_PAGES_WRITTEN.load(Ordering::Relaxed),
            FLASH_RECORDS_DROPPED.load(Ordering::Relaxed),
            FLASH_WRITE_FAULTS.load(Ordering::Relaxed),
            FLASH_LOG_RATE_DIVISOR.load(Ordering::Relaxed)
        );
        Mono::delay(2000.millis()).await;
    }
}
