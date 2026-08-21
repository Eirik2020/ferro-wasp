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
    use std::fmt::Write as _;

    use ferrowasp_core::config::ConfigKey;
    use ferrowasp_tasks::{
        drone_toolbox::RC_RATE_PROFILE,
        flash_storage::{LEGACY_STORED_CONFIG_LEN, STORED_CONFIG_LEN, StoredConfig},
    };

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

    #[test]
    fn selected_target_matches_checked_golden_outputs_deterministically() {
        let root = source_root();
        let first = render_selected_sources(&root).unwrap();
        let second = render_selected_sources(&root).unwrap();

        for (label, first, second, relative_path) in [
            (
                "RTIC application",
                first.main.as_str(),
                second.main.as_str(),
                GENERATED_APP_PATH,
            ),
            (
                "RTIC prelude",
                first.prelude.as_str(),
                second.prelude.as_str(),
                GENERATED_PRELUDE_PATH,
            ),
            (
                "platform configuration",
                first.platform_config.as_str(),
                second.platform_config.as_str(),
                GENERATED_PLATFORM_CONFIG_PATH,
            ),
            (
                "safety-spine report",
                first.safety_spine.as_str(),
                second.safety_spine.as_str(),
                GENERATED_SAFETY_SPINE_PATH,
            ),
        ] {
            assert_eq!(first, second, "repeated {label} rendering drifted");
            let checked = fs::read_to_string(root.join(relative_path))
                .unwrap_or_else(|error| panic!("read checked {label} fixture: {error}"));
            assert_eq!(first, checked, "checked {label} fixture drifted");
        }
    }

    #[test]
    fn authoritative_live_configuration_contract_matches_golden_snapshot() {
        let default = StoredConfig::foxeer_f405_v2_default();
        let encoded = default.encode();
        let mut legacy = [0_u8; LEGACY_STORED_CONFIG_LEN];
        legacy.copy_from_slice(&encoded[..LEGACY_STORED_CONFIG_LEN]);
        legacy[42..44].fill(0);
        let migrated = StoredConfig::decode(&legacy).unwrap();

        let mut actual = String::new();
        writeln!(
            actual,
            "stored_config_schema_version={}",
            u16::from_le_bytes([encoded[42], encoded[43]])
        )
        .unwrap();
        writeln!(actual, "stored_config_len={STORED_CONFIG_LEN}").unwrap();
        writeln!(
            actual,
            "legacy_stored_config_len={LEGACY_STORED_CONFIG_LEN}"
        )
        .unwrap();
        writeln!(actual, "initial_tuning_request_seq=1").unwrap();
        writeln!(
            actual,
            "default_log_rate_divisor={}",
            default.log_rate_divisor
        )
        .unwrap();
        writeln!(
            actual,
            "legacy_migration_preserves_current_rc_defaults={}",
            migrated.tuning.rc_rates == RC_RATE_PROFILE
        )
        .unwrap();
        for key in ConfigKey::ALL {
            let spec = key.value_spec();
            writeln!(
                actual,
                "{}|{:.8}|{:.8}|{}|{:.8}",
                key.name(),
                spec.minimum,
                spec.maximum,
                spec.integer,
                default.get(key)
            )
            .unwrap();
        }

        let expected = include_str!("../../tests/golden/foxeer-f405-v2-live-config.txt");
        assert_eq!(actual, expected);

        let rendered = render_selected_sources(&source_root()).unwrap();
        assert!(
            rendered
                .main
                .contains("let tuning_profile = dt::TuningProfile::default_foxeer_f405_v2();")
        );
        assert!(rendered.main.contains("let tuning_request_seq = 1;"));
        assert!(rendered.main.contains("let flash_log_rate_divisor = 1;"));
    }
}
