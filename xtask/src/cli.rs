use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Assemble validated RTIC applications")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate and validate an application from separate BSP and application manifests.
    Generate {
        /// Application name from applications/ without `.toml`.
        #[arg(
            long = "app",
            visible_alias = "application",
            conflicts_with_all = ["manifest", "bsp"]
        )]
        app: Option<String>,

        /// Application manifest path or name from applications/ without `.toml`.
        #[arg(long, requires = "bsp")]
        manifest: Option<PathBuf>,

        /// BSP manifest path or name from bsp/ without `.toml`.
        #[arg(long, requires = "manifest")]
        bsp: Option<PathBuf>,

        /// Continue from a fingerprint-matching working checkpoint.
        #[arg(long)]
        resume: bool,
    },

    /// Generate, validate, and release-build one conventionally named application.
    Build {
        /// Application name from applications/ without `.toml`.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },

    /// Generate, build, and flash one conventionally named application.
    Flash {
        /// Application name from applications/ without `.toml`.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },

    /// Generate, build, and start a cargo-embed session for one application.
    Embed {
        /// Application name from applications/ without `.toml`.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },

    /// Remove generated state for one application.
    Clean {
        /// Validated application slug below generated/.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },
}
