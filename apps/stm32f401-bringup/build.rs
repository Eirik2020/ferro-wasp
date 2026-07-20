use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set by Cargo"));
    File::create(out.join("memory.x"))
        .expect("memory.x output must be creatable")
        .write_all(include_bytes!("memory.x"))
        .expect("memory.x must be writable");

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
}
