//! The message types hardware exchanges with the safety-critical core
//! (ADR-0057): what a serial port hands the core and what the core hands back,
//! and the timestamps every message carries. Only types live here, so a change
//! is rare and costs a re-run of every chip's conformance evidence.

#![no_std]
#![forbid(unsafe_code)]

pub mod serial;
pub mod time;
