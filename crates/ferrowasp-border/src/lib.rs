//! The border roles (ADR-0061): safety-critical tasks that run below the
//! control path and the safety master and above every non-critical task, so
//! one that hangs or faults cannot bring the core down. One module each.

#![no_std]
#![forbid(unsafe_code)]

pub mod observer;
