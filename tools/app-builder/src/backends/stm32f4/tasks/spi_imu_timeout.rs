use fugit::MillisDurationU32;

use crate::{
    backends::stm32f4::task_authoring::spi_imu::{Spi1ImuEndpointOwner, SpiImuTimeoutOutcome},
    rtic::task::Mono,
};

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Exclusive DMA and mailbox owner.
            owner: Spi1ImuEndpointOwner,
        }
        config {
            /// Delay before checking the sub-millisecond transaction deadline.
            check_after: MillisDurationU32,
        }
        spawns {}
    }

    /// Recovers a transaction that did not complete before its deadline.
    pub async fn spi_imu_timeout(mut cx: spi_imu_timeout::Context<'_>, observed_at_us: u64) {
        let _ = observed_at_us;
        Mono::delay(cx.config.check_after).await;
        let now_us = Mono::now().duration_since_epoch().to_micros();
        match cx.shared.owner.lock(|owner| owner.service_timeout(now_us)) {
            SpiImuTimeoutOutcome::Idle | SpiImuTimeoutOutcome::Active => {}
            SpiImuTimeoutOutcome::TimedOut => {
                defmt::warn!("SPI1 IMU transaction timed out and was recovered")
            }
            SpiImuTimeoutOutcome::RecoveryFailed => {
                defmt::warn!("SPI1 IMU timeout recovery failed; owner disabled")
            }
        }
    }
}
