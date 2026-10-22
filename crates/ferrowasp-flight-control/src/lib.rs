//! How FerroWasp flies, safety-critical (ADR-0060): the rate controller and
//! its tuning, stick shaping, the mixer and the motor map it applies, dynamic
//! idle and the RPM filter's wiring. The general maths it builds on lives in
//! `ferrowasp-control`; the control loop task joins it later.
//!
//! Safety-critical.

#![forbid(unsafe_code)]
#![no_std]

pub mod actuator;
pub mod drone_toolbox;
pub mod dynamic_idle;
pub mod rpm_filter;
#[cfg(all(feature = "tasks", target_arch = "arm"))]
pub mod tasks;
