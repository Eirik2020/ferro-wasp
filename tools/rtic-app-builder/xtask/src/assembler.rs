use std::{
    collections::BTreeSet,
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::{Context, Result, anyhow, bail};

use crate::{
    BACKEND_VERSION, GENERATOR_VERSION,
    backend::{self, ResolvedFeature},
    cli::{Cli, Command},
    diagnostics,
    feature::load_feature_bundle,
    manifest::{self, Manifest},
    mcu,
    render::{self, RenderedCrate, TemplateSet},
    runner::{
        CommandOutput, CommandRunner, ProcessRunner, run_cargo_check, run_cargo_fmt,
        run_cargo_fmt_check, run_cargo_release_build,
    },
    state::{
        BUILD_STATE_SCHEMA_VERSION, BuildState, hash_labeled_bytes, hash_labeled_files,
        next_failed_candidate_path, promote_candidate, read_build_state, write_atomic_file,
        write_build_state,
    },
    syntax,
    validate::validate_manifest,
};

#[cfg(test)]
use crate::runner::FIRMWARE_TARGET;

const FEATURE_LIBRARY: &str = "feature-library";
const GENERATED_DIR: &str = "generated";
const WORKING_DIR: &str = "working";
const STATE_FILE: &str = "build-state.toml";
const TOOLCHAIN: &str = "nightly-2025-12-13";
const GENERATED_PACKAGE: &str = "rtic-generated-app";

pub fn execute(cli: Cli) -> Result<()> {
    let root = repository_root()?;
    match cli.command {
        Command::Generate {
            app,
            manifest,
            bsp,
            resume,
        } => match (app, manifest, bsp) {
            (Some(app), None, None) => {
                let (manifest, bsp) = resolve_named_application_paths(&root, &app)?;
                generate(&root, &manifest, &bsp, resume, &ProcessRunner)
            }
            (None, Some(manifest), Some(bsp)) => {
                let manifest = resolve_input_path(&root, &manifest, "applications")?;
                let bsp = resolve_input_path(&root, &bsp, "bsp")?;
                generate(&root, &manifest, &bsp, resume, &ProcessRunner)
            }
            _ => bail!(
                "generate requires either `--app <name>` or both `--manifest <path-or-name>` and `--bsp <path-or-name>`"
            ),
        },
        Command::Build { app } => {
            prepare_named_application(&root, &app, &ProcessRunner)?;
            Ok(())
        }
        Command::Flash { app } => flash(&root, &app, &ProcessRunner),
        Command::Embed { app } => embed(&root, &app, &ProcessRunner),
        Command::Clean { app } => clean(&root, &app),
    }
}

fn repository_root() -> Result<PathBuf> {
    let xtask = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    xtask
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("xtask manifest directory has no repository parent"))
}

fn resolve_input_path(repository_root: &Path, path: &Path, catalog: &str) -> Result<PathBuf> {
    let is_bare_name = path.components().count() == 1 && path.extension().is_none();
    let absolute = if is_bare_name {
        repository_root
            .join(catalog)
            .join(path)
            .with_extension("toml")
    } else if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .context("read current directory")?
            .join(path)
    };
    fs::canonicalize(&absolute)
        .with_context(|| format!("resolve input manifest {}", absolute.display()))
}

struct PreparedApplication {
    binary: PathBuf,
    profile: mcu::McuProfile,
}

fn resolve_named_application_paths(
    repository_root: &Path,
    application: &str,
) -> Result<(PathBuf, PathBuf)> {
    if !is_safe_application_slug(application) {
        bail!("refusing unsafe application name: {application:?}");
    }

    let application_path =
        resolve_input_path(repository_root, Path::new(application), "applications")?;
    let application_document = manifest::load_application(&application_path)?;
    require_matching_application_name(application, &application_document.application.name)?;
    let bsp_path = resolve_input_path(
        repository_root,
        Path::new(&application_document.application.bsp),
        "bsp",
    )?;
    Ok((application_path, bsp_path))
}

fn prepare_named_application<R: CommandRunner + ?Sized>(
    repository_root: &Path,
    application: &str,
    command_runner: &R,
) -> Result<PreparedApplication> {
    let (application_path, bsp_path) =
        resolve_named_application_paths(repository_root, application)?;

    generate(
        repository_root,
        &application_path,
        &bsp_path,
        false,
        command_runner,
    )?;

    let resolved = manifest::load(&application_path, &bsp_path)?;
    validate_manifest(&resolved)?;
    let profile = *mcu::profile(&resolved.bsp.mcu)?;
    let target_dir = repository_root
        .join(GENERATED_DIR)
        .join(application)
        .join("target");
    let binary = locate_binary(&target_dir, profile.rust_target)?;

    Ok(PreparedApplication { binary, profile })
}

fn flash<R: CommandRunner + ?Sized>(
    repository_root: &Path,
    application: &str,
    command_runner: &R,
) -> Result<()> {
    let prepared = prepare_named_application(repository_root, application, command_runner)?;

    println!(
        "\nFlashing {} as {}",
        prepared.binary.display(),
        prepared.profile.probe_rs_chip
    );
    let status = ProcessCommand::new("probe-rs")
        .arg("download")
        .arg("--chip")
        .arg(prepared.profile.probe_rs_chip)
        .arg("--protocol")
        .arg("swd")
        .arg("--verify")
        .arg("--reset")
        .arg(&prepared.binary)
        .current_dir(repository_root)
        .status()
        .context("start probe-rs; install probe-rs-tools or ensure probe-rs is on PATH")?;
    if !status.success() {
        bail!("probe-rs flash failed with status {status}");
    }
    diagnostics::pass("Flashing and verifying application");
    Ok(())
}

fn embed<R: CommandRunner + ?Sized>(
    repository_root: &Path,
    application: &str,
    command_runner: &R,
) -> Result<()> {
    let prepared = prepare_named_application(repository_root, application, command_runner)?;

    println!(
        "\nStarting cargo embed for {} as {}",
        prepared.binary.display(),
        prepared.profile.probe_rs_chip
    );
    let status = ProcessCommand::new("cargo")
        .arg("embed")
        .arg("--chip")
        .arg(prepared.profile.probe_rs_chip)
        .arg("--path")
        .arg(&prepared.binary)
        .current_dir(repository_root)
        .status()
        .context("start cargo embed; install probe-rs-tools or ensure cargo-embed is on PATH")?;
    if !status.success() {
        bail!("cargo embed failed with status {status}");
    }
    Ok(())
}

fn require_matching_application_name(requested: &str, declared: &str) -> Result<()> {
    if requested != declared {
        bail!("application name `{requested}` resolves to a manifest declaring `{declared}`");
    }
    Ok(())
}

fn generate<R: CommandRunner + ?Sized>(
    repository_root: &Path,
    manifest_path: &Path,
    bsp_path: &Path,
    resume: bool,
    command_runner: &R,
) -> Result<()> {
    println!("Loading application manifest: {}", manifest_path.display());
    println!("Loading BSP manifest: {}", bsp_path.display());
    let manifest_source = fs::read(manifest_path).with_context(|| {
        format!(
            "read application manifest bytes {}",
            manifest_path.display()
        )
    })?;
    let bsp_source = fs::read(bsp_path)
        .with_context(|| format!("read BSP manifest bytes {}", bsp_path.display()))?;
    let manifest_text = std::str::from_utf8(&manifest_source).with_context(|| {
        format!(
            "application manifest is not UTF-8: {}",
            manifest_path.display()
        )
    })?;
    let bsp_text = std::str::from_utf8(&bsp_source)
        .with_context(|| format!("BSP manifest is not UTF-8: {}", bsp_path.display()))?;
    let manifest = manifest::parse(manifest_text, bsp_text).with_context(|| {
        format!(
            "parse application/BSP manifest snapshots {} and {}",
            manifest_path.display(),
            bsp_path.display()
        )
    })?;
    validate_manifest(&manifest)?;
    crate::architecture::validate_for_manifest(repository_root, &manifest)?;
    let mcu_profile = mcu::profile(&manifest.bsp.mcu)?;
    let firmware_target = mcu_profile.rust_target;
    diagnostics::pass("Validating BSP and application manifests");

    let templates = TemplateSet::load(repository_root)?;
    let resolved_features = load_and_resolve_features(repository_root, &manifest)?;
    backend::validate_resolved_claims(&resolved_features)?;
    for feature in &resolved_features {
        diagnostics::pass(&format!("Validating feature {}", feature.id));
    }

    // Resolve all markers and parse both endpoints before any generated path is
    // created. This preserves an older generated application on input failure.
    let empty_render = render::render_crate(&manifest, &templates, &[])?;
    syntax::validate_rendered_rust(&empty_render)?;
    let complete_render = render::render_crate(&manifest, &templates, &resolved_features)?;
    syntax::validate_rendered_rust(&complete_render)?;

    let fingerprints = InputFingerprints::calculate(
        repository_root,
        &manifest_source,
        &bsp_source,
        &templates,
        &resolved_features,
    )?;

    let (generated_root, application_root) =
        prepare_application_root(repository_root, &manifest.application.name)?;
    let _application_lock = ApplicationLock::acquire(&generated_root, &manifest.application.name)?;
    let working = validate_existing_child(&application_root, WORKING_DIR)?
        .unwrap_or_else(|| application_root.join(WORKING_DIR));
    let target_dir = create_or_validate_child(&application_root, "target")?;

    let mut state = if resume {
        resume_checkpoint(
            &application_root,
            &working,
            &target_dir,
            firmware_target,
            &manifest,
            &fingerprints,
            command_runner,
        )?
    } else {
        println!("Checking empty RTIC application");
        let mut initial = fingerprints.new_state();
        let candidate = next_candidate_path(&application_root, "empty")?;
        if let Err(error) = materialize_and_check(
            &candidate,
            &empty_render,
            &target_dir,
            firmware_target,
            command_runner,
        ) {
            diagnostics::fail("Checking empty RTIC application");
            let failed = preserve_failed_candidate(
                &candidate,
                &application_root,
                "empty",
                &manifest,
                &error,
            )?;
            bail!(
                "empty RTIC application validation failed; candidate preserved at {}: {error:#}",
                failed.display()
            );
        }

        initial.checkpoint_hash = checkpoint_hash(&candidate)?;
        write_build_state(&candidate.join(STATE_FILE), &initial)?;
        invalidate_final_artifacts(&application_root, &target_dir, firmware_target)?;
        let outcome = promote_candidate(&candidate, &working)?;
        report_retained_backup(outcome.retained_backup.as_deref());
        write_build_state(&application_root.join(STATE_FILE), &initial)?;
        diagnostics::pass("Checking empty RTIC application");
        initial
    };

    for index in state.next_feature_index..resolved_features.len() {
        let feature = &resolved_features[index];
        diagnostics::stage(index + 1, resolved_features.len(), &feature.id);
        diagnostics::pass("Rendering candidate");

        let rendered = render::render_crate(&manifest, &templates, &resolved_features[..=index])?;
        let candidate = next_candidate_path(&application_root, &feature.id)?;
        if let Err(error) = materialize_and_check(
            &candidate,
            &rendered,
            &target_dir,
            firmware_target,
            command_runner,
        ) {
            diagnostics::fail("Validating candidate");
            let failed = preserve_failed_candidate(
                &candidate,
                &application_root,
                &feature.id,
                &manifest,
                &error,
            )?;
            state.failed_after_inserting = feature.id.clone();
            write_build_state(&application_root.join(STATE_FILE), &state)?;

            println!("\nCandidate validation failed after inserting:");
            println!("{}", feature.id);
            println!("\nLast valid application:\n{}", working.display());
            println!("\nFailed candidate:\n{}", failed.display());
            println!(
                "\nResume unchanged inputs with:\ncargo xtask generate --manifest {:?} --bsp {:?} --resume",
                manifest_path, bsp_path
            );
            bail!(
                "candidate validation failed after inserting feature `{}`; diagnostics are preserved",
                feature.id
            );
        }

        let successful_features = manifest.application.feature_order[..=index].to_vec();
        state.successful_features = successful_features;
        state.next_feature_index = index + 1;
        state.failed_after_inserting.clear();
        state.checkpoint_hash = checkpoint_hash(&candidate)?;
        write_build_state(&candidate.join(STATE_FILE), &state)?;

        let outcome = promote_candidate(&candidate, &working)?;
        report_retained_backup(outcome.retained_backup.as_deref());
        write_build_state(&application_root.join(STATE_FILE), &state)?;
        diagnostics::pass("Promoting candidate");
    }

    println!("\nRunning final release build");
    let manifest_file = working.join("Cargo.toml");
    let output = run_cargo_release_build(
        command_runner,
        &working,
        &manifest_file,
        firmware_target,
        Some(&target_dir),
    )
    .with_context(|| "start final cargo build")?;
    write_command_record(&working, "final-build", &output)?;
    if !output.success() {
        diagnostics::fail("Running final release build");
        bail!(
            "final release build failed with status {:?}; see {}",
            output.status_code(),
            working
                .join("assembler-diagnostics/final-build.stderr")
                .display()
        );
    }
    diagnostics::pass("Running final release build");

    let binary = locate_binary(&target_dir, firmware_target)?;
    let binary_size = fs::metadata(&binary)
        .with_context(|| format!("inspect linked binary {}", binary.display()))?
        .len();
    write_build_metadata(&application_root, &binary, binary_size, &fingerprints)?;

    println!("\nGenerated application:\n{}", working.display());
    println!("\nBinary:\n{}", binary.display());
    println!("Binary size: {binary_size} bytes");
    Ok(())
}

fn load_and_resolve_features(
    repository_root: &Path,
    manifest: &Manifest,
) -> Result<Vec<ResolvedFeature>> {
    let library_root = repository_root.join(FEATURE_LIBRARY);
    let mut resolved = Vec::with_capacity(manifest.application.feature_order.len());
    for feature_name in &manifest.application.feature_order {
        let configuration = manifest.feature(feature_name).ok_or_else(|| {
            anyhow!("feature `{feature_name}` is ordered but has no configuration after validation")
        })?;
        let bundle = load_feature_bundle(&library_root, configuration.implementation())
            .with_context(|| format!("load feature `{feature_name}`"))?;
        resolved.push(
            backend::resolve_feature(manifest, feature_name, &bundle)
                .with_context(|| format!("resolve feature `{feature_name}`"))?,
        );
    }
    Ok(resolved)
}

fn materialize_and_check<R: CommandRunner + ?Sized>(
    candidate: &Path,
    rendered: &RenderedCrate,
    target_dir: &Path,
    firmware_target: &str,
    command_runner: &R,
) -> Result<()> {
    rendered.write_to(candidate)?;
    syntax::validate_rust_tree(candidate).context("parse generated Rust before formatting")?;
    diagnostics::pass("Parsing generated Rust");

    let manifest_file = candidate.join("Cargo.toml");
    let formatted = run_cargo_fmt(command_runner, candidate, &manifest_file, Some(target_dir))
        .context("start cargo fmt")?;
    write_command_record(candidate, "cargo-fmt", &formatted)?;
    require_command_success("cargo fmt", &formatted)?;
    syntax::validate_rust_tree(candidate).context("parse generated Rust after formatting")?;
    diagnostics::pass("Formatting candidate");

    let format_check =
        run_cargo_fmt_check(command_runner, candidate, &manifest_file, Some(target_dir))
            .context("start cargo fmt --check")?;
    write_command_record(candidate, "cargo-fmt-check", &format_check)?;
    require_command_success("cargo fmt --check", &format_check)?;
    diagnostics::pass("Checking candidate formatting");

    let checked = run_cargo_check(
        command_runner,
        candidate,
        &manifest_file,
        firmware_target,
        Some(target_dir),
    )
    .context("start embedded cargo check")?;
    write_command_record(candidate, "cargo-check", &checked)?;
    require_command_success("embedded cargo check", &checked)?;
    diagnostics::pass("Running cargo check");
    Ok(())
}

fn require_command_success(label: &str, output: &CommandOutput) -> Result<()> {
    if !output.success() {
        bail!(
            "{label} failed with status {:?}: {}",
            output.status_code(),
            output.stderr_lossy().trim()
        );
    }
    Ok(())
}

fn resume_checkpoint<R: CommandRunner + ?Sized>(
    application_root: &Path,
    working: &Path,
    target_dir: &Path,
    firmware_target: &str,
    manifest: &Manifest,
    fingerprints: &InputFingerprints,
    command_runner: &R,
) -> Result<BuildState> {
    let state_path = application_root.join(STATE_FILE);
    let working_state = read_build_state(&working.join(STATE_FILE))
        .context("working checkpoint has no valid embedded state")?;
    fingerprints.validate_state(&working_state)?;
    validate_checkpoint_prefix(&working_state, manifest)?;

    let actual_hash = checkpoint_hash(working)?;
    if actual_hash != working_state.checkpoint_hash {
        bail!(
            "resume refused: working checkpoint fingerprint changed; run generate without --resume"
        );
    }

    let state = match read_build_state(&state_path) {
        Ok(state) if same_checkpoint_state(&state, &working_state) => state,
        Ok(_) | Err(_) => {
            eprintln!(
                "warning: recovering sibling build state from validated working checkpoint {}",
                working.display()
            );
            write_build_state(&state_path, &working_state)?;
            working_state.clone()
        }
    };

    syntax::validate_rust_tree(working).context("resume syntax validation failed")?;
    let manifest_file = working.join("Cargo.toml");
    let format_check =
        run_cargo_fmt_check(command_runner, working, &manifest_file, Some(target_dir))
            .context("start resume cargo fmt --check")?;
    write_command_record(working, "resume-fmt-check", &format_check)?;
    require_command_success("resume cargo fmt --check", &format_check)?;

    let check = run_cargo_check(
        command_runner,
        working,
        &manifest_file,
        firmware_target,
        Some(target_dir),
    )
    .context("start resume cargo check")?;
    write_command_record(working, "resume-cargo-check", &check)?;
    require_command_success("resume cargo check", &check)?;
    diagnostics::pass("Validating resume checkpoint");
    Ok(state)
}

fn validate_checkpoint_prefix(state: &BuildState, manifest: &Manifest) -> Result<()> {
    if state.next_feature_index > manifest.application.feature_order.len() {
        bail!("resume refused: next_feature_index exceeds the selected feature count");
    }
    let expected = &manifest.application.feature_order[..state.next_feature_index];
    if state.successful_features != expected {
        bail!(
            "resume refused: successful feature list is not the declared prefix; run generate without --resume"
        );
    }
    Ok(())
}

fn same_checkpoint_state(left: &BuildState, right: &BuildState) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.failed_after_inserting.clear();
    right.failed_after_inserting.clear();
    left == right
}

fn preserve_failed_candidate(
    candidate: &Path,
    application_root: &Path,
    feature: &str,
    manifest: &Manifest,
    error: &anyhow::Error,
) -> Result<PathBuf> {
    let failed_root = create_or_validate_child(application_root, "failed")?;
    let destination = next_failed_candidate_path(&failed_root, feature)?;

    if candidate.is_dir() {
        fs::rename(candidate, &destination).with_context(|| {
            format!(
                "preserve failed candidate {} as {}",
                candidate.display(),
                destination.display()
            )
        })?;
    } else {
        fs::create_dir_all(&destination).with_context(|| {
            format!(
                "create diagnostics-only failed candidate {}",
                destination.display()
            )
        })?;
    }

    let mut resolved_manifest = toml::to_string_pretty(manifest)
        .context("serialize resolved manifest for failed candidate")?;
    resolved_manifest.push('\n');
    fs::write(
        destination.join("resolved-manifest.toml"),
        resolved_manifest,
    )
    .context("write failed candidate resolved manifest")?;
    fs::write(destination.join("failure.txt"), format!("{error:#}\n"))
        .context("write failed candidate failure summary")?;
    for extension in ["command", "stdout", "stderr"] {
        let source = destination
            .join("assembler-diagnostics")
            .join(format!("cargo-check.{extension}"));
        if source.is_file() {
            fs::copy(
                &source,
                destination.join(format!("cargo-check.{extension}")),
            )
            .with_context(|| format!("copy failed cargo-check {extension} diagnostic"))?;
        }
    }
    Ok(destination)
}

fn write_command_record(root: &Path, name: &str, output: &CommandOutput) -> Result<()> {
    let canonical_root = fs::canonicalize(root)
        .with_context(|| format!("resolve diagnostics owner {}", root.display()))?;
    let diagnostics = create_or_validate_child(&canonical_root, "assembler-diagnostics")?;

    let mut command = format!("cwd = {:?}\nargv =", output.command.cwd);
    for argument in output.command.argv() {
        command.push(' ');
        command.push_str(&format!("{argument:?}"));
    }
    command.push('\n');
    command.push_str(&format!("status = {:?}\n", output.status_code()));

    fs::write(diagnostics.join(format!("{name}.command")), command)
        .with_context(|| format!("write {name} command record"))?;
    fs::write(diagnostics.join(format!("{name}.stdout")), &output.stdout)
        .with_context(|| format!("write {name} stdout"))?;
    fs::write(diagnostics.join(format!("{name}.stderr")), &output.stderr)
        .with_context(|| format!("write {name} stderr"))?;
    Ok(())
}

fn checkpoint_hash(root: &Path) -> Result<String> {
    let expected = BTreeSet::from([
        "Cargo.toml".to_owned(),
        "Cargo.lock".to_owned(),
        "build.rs".to_owned(),
        "memory.x".to_owned(),
        "src/main.rs".to_owned(),
    ]);
    let mut discovered = Vec::new();
    collect_checkpoint_files(root, root, &mut discovered)?;
    let actual = discovered
        .iter()
        .map(|(label, _)| label.clone())
        .collect::<BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let unexpected = actual.difference(&expected).cloned().collect::<Vec<_>>();
        bail!(
            "working checkpoint file inventory changed (missing: {missing:?}; unexpected: {unexpected:?})"
        );
    }
    hash_labeled_files(discovered)
}

fn collect_checkpoint_files(
    checkpoint_root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("read checkpoint directory {}", directory.display()))?
    {
        let entry = entry.with_context(|| format!("read entry below {}", directory.display()))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(checkpoint_root)
            .with_context(|| format!("checkpoint entry escaped root: {}", path.display()))?;
        let label = relative.to_string_lossy().replace('\\', "/");
        let file_type = entry
            .file_type()
            .with_context(|| format!("inspect checkpoint entry {}", path.display()))?;
        if file_type.is_symlink() {
            bail!("working checkpoint contains a symlink: {}", path.display());
        }
        if label == "assembler-diagnostics" {
            if !file_type.is_dir() {
                bail!(
                    "checkpoint diagnostics path is not a directory: {}",
                    path.display()
                );
            }
            continue;
        }
        if file_type.is_dir() {
            collect_checkpoint_files(checkpoint_root, &path, output)?;
        } else if file_type.is_file() && label != STATE_FILE {
            output.push((label, path));
        }
    }
    Ok(())
}

fn next_candidate_path(application_root: &Path, label: &str) -> Result<PathBuf> {
    if !is_safe_component(label) {
        bail!("candidate label is not a safe path component: {label:?}");
    }
    let base = format!(".candidate-{label}");
    for suffix in 1_u64.. {
        let name = if suffix == 1 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let candidate = application_root.join(name);
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect candidate path {}", candidate.display()));
            }
        }
    }
    unreachable!("u64 candidate suffix space was exhausted")
}

fn report_retained_backup(backup: Option<&Path>) {
    if let Some(backup) = backup {
        eprintln!(
            "warning: promoted candidate but retained locked backup at {}",
            backup.display()
        );
    }
}

fn locate_binary(target_dir: &Path, firmware_target: &str) -> Result<PathBuf> {
    let base = target_dir
        .join(firmware_target)
        .join("release")
        .join(GENERATED_PACKAGE);
    if base.is_file() {
        return Ok(base);
    }
    let with_extension = base.with_extension(env::consts::EXE_EXTENSION);
    if with_extension.is_file() {
        return Ok(with_extension);
    }
    bail!(
        "cargo reported success but linked binary was not found at {}",
        base.display()
    )
}

fn invalidate_final_artifacts(
    application_root: &Path,
    target_dir: &Path,
    firmware_target: &str,
) -> Result<()> {
    let candidates = [
        application_root.join("build-metadata.toml"),
        target_dir
            .join(firmware_target)
            .join("release")
            .join(GENERATED_PACKAGE),
        target_dir
            .join(firmware_target)
            .join("release")
            .join(format!("{GENERATED_PACKAGE}.exe")),
        target_dir
            .join(firmware_target)
            .join("release")
            .join(format!("{GENERATED_PACKAGE}.d")),
    ];
    for path in candidates {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("invalidate stale artifact {}", path.display()));
            }
        }
    }
    Ok(())
}

fn write_build_metadata(
    application_root: &Path,
    binary: &Path,
    binary_size: u64,
    fingerprints: &InputFingerprints,
) -> Result<()> {
    let text = format!(
        "schema_version = 1\nbuilder_version = {GENERATOR_VERSION:?}\nbackend_version = {BACKEND_VERSION:?}\ntoolchain = {:?}\nmanifest_fingerprint = {:?}\nfeature_fingerprint = {:?}\nbinary = {:?}\nbinary_size_bytes = {binary_size}\n",
        fingerprints.toolchain,
        fingerprints.manifest_hash,
        fingerprints.ordered_feature_hash,
        binary.display().to_string(),
    );
    write_atomic_file(
        &application_root.join("build-metadata.toml"),
        text.as_bytes(),
        "build metadata",
    )
}

fn clean(repository_root: &Path, application: &str) -> Result<()> {
    if !is_safe_application_slug(application) {
        bail!("refusing unsafe application name for clean: {application:?}");
    }
    let canonical_repository = fs::canonicalize(repository_root)
        .with_context(|| format!("resolve repository root {}", repository_root.display()))?;
    let Some(generated) = validate_existing_child(&canonical_repository, GENERATED_DIR)? else {
        println!("No generated application exists for {application}");
        return Ok(());
    };
    let _application_lock = ApplicationLock::acquire(&generated, application)?;
    let Some(target) = validate_existing_child(&generated, application)? else {
        println!(
            "No generated application exists at {}",
            generated.join(application).display()
        );
        return Ok(());
    };
    fs::remove_dir_all(&target)
        .with_context(|| format!("remove generated application {}", target.display()))?;
    println!(
        "Removed generated application {} (not recoverable from this directory)",
        target.display()
    );
    Ok(())
}

fn prepare_application_root(
    repository_root: &Path,
    application: &str,
) -> Result<(PathBuf, PathBuf)> {
    if !is_safe_application_slug(application) {
        bail!("refusing unsafe generated application name: {application:?}");
    }
    let canonical_repository = fs::canonicalize(repository_root)
        .with_context(|| format!("resolve repository root {}", repository_root.display()))?;
    let generated = create_or_validate_child(&canonical_repository, GENERATED_DIR)?;
    let application_root = create_or_validate_child(&generated, application)?;
    Ok((generated, application_root))
}

fn create_or_validate_child(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path)
                .with_context(|| format!("create confined directory {}", path.display()))?;
        }
        Err(error) => {
            return Err(error).with_context(|| format!("inspect directory {}", path.display()));
        }
    }
    validate_existing_child(parent, name)?.ok_or_else(|| {
        anyhow!(
            "directory disappeared while validating confinement: {}",
            path.display()
        )
    })
}

fn validate_existing_child(parent: &Path, name: &str) -> Result<Option<PathBuf>> {
    if !is_safe_component(name) {
        bail!("unsafe child path component: {name:?}");
    }
    let path = parent.join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("inspect directory {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "confined generated path must be a real directory, not a link or file: {}",
            path.display()
        );
    }
    let canonical = fs::canonicalize(&path)
        .with_context(|| format!("resolve generated directory {}", path.display()))?;
    let expected = parent.join(name);
    if canonical != expected {
        bail!(
            "generated directory resolves through a symlink or junction: {} -> {}",
            path.display(),
            canonical.display()
        );
    }
    Ok(Some(canonical))
}

struct ApplicationLock {
    directory: PathBuf,
}

impl ApplicationLock {
    fn acquire(generated_root: &Path, application: &str) -> Result<Self> {
        let directory = generated_root.join(format!(".assembler-lock-{application}"));
        fs::create_dir(&directory).with_context(|| {
            format!(
                "another assembler may be using `{application}` (or a stale lock remains at {}); refusing concurrent mutation",
                directory.display()
            )
        })?;
        if let Err(error) = fs::write(
            directory.join("owner"),
            format!("process_id = {}\n", std::process::id()),
        ) {
            let _ = fs::remove_dir(&directory);
            return Err(error)
                .with_context(|| format!("write lock owner in {}", directory.display()));
        }
        Ok(Self { directory })
    }
}

impl Drop for ApplicationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.directory.join("owner"));
        let _ = fs::remove_dir(&self.directory);
    }
}

fn is_safe_application_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !value.contains("--")
}

fn is_safe_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !value.is_empty()
}

fn active_toolchain_identity(repository_root: &Path) -> Result<String> {
    let active = capture_version_command(repository_root, "rustup", &["show", "active-toolchain"])?;
    let active_name = active
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("rustup returned an empty active-toolchain description"))?;
    if active_name != TOOLCHAIN && !active_name.starts_with(&format!("{TOOLCHAIN}-")) {
        bail!(
            "active Rust toolchain is `{active_name}`, but this repository requires `{TOOLCHAIN}`"
        );
    }
    let rustc = capture_version_command(repository_root, "rustc", &["-Vv"])?;
    let cargo = capture_version_command(repository_root, "cargo", &["-Vv"])?;
    Ok(format!(
        "active: {}\nrustc:\n{}\ncargo:\n{}",
        active_name,
        rustc.trim(),
        cargo.trim()
    ))
}

fn capture_version_command(root: &Path, program: &str, arguments: &[&str]) -> Result<String> {
    let output = ProcessCommand::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .with_context(|| format!("start `{program} {}`", arguments.join(" ")))?;
    if !output.status.success() {
        bail!(
            "`{program} {}` failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .with_context(|| format!("`{program} {}` output was not UTF-8", arguments.join(" ")))
}

struct InputFingerprints {
    toolchain: String,
    cargo_lock_hash: String,
    template_hash: String,
    manifest_hash: String,
    ordered_feature_hash: String,
    input_hash: String,
}

impl InputFingerprints {
    fn calculate(
        repository_root: &Path,
        manifest_source: &[u8],
        bsp_source: &[u8],
        templates: &TemplateSet,
        features: &[ResolvedFeature],
    ) -> Result<Self> {
        let root_lock = fs::read(repository_root.join("Cargo.lock"))
            .context("read root Cargo.lock for fingerprint")?;
        let toolchain = fs::read(repository_root.join("rust-toolchain.toml"))
            .context("read rust-toolchain.toml for fingerprint")?;
        let toolchain_identity = active_toolchain_identity(repository_root)?;
        let builder_inputs = read_builder_inputs(repository_root)?;

        let template_inputs = templates
            .fingerprint_inputs()
            .into_iter()
            .map(|(label, bytes)| (label.to_owned(), bytes.to_vec()))
            .collect::<Vec<_>>();
        let template_hash = hash_labeled_bytes(
            template_inputs
                .iter()
                .map(|(label, bytes)| (label.as_str(), bytes.as_slice())),
        );
        let ordered_feature_inputs = features
            .iter()
            .enumerate()
            .map(|(index, feature)| {
                (
                    format!("{index:04}/{}", feature.id),
                    feature.fingerprint.as_bytes().to_vec(),
                )
            })
            .collect::<Vec<_>>();
        let ordered_feature_hash = hash_labeled_bytes(
            ordered_feature_inputs
                .iter()
                .map(|(label, bytes)| (label.as_str(), bytes.as_slice())),
        );
        let cargo_lock_hash = hash_labeled_bytes([
            ("root/Cargo.lock", root_lock.as_slice()),
            ("firmware/Cargo.lock", templates.cargo_lock.as_bytes()),
        ]);
        let manifest_hash = hash_labeled_bytes([
            ("application-manifest", manifest_source),
            ("bsp-manifest", bsp_source),
        ]);

        let mut complete = vec![
            ("application-manifest".into(), manifest_source.to_vec()),
            ("bsp-manifest".into(), bsp_source.to_vec()),
            ("root/Cargo.lock".into(), root_lock),
            ("rust-toolchain.toml".into(), toolchain),
            (
                "active-toolchain".into(),
                toolchain_identity.as_bytes().to_vec(),
            ),
            (
                "generator-version".into(),
                GENERATOR_VERSION.as_bytes().to_vec(),
            ),
            (
                "backend-version".into(),
                BACKEND_VERSION.as_bytes().to_vec(),
            ),
        ];
        complete.extend(builder_inputs);
        complete.extend(template_inputs);
        complete.extend(
            ordered_feature_inputs
                .into_iter()
                .map(|(label, value)| (format!("resolved-feature/{label}"), value)),
        );
        let input_hash = hash_labeled_bytes(complete);

        Ok(Self {
            toolchain: toolchain_identity,
            cargo_lock_hash,
            template_hash,
            manifest_hash,
            ordered_feature_hash,
            input_hash,
        })
    }

    fn new_state(&self) -> BuildState {
        BuildState {
            schema_version: BUILD_STATE_SCHEMA_VERSION,
            generator_version: GENERATOR_VERSION.to_owned(),
            backend_version: BACKEND_VERSION.to_owned(),
            toolchain: self.toolchain.clone(),
            cargo_lock_hash: self.cargo_lock_hash.clone(),
            template_hash: self.template_hash.clone(),
            manifest_hash: self.manifest_hash.clone(),
            ordered_feature_hash: self.ordered_feature_hash.clone(),
            input_hash: self.input_hash.clone(),
            checkpoint_hash: String::new(),
            successful_features: Vec::new(),
            next_feature_index: 0,
            failed_after_inserting: String::new(),
        }
    }

    fn validate_state(&self, state: &BuildState) -> Result<()> {
        let expected = self.new_state();
        let comparisons = [
            (
                "generator version",
                state.generator_version.as_str(),
                expected.generator_version.as_str(),
            ),
            (
                "backend version",
                state.backend_version.as_str(),
                expected.backend_version.as_str(),
            ),
            (
                "toolchain",
                state.toolchain.as_str(),
                expected.toolchain.as_str(),
            ),
            (
                "Cargo.lock",
                state.cargo_lock_hash.as_str(),
                expected.cargo_lock_hash.as_str(),
            ),
            (
                "templates",
                state.template_hash.as_str(),
                expected.template_hash.as_str(),
            ),
            (
                "manifest",
                state.manifest_hash.as_str(),
                expected.manifest_hash.as_str(),
            ),
            (
                "ordered features",
                state.ordered_feature_hash.as_str(),
                expected.ordered_feature_hash.as_str(),
            ),
            (
                "complete input",
                state.input_hash.as_str(),
                expected.input_hash.as_str(),
            ),
        ];
        if state.schema_version != BUILD_STATE_SCHEMA_VERSION {
            bail!("resume refused: build-state schema changed; run generate without --resume");
        }
        for (label, actual, expected) in comparisons {
            if actual != expected {
                bail!("resume refused: {label} fingerprint changed; run generate without --resume");
            }
        }
        Ok(())
    }
}

fn read_builder_inputs(repository_root: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut paths = vec![
        repository_root.join("Cargo.toml"),
        repository_root.join(".cargo/config.toml"),
        repository_root.join("xtask/Cargo.toml"),
    ];
    collect_regular_files(&repository_root.join("xtask/src"), &mut paths)?;
    collect_regular_files(&repository_root.join("architecture-contracts"), &mut paths)?;

    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let relative = path
            .strip_prefix(repository_root)
            .with_context(|| format!("builder input escaped repository: {}", path.display()))?;
        let label = format!("builder/{}", relative.to_string_lossy().replace('\\', "/"));
        let bytes = fs::read(&path)
            .with_context(|| format!("read builder fingerprint input {}", path.display()))?;
        inputs.push((label, bytes));
    }
    Ok(inputs)
}

fn collect_regular_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)
        .with_context(|| format!("read builder source directory {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("read entry below {}", root.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("read builder input type {}", entry.path().display()))?;
        if file_type.is_symlink() {
            bail!(
                "builder fingerprint input must not be a symlink: {}",
                entry.path().display()
            );
        }
        if file_type.is_dir() {
            collect_regular_files(&entry.path(), output)?;
        } else if file_type.is_file() {
            output.push(entry.path());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, ffi::OsStr, io, process::ExitStatus};

    use crate::runner::CommandSpec;

    const FAKE_COMPILER_ERROR: &[u8] = b"synthetic compiler error: blink candidate\n";
    type TreeSnapshot = Vec<(PathBuf, Vec<u8>)>;

    #[test]
    fn bare_input_names_resolve_through_repository_catalogs() {
        let root = repository_root().unwrap();
        let application =
            resolve_input_path(&root, Path::new("nucleo-f401re-blinky"), "applications").unwrap();
        let bsp = resolve_input_path(&root, Path::new("nucleo-f401re"), "bsp").unwrap();

        assert_eq!(
            application,
            fs::canonicalize(root.join("applications/nucleo-f401re-blinky.toml")).unwrap()
        );
        assert_eq!(
            bsp,
            fs::canonicalize(root.join("bsp/nucleo-f401re.toml")).unwrap()
        );
    }

    #[test]
    fn flash_name_must_match_the_application_manifest() {
        assert!(require_matching_application_name("blink", "blink").is_ok());
        let error = require_matching_application_name("blink", "other")
            .unwrap_err()
            .to_string();
        assert!(error.contains("declaring `other`"));
    }

    struct FakeCommandRunner {
        commands: RefCell<Vec<CommandSpec>>,
        working_before_failure: RefCell<Option<TreeSnapshot>>,
        fail_blink_check: bool,
        create_release_binary: bool,
    }

    impl FakeCommandRunner {
        fn failing_blink_check() -> Self {
            Self {
                commands: RefCell::new(Vec::new()),
                working_before_failure: RefCell::new(None),
                fail_blink_check: true,
                create_release_binary: false,
            }
        }

        fn successful_build() -> Self {
            Self {
                commands: RefCell::new(Vec::new()),
                working_before_failure: RefCell::new(None),
                fail_blink_check: false,
                create_release_binary: true,
            }
        }

        fn commands(&self) -> Vec<CommandSpec> {
            self.commands.borrow().clone()
        }

        fn working_before_failure(&self) -> Vec<(PathBuf, Vec<u8>)> {
            self.working_before_failure
                .borrow()
                .clone()
                .expect("failure runner did not snapshot working")
        }
    }

    impl CommandRunner for FakeCommandRunner {
        fn run(&self, command: CommandSpec) -> io::Result<CommandOutput> {
            self.commands.borrow_mut().push(command.clone());

            let subcommand = command.args.first().and_then(|value| value.to_str());
            let is_blink_candidate = command
                .cwd
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with(".candidate-blink_led"));
            let should_fail =
                self.fail_blink_check && subcommand == Some("check") && is_blink_candidate;

            if should_fail {
                let application_root = command.cwd.parent().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "blink candidate has no application root",
                    )
                })?;
                *self.working_before_failure.borrow_mut() =
                    Some(snapshot_tree(&application_root.join(WORKING_DIR))?);
            }

            if self.create_release_binary && subcommand == Some("build") {
                let target_dir = command
                    .env_overrides
                    .iter()
                    .find(|(name, _)| name == OsStr::new("CARGO_TARGET_DIR"))
                    .map(|(_, value)| PathBuf::from(value.as_os_str()))
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "fake release build has no CARGO_TARGET_DIR",
                        )
                    })?;
                let release_dir = target_dir.join(FIRMWARE_TARGET).join("release");
                fs::create_dir_all(&release_dir)?;
                fs::write(release_dir.join(GENERATED_PACKAGE), b"fake firmware")?;
            }

            Ok(CommandOutput {
                command,
                status: exit_status(if should_fail { 1 } else { 0 }),
                stdout: Vec::new(),
                stderr: if should_fail {
                    FAKE_COMPILER_ERROR.to_vec()
                } else {
                    Vec::new()
                },
            })
        }
    }

    #[cfg(unix)]
    fn exit_status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;

        ExitStatus::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn exit_status(code: i32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;

        ExitStatus::from_raw(code as u32)
    }

    struct StagedRepository {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        manifest: PathBuf,
        bsp: PathBuf,
    }

    fn stage_repository() -> StagedRepository {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask must have a repository parent");
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("repository");

        for relative in [
            "Cargo.toml",
            "Cargo.lock",
            ".cargo/config.toml",
            "rust-toolchain.toml",
            "xtask/Cargo.toml",
            "xtask/src",
            "applications/nucleo-f401re-blinky.toml",
            "architecture-contracts/nucleo-f401re-blinky.toml",
            "bsp/nucleo-f401re.toml",
            "templates/stm32f4-rtic",
            "feature-library/stm32f4/blink-led",
            "feature-library/stm32f4/button-toggle-blink",
        ] {
            copy_fixture_tree(&source_root.join(relative), &root.join(relative));
        }

        let manifest = root.join("applications/nucleo-f401re-blinky.toml");
        let bsp = root.join("bsp/nucleo-f401re.toml");
        StagedRepository {
            _temporary: temporary,
            root,
            manifest,
            bsp,
        }
    }

    fn copy_fixture_tree(source: &Path, destination: &Path) {
        if source.is_dir() {
            fs::create_dir_all(destination).unwrap();
            let mut entries = fs::read_dir(source)
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                copy_fixture_tree(&entry.path(), &destination.join(entry.file_name()));
            }
        } else {
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source, destination).unwrap();
        }
    }

    fn snapshot_tree(root: &Path) -> io::Result<TreeSnapshot> {
        fn visit(
            root: &Path,
            directory: &Path,
            snapshot: &mut Vec<(PathBuf, Vec<u8>)>,
        ) -> io::Result<()> {
            let mut entries = fs::read_dir(directory)?.collect::<io::Result<Vec<_>>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                if entry.file_type()?.is_dir() {
                    visit(root, &path, snapshot)?;
                } else {
                    let relative = path
                        .strip_prefix(root)
                        .expect("snapshot entry must be below its root")
                        .to_path_buf();
                    snapshot.push((relative, fs::read(path)?));
                }
            }
            Ok(())
        }

        let mut snapshot = Vec::new();
        visit(root, root, &mut snapshot)?;
        Ok(snapshot)
    }

    #[test]
    fn checkpoint_inventory_rejects_unexpected_cargo_configuration() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir(root.join("src")).unwrap();
        for relative in [
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "memory.x",
            "src/main.rs",
        ] {
            fs::write(root.join(relative), relative.as_bytes()).unwrap();
        }
        assert!(checkpoint_hash(root).is_ok());

        fs::create_dir(root.join(".cargo")).unwrap();
        fs::write(
            root.join(".cargo/config.toml"),
            b"[build]\nrustflags = []\n",
        )
        .unwrap();
        assert!(checkpoint_hash(root).is_err());
    }

    #[test]
    fn application_lock_serializes_mutations() {
        let temporary = tempfile::tempdir().unwrap();
        let first = ApplicationLock::acquire(temporary.path(), "blink-app").unwrap();
        assert!(ApplicationLock::acquire(temporary.path(), "blink-app").is_err());
        drop(first);
        assert!(ApplicationLock::acquire(temporary.path(), "blink-app").is_ok());
    }

    #[test]
    fn clean_slug_and_candidate_components_are_confined() {
        assert!(is_safe_application_slug("nucleo-f401re-blinky"));
        assert!(!is_safe_application_slug("../escape"));
        assert!(!is_safe_application_slug("bad--slug"));
        assert!(is_safe_component("blink_led"));
        assert!(!is_safe_component("../blink_led"));
    }

    #[test]
    fn checkpoint_prefix_must_match_manifest_order() {
        let manifest = manifest::parse(
            include_str!("../../applications/nucleo-f401re-blinky.toml"),
            include_str!("../../bsp/nucleo-f401re.toml"),
        )
        .unwrap();
        let mut state = InputFingerprints {
            toolchain: String::new(),
            cargo_lock_hash: String::new(),
            template_hash: String::new(),
            manifest_hash: String::new(),
            ordered_feature_hash: String::new(),
            input_hash: String::new(),
        }
        .new_state();
        assert!(validate_checkpoint_prefix(&state, &manifest).is_ok());
        state.successful_features = vec!["wrong".into()];
        state.next_feature_index = 1;
        assert!(validate_checkpoint_prefix(&state, &manifest).is_err());
    }

    #[test]
    fn failed_feature_is_preserved_and_only_unchanged_inputs_resume() {
        let repository = stage_repository();
        let application_root = repository
            .root
            .join(GENERATED_DIR)
            .join("nucleo-f401re-blinky");
        let working = application_root.join(WORKING_DIR);

        let failing_runner = FakeCommandRunner::failing_blink_check();
        let failure = generate(
            &repository.root,
            &repository.manifest,
            &repository.bsp,
            false,
            &failing_runner,
        )
        .unwrap_err();
        let failure_message = format!("{failure:#}");
        assert!(
            failure_message
                .contains("candidate validation failed after inserting feature `blink_led`")
        );

        let commands = failing_runner.commands();
        assert_eq!(
            commands.len(),
            6,
            "empty and blink candidates each use three commands"
        );
        assert_eq!(
            commands[2].args.first().and_then(|arg| arg.to_str()),
            Some("check")
        );
        assert_eq!(
            commands[5].args.first().and_then(|arg| arg.to_str()),
            Some("check")
        );
        assert!(
            commands[2]
                .cwd
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with(".candidate-empty"))
        );
        assert!(
            commands[5]
                .cwd
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with(".candidate-blink_led"))
        );
        assert_eq!(
            snapshot_tree(&working).unwrap(),
            failing_runner.working_before_failure(),
            "feature failure must not modify any byte in the validated working checkpoint"
        );

        let sibling_state = read_build_state(&application_root.join(STATE_FILE)).unwrap();
        let working_state = read_build_state(&working.join(STATE_FILE)).unwrap();
        let mut attributed_state = working_state.clone();
        attributed_state.failed_after_inserting = "blink_led".to_owned();
        assert_eq!(sibling_state, attributed_state);
        assert!(working_state.successful_features.is_empty());
        assert_eq!(working_state.next_feature_index, 0);
        assert!(working_state.failed_after_inserting.is_empty());
        assert_eq!(sibling_state.failed_after_inserting, "blink_led");
        assert_eq!(
            checkpoint_hash(&working).unwrap(),
            working_state.checkpoint_hash
        );

        let working_main = fs::read_to_string(working.join("src/main.rs")).unwrap();
        assert!(working_main.contains("let _ = cx;"));
        assert!(!working_main.contains("fn blink_led"));

        let failed = application_root.join("failed/blink_led");
        let failed_main = fs::read_to_string(failed.join("src/main.rs")).unwrap();
        assert!(failed_main.contains("fn blink_led"));
        assert_eq!(
            fs::read(failed.join("assembler-diagnostics/cargo-check.stderr")).unwrap(),
            FAKE_COMPILER_ERROR
        );
        assert!(failed.join("resolved-manifest.toml").is_file());
        assert!(
            fs::read_to_string(failed.join("failure.txt"))
                .unwrap()
                .contains("synthetic compiler error: blink candidate")
        );

        let resume_runner = FakeCommandRunner::successful_build();
        generate(
            &repository.root,
            &repository.manifest,
            &repository.bsp,
            true,
            &resume_runner,
        )
        .unwrap();

        let resume_commands = resume_runner.commands();
        assert_eq!(resume_commands.len(), 9);
        assert_eq!(resume_commands[0].cwd, fs::canonicalize(&working).unwrap());
        assert_eq!(
            resume_commands[0].args.first().and_then(|arg| arg.to_str()),
            Some("fmt")
        );
        assert_eq!(
            resume_commands[1].args.first().and_then(|arg| arg.to_str()),
            Some("check")
        );
        assert_eq!(
            resume_commands
                .last()
                .unwrap()
                .args
                .first()
                .and_then(|arg| arg.to_str()),
            Some("build")
        );

        let completed_state = read_build_state(&application_root.join(STATE_FILE)).unwrap();
        assert_eq!(
            completed_state.successful_features,
            vec!["blink_led".to_owned(), "button_toggle".to_owned()]
        );
        assert_eq!(completed_state.next_feature_index, 2);
        assert!(completed_state.failed_after_inserting.is_empty());
        assert_eq!(
            completed_state,
            read_build_state(&working.join(STATE_FILE)).unwrap()
        );
        assert_eq!(
            checkpoint_hash(&working).unwrap(),
            completed_state.checkpoint_hash
        );
        assert!(
            fs::read_to_string(working.join("src/main.rs"))
                .unwrap()
                .contains("fn blink_led")
        );

        let binary = application_root
            .join("target")
            .join(FIRMWARE_TARGET)
            .join("release")
            .join(GENERATED_PACKAGE);
        assert_eq!(fs::read(&binary).unwrap(), b"fake firmware");
        assert!(application_root.join("build-metadata.toml").is_file());

        let original_bsp = fs::read_to_string(&repository.bsp).unwrap();
        let mut changed_bsp = original_bsp.clone();
        changed_bsp.push_str("\n# fingerprint-changing BSP comment\n");
        fs::write(&repository.bsp, changed_bsp).unwrap();

        let changed_bsp_runner = FakeCommandRunner::successful_build();
        let resume_error = generate(
            &repository.root,
            &repository.manifest,
            &repository.bsp,
            true,
            &changed_bsp_runner,
        )
        .unwrap_err();
        assert!(
            format!("{resume_error:#}").contains("resume refused: manifest fingerprint changed")
        );
        assert!(
            changed_bsp_runner.commands().is_empty(),
            "BSP drift must be rejected before any external command"
        );
        fs::write(&repository.bsp, original_bsp).unwrap();

        let mut changed_manifest = fs::read_to_string(&repository.manifest).unwrap();
        changed_manifest.push_str("\n# fingerprint-changing test comment\n");
        fs::write(&repository.manifest, changed_manifest).unwrap();

        let changed_input_runner = FakeCommandRunner::successful_build();
        let resume_error = generate(
            &repository.root,
            &repository.manifest,
            &repository.bsp,
            true,
            &changed_input_runner,
        )
        .unwrap_err();
        assert!(
            format!("{resume_error:#}").contains("resume refused: manifest fingerprint changed")
        );
        assert!(
            changed_input_runner.commands().is_empty(),
            "input drift must be rejected before any external command"
        );
    }
}
