//! Actuator feedback, safety-critical (ADR-0060): the ESC telemetry manager
//! and the idle qualification that arming waits for. Its eRPM qualifies
//! arming, so it is safety-critical. The ESC-telemetry task joins it after
//! the backends split.
//!
//! Safety-critical.

#![forbid(unsafe_code)]
#![no_std]

pub mod esc_manager;
