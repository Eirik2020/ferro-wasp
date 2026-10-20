//! General control maths: PID, filters, frame rotation, the attitude
//! estimator and the calibrations, reusable outside FerroWasp and proven by
//! unit tests. How FerroWasp flies with them lives in the flight-controller
//! crates (ADR-0060).

#![no_std]
#![forbid(unsafe_code)]

pub mod attitude;
pub mod calibration;
pub mod filter;
pub mod frames;
pub mod pid;
