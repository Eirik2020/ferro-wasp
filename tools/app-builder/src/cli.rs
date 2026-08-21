//! Scriptable command-line surface for catalog applications.

use clap::{Parser, Subcommand};

/// Top-level App Builder command-line arguments.
#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Build validated FerroWasp RTIC applications")]
pub struct Cli {
    /// Selected builder operation.
    #[command(subcommand)]
    pub command: Command,
}

/// Operations available for one explicit catalog application.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Resolve and render an application without writing output.
    Check {
        /// Exact ID from the compile-time application catalog.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },
    /// Resolve, render, and write an application working tree.
    Generate {
        /// Exact ID from the compile-time application catalog.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },
    /// Generate and release-build an application.
    Build {
        /// Exact ID from the compile-time application catalog.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },
    /// Remove generated state for one exact catalog application.
    Clean {
        /// Exact ID from the compile-time application catalog.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },
    /// Generate, build, and explicitly flash an application through probe-rs.
    Flash {
        /// Exact ID from the compile-time application catalog.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },
    /// Generate, build, and explicitly start cargo-embed for an application.
    Embed {
        /// Exact ID from the compile-time application catalog.
        #[arg(long = "app", visible_alias = "application")]
        app: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_requires_an_explicit_application() {
        for operation in ["check", "generate", "build", "clean", "flash", "embed"] {
            let error = Cli::try_parse_from(["xtask", operation]).unwrap_err();
            assert_eq!(error.exit_code(), 2, "unexpected exit for {operation}");
        }
    }

    #[test]
    fn commands_accept_app_and_visible_application_alias() {
        for operation in ["check", "generate", "build", "clean", "flash", "embed"] {
            Cli::try_parse_from(["xtask", operation, "--app", "foxeer-f405-v2"]).unwrap();
            Cli::try_parse_from(["xtask", operation, "--application", "nucleo-f401re-blinky"])
                .unwrap();
        }
    }

    #[test]
    fn arbitrary_manifest_and_rust_source_arguments_are_rejected() {
        for argument in ["--manifest", "--bsp", "--rust-source"] {
            let error = Cli::try_parse_from([
                "xtask",
                "generate",
                "--app",
                "foxeer-f405-v2",
                argument,
                "outside.rs",
            ])
            .unwrap_err();
            assert_eq!(error.exit_code(), 2);
        }
    }
}
