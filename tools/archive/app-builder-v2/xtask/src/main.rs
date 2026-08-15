//! Command-line entry point for the RTIC application generator.

#![deny(missing_docs)]

fn main() {
    if let Err(error) = xtask::run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
