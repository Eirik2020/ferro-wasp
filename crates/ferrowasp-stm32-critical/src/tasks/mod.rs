//! The backend's hardware transports as RTIC task definitions (design §16),
//! target-only: the SPI1 IMU group and the UART ports'
//! transports. They live here because this crate depends on the IMU and
//! serial crates whose logic they serve.
//!
//! Safety-critical.

pub mod spi1;
pub mod uart_port;

/// The names both task modules are written against; each comes from this
/// crate or one it depends on.
#[allow(unused_imports)]
mod prelude {
    pub use crate::memory as stm32_memory;
    pub use crate::memory::{SPI1_JOB_MAX_BYTES, SPI1_JOB_MAX_OPERATIONS};
    pub use crate::spi_common as stm32_spi;
    pub use crate::spi_common::{SPI_BUFFER_SIZE, SpiDmaService as _};
    pub use crate::timebase as stm32_timebase;
    pub use crate::timebase::Timebase as _;
    pub use crate::uart_common as stm32_uart;
    pub use crate::uart_common::{UartRxIrqService as _, UartTxDmaService as _};
    pub use crate::watchdog as stm32_watchdog;
    pub use core::sync::atomic::Ordering;
    pub use defmt::{info, warn};
    pub use embedded_hal::spi::Operation;
    pub use embedded_hal_async::spi::SpiDevice as _;
    pub use ferrowasp_boundary_contracts::messages as observed;
    pub use ferrowasp_icm42688p as icm;
    pub use ferrowasp_imu::spi::{
        AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiRequestMailbox,
    };
    pub use ferrowasp_io_contracts::link::LinkProducer;
    pub use ferrowasp_io_contracts::time::TimestampMicros;
    pub use ferrowasp_mpu6000 as mpu6000;
    pub use ferrowasp_mpu6500 as imu;
    pub use ferrowasp_serial::SerialFault;
    pub use fugit::ExtU64 as _;
}
