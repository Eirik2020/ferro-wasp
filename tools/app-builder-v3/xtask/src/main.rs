//! Command-line entry point for the RTIC application generator.

#![deny(missing_docs)]

#[cfg(feature = "legacy-generator")]
fn main() {
    if let Err(error) = xtask::run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "legacy-generator"))]
fn main() {
    eprintln!(
        "the V3 generator is not yet migrated; run `cargo check` or `cargo test` to validate hardware definitions"
    );
    std::process::exit(2);
}
