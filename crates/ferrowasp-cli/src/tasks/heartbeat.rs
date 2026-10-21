//! The Foxeer-shaped heartbeat: the USB status line's two-second wake, and
//! the storage and optional IMU diagnostics logged beside it, all read from
//! the observer's ledger.

use super::prelude::*;
use crate::tasks::usb::STATUS_LINE_DUE;

/// Wake the USB task for its status line, report the isolated stale IMU
/// ticks the control loop tolerated, then log storage health and blackbox
/// losses - and, with `imu_transport_rtt` or `imu_orientation_rtt`, IMU
/// transport and orientation - every two seconds.
///
/// The IMU's orientation is the board's, so it is configuration: the mapping
/// from the physical IMU to the drone body, and to the rate controller.
#[ferroforge::task(
    local = [
        previous_drdy_count: u32 = 0,
        previous_drdy_rejected: u32 = 0,
        previous_tolerated_stale_ticks: u32 = 0,
    ],
    shared = [ledger: Ledger],
    config = [
        physical_imu_to_drone_rotation: dt::FrameRotation,
        control_imu_to_rate_controller_map: dt::FrameRotation,
        wake_usb: fn(),
    ],
    monotonic = Mono,
)]
pub async fn heartbeat(mut cx: heartbeat::Context) {
    info!("Running heartbeat!");

    loop {
        STATUS_LINE_DUE.store(true, Ordering::Release);
        (CONFIG::WAKE_USB)();

        let (identity, observer, control, imu, drdy, storage) = cx.shared.ledger.lock(|ledger| {
            (
                ledger.identity(),
                ledger.observer().value,
                ledger.control().value,
                ledger.imu().value,
                ledger.drdy().value,
                ledger.storage().value,
            )
        });
        #[cfg(not(feature = "imu_transport_rtt"))]
        let _ = drdy;
        #[cfg(not(any(feature = "imu_transport_rtt", feature = "imu_orientation_rtt")))]
        let _ = imu;

        let tolerated_stale_ticks = control.tolerated_stale_ticks;
        if tolerated_stale_ticks != *cx.local.previous_tolerated_stale_ticks {
            info!(
                "IMU: {} isolated stale ticks tolerated in 2 s, {} since boot",
                tolerated_stale_ticks.wrapping_sub(*cx.local.previous_tolerated_stale_ticks),
                tolerated_stale_ticks
            );
            *cx.local.previous_tolerated_stale_ticks = tolerated_stale_ticks;
        }

        #[cfg(feature = "imu_transport_rtt")]
        info!(
            "IMU raw gyro [{}, {}, {}], seq {}",
            imu.gyro_raw[0], imu.gyro_raw[1], imu.gyro_raw[2], imu.sample_seq
        );
        #[cfg(feature = "imu_orientation_rtt")]
        {
            let sequence = imu.sample_seq;
            let accel_mg = imu.accel_g.map(|accel| (accel * 1_000.0) as i32);
            let gyro_dps10 = imu.gyro_dps.map(|rate| (rate * 10.0) as i32);
            let temp_c10 = (imu.temperature_c * 10.0) as i32;
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
            let drdy_count = drdy.interrupts;
            let drdy_rejected = drdy.rejected;
            info!(
                "IMU DRDY IRQ {}, delta {}, rejected {}, delta {}, last {} us",
                drdy_count,
                drdy_count.wrapping_sub(*cx.local.previous_drdy_count),
                drdy_rejected,
                drdy_rejected.wrapping_sub(*cx.local.previous_drdy_rejected),
                drdy.last_us
            );
            *cx.local.previous_drdy_count = drdy_count;
            *cx.local.previous_drdy_rejected = drdy_rejected;
        }
        info!(
            "Storage ready {}, JEDEC {:02x}:{:02x}:{:02x}, capacity {} bytes",
            storage.ready,
            identity.flash_jedec[0],
            identity.flash_jedec[1],
            identity.flash_jedec[2],
            identity.flash_capacity_bytes
        );
        info!(
            "Blackbox pages {}, dropped records {}, write faults {}, divisor {}",
            storage.pages_written,
            storage.records_dropped,
            storage.write_faults,
            storage.log_rate_divisor
        );
        info!(
            "Observer: control ticks lost {}, blackbox stream refused {}",
            observer.control_ticks_lost, observer.stream_refused
        );
        Mono::delay(2000.millis()).await;
    }
}
