//! Flight-log and configuration storage, non-critical (design §16): the SPI NOR
//! driver and the `NorStorage` trait every storage backend implements, the
//! storage layout and boot slot selection, and saving the configuration.

#![no_std]
#![forbid(unsafe_code)]

pub mod config_save;
pub mod layout;
pub mod nor_storage;
pub mod spi_nor;
#[cfg(any(test, feature = "test-support"))]
pub mod test_nor;
