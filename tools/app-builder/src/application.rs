//! Unified selection, resolution, and rendering for complete applications.

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use crate::{
    input_catalog::{self, ApplicationEntry, ApplicationInput},
    rtic::{render as rtic_render, report, resolve},
    validation::{self, backend::ResolvedFeature, manifest::Manifest},
};

/// One resolved graph produced through the canonical application pipeline.
pub enum ResolvedGraph {
    /// Rich typed RTIC graph compiled from repository-owned Rust declarations.
    Typed(resolve::ResolvedApp),
    /// Strict schema-validated NUCLEO compatibility graph.
    Validation {
        /// Fully combined and validated application/BSP manifest.
        manifest: Manifest,
        /// Deterministically ordered, validated implementation recipes.
        features: Vec<ResolvedFeature>,
    },
}

/// One selected catalog entry and its completely validated graph.
pub struct ResolvedApplication {
    /// Catalog identity and build-facing metadata.
    pub entry: &'static ApplicationEntry,
    /// Resolved graph ready for deterministic rendering.
    pub graph: ResolvedGraph,
}

/// Deterministic set of generated files for one complete application.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RenderedApplication {
    /// Output files keyed by safe application-relative paths.
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

impl RenderedApplication {
    /// Returns one generated UTF-8 file when present.
    pub fn text(&self, path: impl AsRef<Path>) -> Result<&str> {
        let path = path.as_ref();
        let bytes = self
            .files
            .get(path)
            .with_context(|| format!("rendered application has no `{}`", path.display()))?;
        std::str::from_utf8(bytes)
            .with_context(|| format!("rendered file `{}` is not UTF-8", path.display()))
    }

    fn insert_text(&mut self, path: impl Into<PathBuf>, text: String) {
        self.files.insert(path.into(), text.into_bytes());
    }
}

/// Selects and fully resolves one allowlisted application without mutation.
pub fn resolve(builder_root: &Path, application_id: &str) -> Result<ResolvedApplication> {
    let repository_root = builder_root
        .parent()
        .and_then(Path::parent)
        .context("App Builder root must be nested below the repository tools directory")?;
    input_catalog::validate(repository_root).context("validate explicit Rust input catalog")?;
    input_catalog::validate_applications(builder_root)
        .context("validate explicit application catalog")?;
    let entry = input_catalog::application(application_id)?;

    let graph = match entry.input {
        ApplicationInput::RustComposition {
            composition,
            reconcile,
            ..
        } => {
            let resolved = resolve::resolve(composition)
                .with_context(|| format!("resolve typed application `{application_id}`"))?;
            reconcile(&resolved)
                .with_context(|| format!("reconcile typed application `{application_id}`"))?;
            ResolvedGraph::Typed(resolved)
        }
        ApplicationInput::ValidationManifest {
            application_path,
            bsp_path,
        } => {
            let manifest = validation::manifest::load(
                &builder_root.join(application_path),
                &builder_root.join(bsp_path),
            )
            .with_context(|| format!("load strict inputs for `{application_id}`"))?;
            if manifest.application.name != entry.id {
                bail!(
                    "catalog application `{}` resolves to manifest `{}`",
                    entry.id,
                    manifest.application.name
                );
            }
            validation::validate::validate_manifest(&manifest)
                .with_context(|| format!("validate strict manifest for `{application_id}`"))?;
            validation::architecture::validate_for_manifest(builder_root, &manifest).with_context(
                || format!("validate architecture contract for `{application_id}`"),
            )?;
            let features = resolve_validation_features(builder_root, &manifest)?;
            validation::backend::validate_resolved_claims(&features)
                .with_context(|| format!("validate physical claims for `{application_id}`"))?;
            ResolvedGraph::Validation { manifest, features }
        }
    };

    Ok(ResolvedApplication { entry, graph })
}

fn resolve_validation_features(
    builder_root: &Path,
    manifest: &Manifest,
) -> Result<Vec<ResolvedFeature>> {
    let library_root = builder_root.join("feature-library");
    let mut resolved = Vec::with_capacity(manifest.application.feature_order.len());
    for feature_name in &manifest.application.feature_order {
        let configuration = manifest.feature(feature_name).ok_or_else(|| {
            anyhow::anyhow!(
                "feature `{feature_name}` is ordered but has no configuration after validation"
            )
        })?;
        let bundle =
            validation::feature::load_feature_bundle(&library_root, configuration.implementation())
                .with_context(|| format!("load feature `{feature_name}`"))?;
        resolved.push(
            validation::backend::resolve_feature(manifest, feature_name, &bundle)
                .with_context(|| format!("resolve feature `{feature_name}`"))?,
        );
    }
    Ok(resolved)
}

/// Renders one completely resolved application through the canonical entry point.
pub fn render(builder_root: &Path, app: &ResolvedApplication) -> Result<RenderedApplication> {
    match (&app.entry.input, &app.graph) {
        (
            ApplicationInput::RustComposition {
                report: appendix,
                live_config,
                ..
            },
            ResolvedGraph::Typed(resolved),
        ) => {
            let rendered = rtic_render::render_sources(builder_root, resolved)
                .with_context(|| format!("render typed application `{}`", app.entry.id))?;
            let mut safety_spine = report::render(resolved);
            safety_spine.push_str(&appendix());

            let mut output = RenderedApplication::default();
            output.insert_text(
                "src/main.rs",
                format_rust_source(&rendered.main).context("format generated RTIC application")?,
            );
            output.insert_text(
                "src/prelude.rs",
                format_rust_source(&rendered.prelude).context("format generated RTIC prelude")?,
            );
            output.insert_text(
                "src/platform_config.rs",
                format_rust_source(&rendered.platform_config)
                    .context("format generated platform configuration")?,
            );
            output.insert_text(
                "src/live_config.rs",
                format_rust_source(&rtic_render::render_live_config(
                    live_config.initial_tuning_request_seq,
                    live_config.initial_flash_log_rate_divisor,
                ))
                .context("format generated live configuration")?,
            );
            output.insert_text("SAFETY_SPINE.md", safety_spine);
            insert_foxeer_package_files(builder_root, &mut output)?;
            Ok(output)
        }
        (
            ApplicationInput::ValidationManifest { .. },
            ResolvedGraph::Validation { manifest, features },
        ) => {
            let templates = validation::render::TemplateSet::load(builder_root)?;
            let rendered = validation::render::render_crate(manifest, &templates, features)?;
            validation::syntax::validate_rendered_rust(&rendered)?;
            let mut output = RenderedApplication {
                files: rendered.files,
            };
            let rust_paths = output
                .files
                .keys()
                .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
                .cloned()
                .collect::<Vec<_>>();
            for path in rust_paths {
                let source = output.text(&path)?;
                let formatted = format_rust_source(source)
                    .with_context(|| format!("format generated Rust {}", path.display()))?;
                output.insert_text(path, formatted);
            }
            Ok(output)
        }
        _ => bail!("application input and resolved graph variants do not match"),
    }
}

fn insert_foxeer_package_files(
    builder_root: &Path,
    output: &mut RenderedApplication,
) -> Result<()> {
    const PACKAGE_FILES: [&str; 5] = [
        ".cargo/config.toml",
        "Cargo.lock",
        "Cargo.toml",
        "build.rs",
        "memory.x",
    ];

    let template_root = builder_root.join("templates/foxeer-f405-v2");
    for relative in PACKAGE_FILES {
        let source = template_root.join(relative);
        let contents = std::fs::read(&source)
            .with_context(|| format!("read Foxeer package template {}", source.display()))?;
        output.files.insert(PathBuf::from(relative), contents);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn builder_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_owned()
    }

    #[test]
    fn every_catalog_application_resolves_and_renders_deterministically() {
        let root = builder_root();
        for id in input_catalog::application_names() {
            let resolved = resolve(&root, id).unwrap();
            let first = render(&root, &resolved).unwrap();
            let second = render(&root, &resolved).unwrap();
            assert_eq!(first, second, "rendering `{id}` was not deterministic");
            assert!(first.files.contains_key(Path::new("src/main.rs")));
        }
    }

    #[test]
    fn nucleo_blinky_preserves_the_checked_golden_crate() {
        const FILES: [&str; 5] = [
            "Cargo.lock",
            "Cargo.toml",
            "build.rs",
            "memory.x",
            "src/main.rs",
        ];

        let root = builder_root();
        let resolved = resolve(&root, "nucleo-f401re-blinky").unwrap();
        let rendered = render(&root, &resolved).unwrap();
        assert_eq!(
            rendered.files.keys().cloned().collect::<Vec<_>>(),
            FILES.map(PathBuf::from)
        );
        for relative in FILES {
            let expected = std::fs::read(
                root.join("tests/golden/nucleo-f401re-blinky")
                    .join(relative),
            )
            .unwrap();
            assert_eq!(
                rendered.files[Path::new(relative)],
                expected,
                "NUCLEO golden fixture drifted at {relative}"
            );
        }
    }

    #[test]
    fn unknown_application_fails_before_rendering() {
        let error = resolve(&builder_root(), "not-in-the-catalog")
            .err()
            .expect("unknown selection must fail");
        assert!(error.to_string().contains("unknown application"));
    }

    #[test]
    fn same_type_foxeer_serial_instances_have_collision_free_identities() {
        let resolved = resolve(&builder_root(), "foxeer-f405-v2").unwrap();
        let ResolvedGraph::Typed(graph) = resolved.graph else {
            panic!("Foxeer must resolve through the typed graph");
        };
        assert_eq!(graph.serial_endpoints.len(), 2);

        let ids = graph
            .serial_endpoints
            .iter()
            .map(|endpoint| endpoint.declaration.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            ids,
            std::collections::BTreeSet::from(["serial1", "serial2"])
        );

        let resources = graph
            .serial_endpoints
            .iter()
            .flat_map(|endpoint| {
                endpoint
                    .resources
                    .iter()
                    .map(|resource| resource.id.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            resources.len(),
            resources
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            "same-type component instances produced colliding resource IDs"
        );
    }
}
