//! Flight-log and configuration storage, non-critical (ADR-0060): the SPI NOR
//! driver and the `NorStorage` trait every storage backend implements.

#![no_std]
#![forbid(unsafe_code)]

pub mod nor_storage;
pub mod spi_nor;
