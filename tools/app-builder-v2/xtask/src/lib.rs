//! Host-side generator for the NUCLEO-F401RE RTIC prototype application.
//!
//! The generator validates handwritten board and application declarations,
//! resolves task resource ownership, renders STM32F401-specific initialization,
//! and writes the complete RTIC application for the selected target.

#![deny(missing_docs)]

mod app;
mod backend;
mod board;
mod generator;
pub mod hw_resources;
mod resolve;
pub mod task;
mod task_check;
#[path = "../../tasks/mod.rs"]
mod tasks;

use std::{env, path::PathBuf};

use anyhow::{Result, bail};

/// Writes the host-only Rust harness that type-checks unified task bodies.
pub fn write_task_checks(
    repository_root: &std::path::Path,
    destination: &std::path::Path,
) -> Result<()> {
    task_check::write(repository_root, destination)
}

/// Runs the xtask command selected by the process arguments.
///
/// The only supported command is `generate`, which validates and regenerates
/// the NUCLEO-F401RE target application.
pub fn run() -> Result<()> {
    let mut arguments = env::args().skip(1);
    match (arguments.next().as_deref(), arguments.next()) {
        (Some("generate"), None) => generator::generate(&repository_root()),
        _ => bail!("usage: cargo xtask generate"),
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate has a parent directory")
        .to_path_buf()
}
