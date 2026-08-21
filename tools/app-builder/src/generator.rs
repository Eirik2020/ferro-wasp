//! Selected-application generation orchestration.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::application::{self, RenderedApplication};

/// Root directory containing per-application working outputs.
pub const GENERATED_ROOT: &str = "generated";
/// Last rendered candidate directory below an application's output root.
pub const WORKING_DIRECTORY: &str = "working";

#[cfg(test)]
struct FormattedSources {
    main: String,
    prelude: String,
    platform_config: String,
    live_config: String,
    safety_spine: String,
}

/// Resolves and renders one explicit catalog application without writing it.
pub fn render_selected(builder_root: &Path, application_id: &str) -> Result<RenderedApplication> {
    let resolved = application::resolve(builder_root, application_id)?;
    application::render(builder_root, &resolved)
}

#[cfg(test)]
fn render_selected_sources(builder_root: &Path) -> Result<FormattedSources> {
    let rendered = render_selected(builder_root, "foxeer-f405-v2")?;
    Ok(FormattedSources {
        main: rendered.text("src/main.rs")?.to_owned(),
        prelude: rendered.text("src/prelude.rs")?.to_owned(),
        platform_config: rendered.text("src/platform_config.rs")?.to_owned(),
        live_config: rendered.text("src/live_config.rs")?.to_owned(),
        safety_spine: rendered.text("SAFETY_SPINE.md")?.to_owned(),
    })
}

/// Resolves, renders, and writes one explicit application after all validation.
pub fn generate(builder_root: &Path, application_id: &str) -> Result<PathBuf> {
    let rendered = render_selected(builder_root, application_id)?;
    let destination = working_directory(builder_root, application_id)?;
    write_rendered(&destination, &rendered)?;
    Ok(destination)
}

/// Returns the confined working output directory for one safe catalog ID.
pub fn working_directory(builder_root: &Path, application_id: &str) -> Result<PathBuf> {
    if application_id.is_empty()
        || application_id.chars().any(|character| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        })
    {
        bail!("application ID is not a safe output component: `{application_id}`");
    }
    Ok(builder_root
        .join(GENERATED_ROOT)
        .join(application_id)
        .join(WORKING_DIRECTORY))
}

fn write_rendered(destination: &Path, rendered: &RenderedApplication) -> Result<()> {
    for (relative, contents) in &rendered.files {
        validate_relative_output(relative)?;
        let path = destination.join(relative);
        let parent = path
            .parent()
            .context("generated application path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("create generated directory {}", parent.display()))?;
        write_if_changed(&path, contents)?;
    }
    Ok(())
}

fn validate_relative_output(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("renderer produced unsafe output path `{}`", path.display());
    }
    Ok(())
}

fn write_if_changed(destination: &Path, source: &[u8]) -> Result<()> {
    let unchanged = fs::read(destination).is_ok_and(|existing| existing == source);
    if !unchanged {
        fs::write(destination, source)
            .with_context(|| format!("write generated file {}", destination.display()))?;
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
        Path::new(env!("CARGO_MANIFEST_DIR")).to_owned()
    }

    #[test]
    fn selected_target_renders_without_filesystem_output() {
        let rendered = render_selected_sources(&source_root()).unwrap();
        assert!(rendered.main.contains("#[rtic::app("));
        assert!(rendered.main.contains("mod prelude;"));
        assert!(rendered.main.contains("mod platform_config;"));
        assert!(rendered.main.contains("mod live_config;"));
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
        assert!(
            rendered
                .live_config
                .contains("pub(crate) const fn initial_stored_config()")
        );
        assert!(!rendered.live_config.lines().any(|line| line.ends_with(' ')));
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
        syn::parse_file(&rendered.main).unwrap();
        syn::parse_file(&rendered.prelude).unwrap();
        syn::parse_file(&rendered.platform_config).unwrap();
        syn::parse_file(&rendered.live_config).unwrap();
    }

    #[test]
    fn selected_target_matches_checked_golden_outputs_deterministically() {
        let root = source_root();
        let first = render_selected_sources(&root).unwrap();
        let second = render_selected_sources(&root).unwrap();

        for (label, first, second, fixture_name) in [
            (
                "RTIC application",
                first.main.as_str(),
                second.main.as_str(),
                "main.rs",
            ),
            (
                "RTIC prelude",
                first.prelude.as_str(),
                second.prelude.as_str(),
                "prelude.rs",
            ),
            (
                "platform configuration",
                first.platform_config.as_str(),
                second.platform_config.as_str(),
                "platform_config.rs",
            ),
            (
                "live configuration",
                first.live_config.as_str(),
                second.live_config.as_str(),
                "live_config.rs",
            ),
            (
                "safety-spine report",
                first.safety_spine.as_str(),
                second.safety_spine.as_str(),
                "SAFETY_SPINE.md",
            ),
        ] {
            assert_eq!(first, second, "repeated {label} rendering drifted");
            let checked =
                fs::read_to_string(root.join("tests/golden/foxeer-f405-v2").join(fixture_name))
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

        let expected = include_str!("../tests/golden/foxeer-f405-v2-live-config.txt");
        assert_eq!(actual, expected);

        let rendered = render_selected_sources(&source_root()).unwrap();
        assert!(
            rendered
                .main
                .contains("let tuning_profile = initial_tuning_profile();")
        );
        assert!(
            rendered
                .main
                .contains("let tuning_request_seq = INITIAL_TUNING_REQUEST_SEQ;")
        );
        assert!(
            rendered
                .main
                .contains("let flash_log_rate_divisor = INITIAL_FLASH_LOG_RATE_DIVISOR;")
        );
        assert!(
            rendered
                .live_config
                .contains("INITIAL_TUNING_REQUEST_SEQ: u32 = 1;")
        );
        assert!(
            rendered
                .live_config
                .contains("INITIAL_FLASH_LOG_RATE_DIVISOR: u32 = 1;")
        );
    }
}
