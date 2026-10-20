#![forbid(unsafe_code)]
#![no_std]

#[cfg(test)]
extern crate std;

pub mod health;
pub mod serial;
pub use ferrowasp_imu::spi;
pub mod stats;
pub use ferrowasp_io_contracts::time;
pub mod waveform;
