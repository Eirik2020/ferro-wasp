//! Selected-target RTIC generation orchestration.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use crate::{
    rtic::{render, report, resolve},
    target::{app_composition::APP_COMPOSITION, golden_reconciliation},
};

/// Generated RTIC application path relative to the App Builder V3 root.
pub const GENERATED_APP_PATH: &str = "generated/src/main.rs";
/// Generated RTIC import prelude path relative to the App Builder V3 root.
pub const GENERATED_PRELUDE_PATH: &str = "generated/src/prelude.rs";
/// Generated boot-time platform configuration path relative to the App Builder V3 root.
pub const GENERATED_PLATFORM_CONFIG_PATH: &str = "generated/src/platform_config.rs";
/// Generated safety-spine review report path relative to the App Builder V3 root.
pub const GENERATED_SAFETY_SPINE_PATH: &str = "generated/SAFETY_SPINE.md";

struct FormattedSources {
    main: String,
    prelude: String,
    platform_config: String,
    safety_spine: String,
}

/// Resolves and renders the selected application without writing it.
pub fn render_selected(source_root: &Path) -> Result<String> {
    Ok(render_selected_sources(source_root)?.main)
}

fn render_selected_sources(source_root: &Path) -> Result<FormattedSources> {
    let resolved = resolve::resolve(&APP_COMPOSITION).context("resolve selected application")?;
    golden_reconciliation::validate(&resolved)
        .context("validate selected output-inhibited safety spine")?;
    let rendered = render::render_sources(source_root, &resolved)
        .context("render selected RTIC application")?;
    let mut safety_spine = report::render(&resolved);
    safety_spine.push_str(&golden_reconciliation::render());
    Ok(FormattedSources {
        main: format_rust_source(&rendered.main).context("format selected RTIC application")?,
        prelude: format_rust_source(&rendered.prelude).context("format selected RTIC prelude")?,
        platform_config: format_rust_source(&rendered.platform_config)
            .context("format selected platform configuration")?,
        safety_spine,
    })
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
    let rendered = render_selected_sources(source_root)?;
    let destination = source_root.join(GENERATED_APP_PATH);
    let prelude_destination = source_root.join(GENERATED_PRELUDE_PATH);
    let platform_config_destination = source_root.join(GENERATED_PLATFORM_CONFIG_PATH);
    let safety_spine_destination = source_root.join(GENERATED_SAFETY_SPINE_PATH);
    let parent = destination
        .parent()
        .context("generated application path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create generated directory {}", parent.display()))?;
    write_if_changed(&destination, &rendered.main, "RTIC app")?;
    write_if_changed(&prelude_destination, &rendered.prelude, "RTIC prelude")?;
    write_if_changed(
        &platform_config_destination,
        &rendered.platform_config,
        "platform configuration",
    )?;
    write_if_changed(
        &safety_spine_destination,
        &rendered.safety_spine,
        "safety-spine report",
    )?;
    Ok(destination)
}

fn write_if_changed(destination: &Path, source: &str, label: &str) -> Result<()> {
    let unchanged = fs::read_to_string(destination).is_ok_and(|existing| existing == source);
    if !unchanged {
        fs::write(destination, source)
            .with_context(|| format!("write generated {label} {}", destination.display()))?;
    }
    Ok(())
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
        let rendered = render_selected_sources(&source_root()).unwrap();
        assert!(rendered.main.contains("#[rtic::app("));
        assert!(rendered.main.contains("mod prelude;"));
        assert!(rendered.main.contains("mod platform_config;"));
        assert!(rendered.main.contains("use crate::prelude::*;"));
        assert!(!rendered.main.contains("fn load_platform_config"));
        assert!(rendered.main.contains("fn serial2_rx_idle_irq"));
        assert!(rendered.main.contains("binds = TIM4"));
        assert!(rendered.main.contains("fn control_loop"));
        assert!(
            rendered
                .main
                .contains("flight_controller: dt::FlightController")
        );
        assert!(!rendered.main.lines().any(|line| line.ends_with(' ')));
        assert!(
            rendered
                .prelude
                .contains("pub(crate) use ferrowasp_io_core")
        );
        assert!(!rendered.prelude.lines().any(|line| line.ends_with(' ')));
        assert!(
            rendered
                .platform_config
                .contains("pub(crate) fn load_platform_config()")
        );
        assert!(
            !rendered
                .platform_config
                .lines()
                .any(|line| line.ends_with(' '))
        );
        assert!(rendered.safety_spine.contains("## Task and priority map"));
        assert!(
            rendered
                .safety_spine
                .contains("## Golden Foxeer reconciliation")
        );
        assert!(
            rendered
                .safety_spine
                .contains("safety policy and physical component independently disable output")
        );
        assert!(rendered.safety_spine.contains("Tim1Ch3N"));
        assert_eq!(format_rust_source(&rendered.main).unwrap(), rendered.main);
        assert_eq!(
            format_rust_source(&rendered.prelude).unwrap(),
            rendered.prelude
        );
        assert_eq!(
            format_rust_source(&rendered.platform_config).unwrap(),
            rendered.platform_config
        );
    }
}
