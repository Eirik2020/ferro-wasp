//! The IMU group, safety-critical (ADR-0060): the SPI request mailbox and
//! async device the IMU task reads its sensor through. The IMU trait and the
//! group's task join it as the redesign proceeds.

#![forbid(unsafe_code)]
#![no_std]

#[cfg(test)]
extern crate std;

pub mod spi;

use ferrowasp_io_contracts::time;
