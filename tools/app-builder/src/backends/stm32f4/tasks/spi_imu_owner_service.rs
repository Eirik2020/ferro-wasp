use crate::{
    backends::stm32f4::task_authoring::spi_imu::{Spi1ImuEndpointOwner, SpiImuOwnerOutcome},
    rtic::task::Mono,
};

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Exclusive DMA and chip-select owner.
            owner: Spi1ImuEndpointOwner,
        }
        config {}
        spawns {}
    }

    /// Starts or cancels one transaction requested through the async mailbox.
    pub async fn spi_imu_owner_service(mut cx: spi_imu_owner_service::Context<'_>) {
        let now_us = Mono::now().duration_since_epoch().to_micros();
        match cx.shared.owner.lock(|owner| owner.service_request(now_us)) {
            SpiImuOwnerOutcome::Idle
            | SpiImuOwnerOutcome::Started
            | SpiImuOwnerOutcome::Cancelled => {}
            SpiImuOwnerOutcome::RecoveryFailed => {
                defmt::warn!("SPI1 IMU cancellation recovery failed; owner disabled")
            }
            SpiImuOwnerOutcome::StartFailed => {
                defmt::warn!("SPI1 IMU transaction could not start")
            }
        }
    }
}
