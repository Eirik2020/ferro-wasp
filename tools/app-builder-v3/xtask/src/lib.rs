//! Host-side generator for the supported STM32F4 RTIC prototype applications.
//!
//! The generator validates each handwritten board and application declaration,
//! resolves task resource ownership, renders MCU-specific initialization, and
//! writes the complete RTIC application for every supported target.

#![deny(missing_docs)]

#[cfg(feature = "legacy-generator")]
compile_error!(
    "the legacy generator cannot be enabled until its component declarations are migrated into App Builder V3"
);

#[cfg(feature = "legacy-generator")]
mod app;
#[cfg(feature = "legacy-generator")]
mod backend;
#[cfg(feature = "legacy-generator")]
mod board;
#[cfg(feature = "legacy-generator")]
pub mod component;
#[cfg(feature = "legacy-generator")]
#[path = "legacy_components.rs"]
pub mod components;
#[cfg(feature = "legacy-generator")]
mod generator;
/// Board-specific hardware declarations used by the builder authoring APIs.
pub mod hardware_declaration;
/// Hardware-neutral definitions used by builder authoring APIs.
pub mod hardware_definitions;
#[cfg(feature = "legacy-generator")]
pub mod hw_resources;
#[cfg(feature = "legacy-generator")]
mod resolve;
#[cfg(feature = "legacy-generator")]
pub mod task;
#[cfg(feature = "legacy-generator")]
mod task_check;
#[cfg(feature = "legacy-generator")]
#[path = "../../tasks/mod.rs"]
mod tasks;

#[cfg(feature = "legacy-generator")]
use std::{env, path::PathBuf};

/// Renders a balanced, page-width ownership divider for generated code.
#[cfg(feature = "legacy-generator")]
pub(crate) fn component_divider(owner: &str, kind: &str, ending: bool) -> String {
    let label = if ending {
        format!(" End component `{owner}` {kind} ")
    } else {
        format!(" Component `{owner}` {kind} ")
    };
    format!("// {label:=^85}")
}

/// Renders a generated ownership divider with an explicit nesting width.
#[cfg(feature = "legacy-generator")]
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
#[cfg(feature = "legacy-generator")]
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
#[cfg(feature = "legacy-generator")]
pub fn run() -> Result<()> {
    let mut arguments = env::args().skip(1);
    match (arguments.next().as_deref(), arguments.next()) {
        (Some("generate"), None) => generator::generate(&repository_root()),
        _ => bail!("usage: cargo xtask generate"),
    }
}

#[cfg(feature = "legacy-generator")]
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate has a parent directory")
        .to_path_buf()
}
