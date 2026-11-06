//! The sensor classes: one trait per class of sensor, so a consumer of a
//! class uses one API whichever part a board carries. Every class shares one
//! envelope, the `Stamped` message of `ferrowasp-io-contracts`, numbered and
//! timed by the sensor, and one set of error kinds; each class adds its own
//! sample.
//!
//! Only traits, sample types and the checks every implementation shares live
//! here; drivers and their adapters live elsewhere and implement them.
//!
//! Safety-critical.

#![no_std]
#![forbid(unsafe_code)]

pub mod barometer;
pub mod class;
pub mod compass;
pub mod gnss;
pub mod imu;
