//! The message types hardware exchanges with the safety-critical core
//! (design §12): what a serial port hands the core and what the core hands back,
//! the timestamps every message carries, and the link every message travels
//! over (design §9). Only types and the link live here, so a change is rare and
//! costs a re-run of every chip's conformance evidence.
//!
//! Safety-critical.

#![no_std]
#![forbid(unsafe_code)]

pub mod imu;
pub mod link;
pub mod serial;
pub mod time;
