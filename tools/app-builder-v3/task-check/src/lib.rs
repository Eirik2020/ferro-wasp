//! Compile-only RTIC-shaped environment for reusable task functions.

#![no_std]
#![deny(warnings)]

pub use xtask::app_task;

/// Declaration API mirrored for unified task modules included by the checker.
pub mod task {
    pub use xtask::task::*;
}

mod support;

include!(concat!(env!("OUT_DIR"), "/task_checks.rs"));
