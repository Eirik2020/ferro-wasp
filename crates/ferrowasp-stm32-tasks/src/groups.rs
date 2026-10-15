//! Task groups: sets of definitions a firmware selects with one
//! `#[group(from = ..)]`, binding their resources and configuration once and
//! giving each member only its priority and interrupt.

/// The SPI1 IMU path: the data-ready interrupt, the polling and owner
/// tasks, the receive DMA completion, the I/O watchdog and its timeout, and
/// the sample parser.
#[ferroforge::group]
pub mod spi1_imu {
    pub use crate::imu_data_ready;
    pub use crate::io_watchdog;
    pub use crate::spi1_owner_service;
    pub use crate::spi1_parser;
    pub use crate::spi1_poll;
    pub use crate::spi1_rx_dma;
    pub use crate::spi1_timeout;
}
