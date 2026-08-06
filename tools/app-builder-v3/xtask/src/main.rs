//! Command-line entry point for the RTIC application generator.

#![deny(missing_docs)]

fn main() {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next();
    if arguments.next().is_some() {
        usage();
    }
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest directory has an App Builder V3 parent");
    let result = match command.as_deref() {
        Some("generate") => xtask::generator::generate(source_root).map(|path| {
            println!("generated RTIC application: {}", path.display());
        }),
        Some("check") => xtask::generator::render_selected(source_root).map(|_| {
            println!("selected application resolves and renders successfully");
        }),
        _ => usage(),
    };
    if let Err(error) = result {
        eprintln!("generation failed: {error:#}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!("usage: cargo run -p xtask -- <generate|check>");
    std::process::exit(2);
}
