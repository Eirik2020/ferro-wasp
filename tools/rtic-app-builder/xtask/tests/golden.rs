use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use xtask::{
    backend,
    feature::load_feature_bundle,
    manifest,
    render::{RenderedCrate, TemplateSet, render_crate},
    runner::{ProcessRunner, run_cargo_fmt},
    validate::validate_manifest,
};

const GOLDEN_FILES: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "memory.x",
    "src/main.rs",
];

#[test]
fn valid_blink_application_matches_golden_bytes_deterministically() {
    let repository_root = repository_root();

    let first = render_valid_blink_application(&repository_root);
    let second = render_valid_blink_application(&repository_root);
    assert_eq!(
        first, second,
        "rendering the same inputs twice must be identical"
    );

    let expected_paths = GOLDEN_FILES
        .map(PathBuf::from)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let actual_paths = first.files.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(actual_paths, expected_paths, "unexpected rendered file set");

    let golden_root = repository_root.join("tests/golden/nucleo-f401re-blinky");
    assert_eq!(
        collect_files(&golden_root),
        expected_paths,
        "golden fixture must contain exactly the generated text files"
    );

    let temporary = tempfile::tempdir().expect("create temporary render directories");
    let rendered_root = temporary.path().join("rendered-first");
    let second_root = temporary.path().join("rendered-second");
    first
        .write_to(&rendered_root)
        .expect("materialize rendered application");
    second
        .write_to(&second_root)
        .expect("materialize second rendered application");
    format_rendered_crate(&rendered_root);
    format_rendered_crate(&second_root);

    for relative in GOLDEN_FILES.map(PathBuf::from) {
        let actual = fs::read(rendered_root.join(&relative))
            .unwrap_or_else(|error| panic!("read rendered {}: {error}", relative.display()));
        let repeated = fs::read(second_root.join(&relative))
            .unwrap_or_else(|error| panic!("read repeated render {}: {error}", relative.display()));
        let expected = fs::read(golden_root.join(&relative))
            .unwrap_or_else(|error| panic!("read golden {}: {error}", relative.display()));
        assert_eq!(
            actual,
            repeated,
            "formatted output is not deterministic for {}",
            relative.display()
        );
        assert_eq!(
            actual,
            expected,
            "rendered bytes differ from golden fixture for {}",
            relative.display()
        );
    }
}

#[test]
fn nucleo_osd_renders_divergent_channel_consumers_and_typed_faults() {
    let rendered = render_application(&repository_root(), "nucleo-f401re-osd");
    let main = std::str::from_utf8(
        rendered
            .files
            .get(Path::new("src/main.rs"))
            .expect("rendered OSD main source"),
    )
    .expect("rendered OSD main source is UTF-8");
    let cargo = std::str::from_utf8(
        rendered
            .files
            .get(Path::new("Cargo.toml"))
            .expect("rendered OSD Cargo manifest"),
    )
    .expect("rendered OSD Cargo manifest is UTF-8");

    for required in [
        "make_channel!(OsdWork<70>, 4)",
        "async fn osd_displayport",
        "async fn usart1_tx_worker",
        "recv().await",
        "try_send",
        "OsdFaultId",
        "telemetry.snapshot()",
        "process_work(",
    ] {
        assert!(
            main.contains(required),
            "rendered OSD source is missing `{required}`"
        );
    }
    assert!(!main.contains("SerialRxTx"));
    assert!(!main.contains(".spawn().ok()"));
    assert!(cargo.contains("rtic-sync = \"=1.5.0\""));
}

fn format_rendered_crate(root: &Path) {
    let output = run_cargo_fmt(&ProcessRunner, root, &root.join("Cargo.toml"), None)
        .expect("start pinned cargo fmt for golden candidate");
    assert!(
        output.success(),
        "cargo fmt failed for golden candidate: {}",
        output.stderr_lossy()
    );
}

fn render_valid_blink_application(repository_root: &Path) -> RenderedCrate {
    render_application(repository_root, "nucleo-f401re-blinky")
}

fn render_application(repository_root: &Path, application: &str) -> RenderedCrate {
    let manifest_path = repository_root
        .join("applications")
        .join(format!("{application}.toml"));
    let bsp_path = repository_root.join("bsp/nucleo-f401re.toml");
    let manifest =
        manifest::load(&manifest_path, &bsp_path).expect("load valid blink manifest set");
    validate_manifest(&manifest).expect("validate blink manifest");

    let mut resolved = Vec::new();
    for feature_name in &manifest.application.feature_order {
        let feature = manifest
            .feature(feature_name)
            .expect("declared feature exists");
        let bundle = load_feature_bundle(
            &repository_root.join("feature-library"),
            feature.implementation(),
        )
        .expect("load feature bundle");
        resolved.push(
            backend::resolve_feature(&manifest, feature_name, &bundle)
                .expect("resolve feature through the STM32F4 backend"),
        );
    }
    backend::validate_resolved_claims(&resolved).expect("validate resolved feature claims");

    let templates = TemplateSet::load(repository_root).expect("load application templates");
    render_crate(&manifest, &templates, &resolved).expect("render blink application")
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate has a repository parent")
        .to_path_buf()
}

fn collect_files(root: &Path) -> BTreeSet<PathBuf> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeSet<PathBuf>) {
        let entries = fs::read_dir(directory).unwrap_or_else(|error| {
            panic!("read fixture directory {}: {error}", directory.display())
        });
        for entry in entries {
            let entry = entry.expect("read fixture entry");
            let path = entry.path();
            let file_type = entry
                .file_type()
                .unwrap_or_else(|error| panic!("inspect fixture {}: {error}", path.display()));
            if file_type.is_dir() {
                visit(root, &path, files);
            } else if file_type.is_file() {
                files.insert(
                    path.strip_prefix(root)
                        .expect("fixture entry stays below fixture root")
                        .to_path_buf(),
                );
            } else {
                panic!(
                    "fixture entry is neither a file nor directory: {}",
                    path.display()
                );
            }
        }
    }

    let mut files = BTreeSet::new();
    visit(root, root, &mut files);
    files
}
