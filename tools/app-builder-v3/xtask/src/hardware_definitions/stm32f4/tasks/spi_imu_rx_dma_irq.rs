use crate::hardware_definitions::stm32f4::spi_imu::{Spi1ImuEndpointOwner, SpiImuRxOutcome};

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Exclusive DMA and mailbox owner.
            owner: Spi1ImuEndpointOwner,
        }
        config {}
        spawns {
            /// Drains and decodes completed receive buffers.
            parse(),
        }
    }

    /// Completes one SPI1 receive-DMA transaction.
    pub fn spi_imu_rx_dma_irq(mut cx: spi_imu_rx_dma_irq::Context<'_>) {
        match cx.shared.owner.lock(|owner| owner.service_dma_irq()) {
            SpiImuRxOutcome::Ignored | SpiImuRxOutcome::NoChunk => {}
            SpiImuRxOutcome::Delivered => {
                let _ = parse::spawn();
            }
            SpiImuRxOutcome::DmaError => defmt::warn!("SPI1 IMU RX DMA error"),
            SpiImuRxOutcome::NoFreshBuffer => {
                defmt::warn!("SPI1 IMU RX buffer pool exhausted")
            }
            SpiImuRxOutcome::TransferNotReady => {
                defmt::warn!("SPI1 IMU RX DMA restart failed")
            }
            SpiImuRxOutcome::FilledQueueFull => {
                defmt::warn!("SPI1 IMU filled queue full")
            }
            SpiImuRxOutcome::PlannerRejected => {
                defmt::warn!("SPI1 IMU RX planner rejected completion")
            }
        }
    }
}
