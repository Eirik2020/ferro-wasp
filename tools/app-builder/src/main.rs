//! Command-line entry point for the canonical RTIC application builder.

#![deny(missing_docs)]

fn main() {
    if let Err(error) = xtask::run() {
        eprintln!("app builder failed: {error:#}");
        std::process::exit(1);
    }
}
