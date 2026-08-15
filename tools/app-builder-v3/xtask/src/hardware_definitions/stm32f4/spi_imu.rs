//! Host-checkable authoring surface for the SPI1 IMU endpoint.
//!
//! Shared outcomes and frame types come from the reusable STM32F4 adapter.
//! The concrete DMA-owning types are ARM-only, so these narrow stand-ins let
//! rust-analyzer check task bodies on the host.

pub use ferrowasp_stm32f4::spi_imu_endpoint::{
    IMU_KIND_ICM42688P, IMU_KIND_MPU6500, IMU_KIND_NONE, SPI_IMU_FRAME_SIZE, SpiImuBufferError,
    SpiImuFrame, SpiImuOwnerOutcome, SpiImuRxOutcome, SpiImuTimeoutOutcome,
};

/// Authoring stand-in for the ARM-only exclusive DMA owner.
pub struct Spi1ImuEndpointOwner;

impl Spi1ImuEndpointOwner {
    /// Checks and services one mailbox request.
    pub fn service_request(&mut self, _now_us: u64) -> SpiImuOwnerOutcome {
        SpiImuOwnerOutcome::Idle
    }

    /// Services one receive-DMA interrupt.
    pub fn service_dma_irq(&mut self) -> SpiImuRxOutcome {
        SpiImuRxOutcome::Ignored
    }

    /// Applies bounded timeout recovery.
    pub fn service_timeout(&mut self, _now_us: u64) -> SpiImuTimeoutOutcome {
        SpiImuTimeoutOutcome::Idle
    }
}

/// Authoring stand-in for the ARM-only async SPI device.
pub struct Spi1ImuDevice;

impl Spi1ImuDevice {
    /// Submits one fixed-size register burst through the endpoint mailbox.
    pub async fn read_burst(
        &mut self,
        _request: u8,
        _observed_at_us: u64,
    ) -> Result<(), ferrowasp_io_core::spi::SpiDeviceError> {
        Err(ferrowasp_io_core::spi::SpiDeviceError::Unavailable)
    }
}

/// Authoring stand-in for the ARM-only completed-frame parser side.
pub struct Spi1ImuParser;

impl Spi1ImuParser {
    /// Takes the next completed frame, if any.
    pub fn next_frame(&mut self) -> Option<SpiImuFrame> {
        None
    }

    /// Returns one processed frame buffer to the bounded free pool.
    pub fn return_buffer(
        &mut self,
        _buffer: &'static mut [u8; SPI_IMU_FRAME_SIZE],
    ) -> Result<(), SpiImuBufferError> {
        Ok(())
    }
}
