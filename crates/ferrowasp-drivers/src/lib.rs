#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod blheli_telemetry;
pub mod icm42688p;
pub mod mpu6500;
pub mod serial_consumer;
pub mod spi_nor;
