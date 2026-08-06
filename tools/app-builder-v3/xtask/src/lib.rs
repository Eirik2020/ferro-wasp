//! Host-side authoring model and generator for STM32F4 RTIC applications.
//!
//! The active V3 surface currently provides hardware definitions, reusable
//! task-local contracts, application-composition declarations, deterministic
//! resolution, and RTIC source rendering.

#![deny(missing_docs)]

/// Hardware-neutral definitions used by builder authoring APIs.
pub mod hardware_definitions;
/// RTIC-shaped task, component, and application-composition definitions.
pub mod rtic;
/// Board and application declarations for the selected generation target.
pub mod target;
/// Reusable HAL-agnostic task definitions and ordinary Rust task bodies.
pub mod tasks;

/// Selected-target generation orchestration and filesystem output.
pub mod generator;
