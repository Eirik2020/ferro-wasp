mod app;
mod backend;
mod board;
mod generator;
mod resolve;
mod task;
#[path = "../../tasks/task-declarations/mod.rs"]
mod task_declarations;

use std::{env, path::PathBuf};

use anyhow::{Result, bail};

pub fn run() -> Result<()> {
    let mut arguments = env::args().skip(1);
    match (arguments.next().as_deref(), arguments.next()) {
        (Some("generate"), None) => generator::generate(&repository_root()),
        _ => bail!("usage: cargo xtask generate"),
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate has a parent directory")
        .to_path_buf()
}
