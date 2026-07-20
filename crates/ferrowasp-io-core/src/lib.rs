#![forbid(unsafe_code)]
#![no_std]

#[cfg(test)]
extern crate std;

pub mod health;
pub mod serial;
pub mod spi;
pub mod stats;
pub mod time;
pub mod waveform;
