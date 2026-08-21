use crate::backends::stm32f4::task_authoring::spi_imu::{
    IMU_KIND_ICM42688P, IMU_KIND_MPU6500, Spi1ImuDevice,
};
use ferrowasp_io_core::spi::SpiDeviceError;

crate::reusable_task! {
    contract {
        local {
            /// Async SPI device uniquely owned by the polling task.
            device: Spi1ImuDevice,
            /// Suppresses repeated unavailable-owner diagnostics.
            unavailable_logged: bool,
        }
        shared {
            /// Active sensor discriminant selected during initialization.
            kind: u8,
        }
        config {}
        spawns {
            /// Checks that this transaction reaches a terminal state.
            timeout(observed_at_us: u64),
        }
    }

    /// Submits one full IMU burst through the bounded SPI mailbox.
    pub async fn spi_imu_poll(mut cx: spi_imu_poll::Context<'_>, observed_at_us: u64) {
        let kind = cx.shared.kind.lock(|kind| *kind);
        let request = match kind {
            IMU_KIND_MPU6500 => ferrowasp_drivers::mpu6500::Register::AccelXoutH as u8,
            IMU_KIND_ICM42688P => ferrowasp_drivers::icm42688p::Register::TempData1 as u8,
            _ => return,
        };

        let _ = timeout::spawn(observed_at_us);
        let result = cx.local.device.read_burst(request, observed_at_us).await;

        match result {
            Ok(()) => {
                *cx.local.unavailable_logged = false;
            }
            Err(SpiDeviceError::Busy) => defmt::warn!("SPI1 IMU transaction already active"),
            Err(SpiDeviceError::Unavailable) => {
                if !*cx.local.unavailable_logged {
                    defmt::warn!("SPI1 IMU owner unavailable after recovery failure");
                    *cx.local.unavailable_logged = true;
                }
            }
            Err(SpiDeviceError::Timeout) => {}
            Err(SpiDeviceError::Cancelled) => {
                defmt::warn!("SPI1 IMU transaction cancelled")
            }
            Err(SpiDeviceError::DmaTransfer) => {
                defmt::warn!("SPI1 IMU DMA transaction failed")
            }
            Err(
                SpiDeviceError::TooManyOperations
                | SpiDeviceError::TxCapacityExceeded
                | SpiDeviceError::RxCapacityExceeded
                | SpiDeviceError::CopybackShapeMismatch,
            ) => defmt::warn!("SPI1 IMU transaction packing failed"),
            Err(
                SpiDeviceError::InvalidState
                | SpiDeviceError::StaleTransaction
                | SpiDeviceError::Backend,
            ) => defmt::warn!("SPI1 IMU transaction backend error"),
        }
    }
}
