use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rustc-check-cfg=cfg(app_builder_task_check)");
    println!("cargo:rustc-cfg=app_builder_task_check");
    println!("cargo:rerun-if-changed=../tasks");
    println!("cargo:rerun-if-changed=../targets/nucleo_f401re/src/board.rs");
    println!("cargo:rerun-if-changed=../targets/nucleo_f401re/src/app_composition.rs");

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("task-check manifest directory is set"),
    );
    let repository_root = manifest_dir
        .parent()
        .expect("task-check crate has an app-builder parent");
    let destination = PathBuf::from(env::var_os("OUT_DIR").expect("task-check OUT_DIR is set"))
        .join("task_checks.rs");
    if let Err(error) = xtask::write_task_checks(repository_root, &destination) {
        panic!("failed to generate task checks: {error:#}");
    }
}
