pub mod assembler;
pub mod backend;
pub mod cli;
pub mod diagnostics;
pub mod feature;
pub mod manifest;
pub mod mcu;
pub mod render;
pub mod runner;
pub mod state;
pub mod syntax;
pub mod validate;

use anyhow::Result;
use clap::Parser;

pub const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BACKEND_VERSION: &str = "0.8.0";

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    assembler::execute(cli)
}
