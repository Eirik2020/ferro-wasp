use crate::hardware_definitions::stm32f4::{
    golden_service_authoring::{IoWatchdog, acknowledge_watchdog_tick},
    spi_imu::{Spi1ImuEndpointOwner, SpiImuTimeoutOutcome},
};
use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// Sole TIM6 update-interrupt owner.
            watchdog: IoWatchdog,
        }
        shared {
            /// SPI1 transaction owner inspected for expired deadlines.
            spi1_owner: Spi1ImuEndpointOwner,
        }
        config {}
        spawns {}
    }

    /// Acknowledges TIM6 and performs one bounded SPI1 timeout recovery check.
    pub fn io_watchdog(mut cx: io_watchdog::Context<'_>) {
        acknowledge_watchdog_tick(cx.local.watchdog);
        let now_us = Mono::now().duration_since_epoch().to_micros();
        match cx
            .shared
            .spi1_owner
            .lock(|owner| owner.service_timeout(now_us))
        {
            SpiImuTimeoutOutcome::Idle | SpiImuTimeoutOutcome::Active => {}
            SpiImuTimeoutOutcome::TimedOut => {
                defmt::warn!("TIM6 watchdog recovered an expired SPI1 transaction")
            }
            SpiImuTimeoutOutcome::RecoveryFailed => {
                defmt::warn!("TIM6 watchdog could not recover SPI1; owner disabled")
            }
        }
    }
}
