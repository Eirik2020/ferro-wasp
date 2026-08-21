//! Host-side typed model and generator for FerroWasp RTIC applications.

#![deny(missing_docs)]

/// Unified complete-application selection, resolution, and rendering.
pub mod application;
/// MCU-family validation and lowering backends.
pub mod backends;
/// Scriptable command-line parser.
pub mod cli;
/// Explicit registry for repository-owned board, app, component, and task inputs.
pub mod input_catalog;
/// Catalog-confined command execution.
pub mod operations;
/// RTIC-shaped task, component, and application-composition definitions.
pub mod rtic;
/// Shell-free child-process specifications and execution.
pub mod runner;
/// Strict compatibility ingestion for checked NUCLEO validation inputs.
pub mod validation;

/// Selected-target generation orchestration and filesystem output.
pub mod generator;

/// Parses process arguments and executes one command from this builder workspace.
pub fn run() -> anyhow::Result<()> {
    use clap::Parser as _;

    let cli = cli::Cli::parse();
    operations::execute(cli, std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
}
