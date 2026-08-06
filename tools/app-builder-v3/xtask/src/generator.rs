//! Selected-target RTIC generation orchestration.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use crate::{
    rtic::{render, resolve},
    target::app_composition::APP_COMPOSITION,
};

/// Generated RTIC application path relative to the App Builder V3 root.
pub const GENERATED_APP_PATH: &str = "generated/src/main.rs";

/// Resolves and renders the selected application without writing it.
pub fn render_selected(source_root: &Path) -> Result<String> {
    let resolved = resolve::resolve(&APP_COMPOSITION).context("resolve selected application")?;
    let rendered =
        render::render(source_root, &resolved).context("render selected RTIC application")?;
    format_rust_source(&rendered).context("format selected RTIC application")
}

fn format_rust_source(source: &str) -> Result<String> {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start rustfmt; install the rustfmt Rust component")?;
    child
        .stdin
        .take()
        .context("open rustfmt standard input")?
        .write_all(source.as_bytes())
        .context("send generated source to rustfmt")?;
    let output = child.wait_with_output().context("wait for rustfmt")?;
    if !output.status.success() {
        bail!(
            "rustfmt exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("rustfmt returned non-UTF-8 source")
}

/// Resolves, renders, and writes the selected RTIC application.
pub fn generate(source_root: &Path) -> Result<PathBuf> {
    let rendered = render_selected(source_root)?;
    let destination = source_root.join(GENERATED_APP_PATH);
    let parent = destination
        .parent()
        .context("generated application path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create generated directory {}", parent.display()))?;
    let unchanged = fs::read_to_string(&destination).is_ok_and(|existing| existing == rendered);
    if !unchanged {
        fs::write(&destination, rendered)
            .with_context(|| format!("write generated RTIC app {}", destination.display()))?;
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn selected_target_renders_without_filesystem_output() {
        let rendered = render_selected(&source_root()).unwrap();
        assert!(rendered.contains("#[rtic::app("));
        assert!(rendered.contains("fn osd_uart_rx_idle_irq"));
        assert!(!rendered.lines().any(|line| line.ends_with(' ')));
        assert_eq!(format_rust_source(&rendered).unwrap(), rendered);
    }
}
