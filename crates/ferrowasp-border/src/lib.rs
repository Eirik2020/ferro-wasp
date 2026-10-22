//! The border roles (design §19): safety-critical tasks that run below the
//! control path and the safety master and above every non-critical task, so
//! one that hangs or faults cannot bring the core down. One module each.
//!
//! Safety-critical.

#![no_std]
#![forbid(unsafe_code)]

pub mod coordinator;
pub mod inbound;
pub mod observer;
#[cfg(all(feature = "tasks", target_arch = "arm"))]
pub mod tasks;
