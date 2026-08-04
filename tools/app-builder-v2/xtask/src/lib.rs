//! Host-side generator for the supported STM32F4 RTIC prototype applications.
//!
//! The generator validates each handwritten board and application declaration,
//! resolves task resource ownership, renders MCU-specific initialization, and
//! writes the complete RTIC application for every supported target.

#![deny(missing_docs)]

mod app;
mod backend;
mod board;
pub mod component;
mod generator;
pub mod hw_resources;
mod resolve;
pub mod serial_port;
pub mod task;
mod task_check;
#[path = "../../tasks/mod.rs"]
mod tasks;

use std::{env, path::PathBuf};

use anyhow::{Result, bail};

/// Renders a balanced, page-width ownership divider for generated code.
pub(crate) fn component_divider(owner: &str, kind: &str, ending: bool) -> String {
    let label = if ending {
        format!(" End component `{owner}` {kind} ")
    } else {
        format!(" Component `{owner}` {kind} ")
    };
    format!("// {label:=^85}")
}

/// Renders a generated ownership divider with an explicit nesting width.
pub(crate) fn scope_divider(label: &str, ending: bool, width: usize, fill: char) -> String {
    let label = if ending {
        let mut ending_label = label.to_owned();
        if let Some(first) = ending_label.get_mut(..1) {
            first.make_ascii_lowercase();
        }
        format!(" End {ending_label} ")
    } else {
        format!(" {label} ")
    };
    let content_width = width.saturating_sub(3);
    let padding = content_width.saturating_sub(label.len());
    let left = padding / 2;
    let right = padding - left;
    format!(
        "// {}{label}{}",
        fill.to_string().repeat(left),
        fill.to_string().repeat(right)
    )
}

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
/// every supported target application.
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
