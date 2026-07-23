//! This build script copies the app's `memory.x` file into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn git_value(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn main() {
    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    let revision =
        git_value(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let build_date = git_value(&[
        "show",
        "-s",
        "--format=%cd",
        "--date=format:%b %d %Y",
        "HEAD",
    ])
    .unwrap_or_else(|| "Jan 01 1970".to_owned());
    let build_time = git_value(&[
        "show",
        "-s",
        "--format=%cd",
        "--date=format:%H:%M:%S",
        "HEAD",
    ])
    .unwrap_or_else(|| "00:00:00".to_owned());
    println!("cargo:rustc-env=FWSP_GIT_REV={revision}");
    println!("cargo:rustc-env=FWSP_BUILD_DATE={build_date}");
    println!("cargo:rustc-env=FWSP_BUILD_TIME={build_time}");
}
