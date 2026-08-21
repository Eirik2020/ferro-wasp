//! Catalog-confined command execution for generated applications.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    cli::{Cli, Command},
    generator,
    input_catalog::{self, ApplicationEntry},
    runner::{
        CommandOutput, CommandRunner, ProcessRunner, cargo_release_build_command, embed_command,
        flash_command,
    },
};

/// Executes a parsed CLI command using real child processes.
pub fn execute(cli: Cli, builder_root: &Path) -> Result<()> {
    execute_with_runner(cli, builder_root, &ProcessRunner)
}

/// Executes a parsed CLI command through an injectable process boundary.
pub fn execute_with_runner<R: CommandRunner + ?Sized>(
    cli: Cli,
    builder_root: &Path,
    runner: &R,
) -> Result<()> {
    match cli.command {
        Command::Check { app } => {
            generator::render_selected(builder_root, &app)?;
            println!("application `{app}` resolves and renders successfully");
            Ok(())
        }
        Command::Generate { app } => {
            let destination = generator::generate(builder_root, &app)?;
            println!("generated application `{app}` at {}", destination.display());
            Ok(())
        }
        Command::Build { app } => {
            let prepared = build(builder_root, &app, runner)?;
            println!("built application `{app}` at {}", prepared.binary.display());
            Ok(())
        }
        Command::Clean { app } => clean(builder_root, &app),
        Command::Flash { app } => {
            let prepared = build(builder_root, &app, runner)?;
            ensure_binary_exists(&prepared.binary)?;
            let output = runner
                .run(flash_command(
                    builder_root,
                    prepared.entry.probe_chip,
                    &prepared.binary,
                ))
                .context("start probe-rs; install probe-rs-tools or ensure probe-rs is on PATH")?;
            require_success("probe-rs flash", &output)
        }
        Command::Embed { app } => {
            let prepared = build(builder_root, &app, runner)?;
            ensure_binary_exists(&prepared.binary)?;
            let output = runner
                .run(embed_command(
                    builder_root,
                    prepared.entry.probe_chip,
                    &prepared.binary,
                ))
                .context(
                    "start cargo embed; install probe-rs-tools or ensure cargo-embed is on PATH",
                )?;
            require_success("cargo embed", &output)
        }
    }
}

struct PreparedApplication {
    entry: &'static ApplicationEntry,
    binary: PathBuf,
}

fn build<R: CommandRunner + ?Sized>(
    builder_root: &Path,
    application_id: &str,
    runner: &R,
) -> Result<PreparedApplication> {
    let entry = input_catalog::application(application_id)?;
    let working = generator::generate(builder_root, application_id)?;
    let target_dir = builder_root
        .join(generator::GENERATED_ROOT)
        .join(application_id)
        .join("target");
    let output = runner
        .run(cargo_release_build_command(
            &working,
            entry.rust_target,
            &target_dir,
        ))
        .with_context(|| format!("start locked release build for `{application_id}`"))?;
    require_success("locked release build", &output)?;

    Ok(PreparedApplication {
        entry,
        binary: target_dir
            .join(entry.rust_target)
            .join("release")
            .join(entry.binary_name),
    })
}

fn ensure_binary_exists(binary: &Path) -> Result<()> {
    if !binary.is_file() {
        bail!(
            "release build succeeded but the expected ELF is missing at {}",
            binary.display()
        );
    }
    Ok(())
}

fn require_success(label: &str, output: &CommandOutput) -> Result<()> {
    if !output.success() {
        bail!(
            "{label} failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            output.stdout_lossy(),
            output.stderr_lossy()
        );
    }
    Ok(())
}

fn clean(builder_root: &Path, application_id: &str) -> Result<()> {
    input_catalog::application(application_id)?;
    let working = generator::working_directory(builder_root, application_id)?;
    let application_root = working
        .parent()
        .context("generated working directory has no application parent")?;
    if application_root.exists() {
        fs::remove_dir_all(application_root).with_context(|| {
            format!(
                "remove generated application state {}",
                application_root.display()
            )
        })?;
    }
    println!("cleaned generated state for `{application_id}`");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    struct RejectCommands;

    impl CommandRunner for RejectCommands {
        fn run(&self, command: crate::runner::CommandSpec) -> io::Result<CommandOutput> {
            panic!("check unexpectedly tried to run {:?}", command.argv());
        }
    }

    #[test]
    fn check_all_catalog_apps_is_read_only_and_process_free() {
        let builder = Path::new(env!("CARGO_MANIFEST_DIR"));
        for app in input_catalog::application_names() {
            execute_with_runner(
                Cli {
                    command: Command::Check {
                        app: app.to_owned(),
                    },
                },
                builder,
                &RejectCommands,
            )
            .unwrap();
        }
    }

    #[test]
    fn clean_removes_only_the_exact_catalog_application() {
        let temporary = tempfile::tempdir().unwrap();
        let selected = temporary
            .path()
            .join("generated/foxeer-f405-v2/working/src");
        let neighbor = temporary
            .path()
            .join("generated/nucleo-f401re-blinky/working/src");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&neighbor).unwrap();
        clean(temporary.path(), "foxeer-f405-v2").unwrap();
        assert!(!selected.exists());
        assert!(neighbor.exists());
    }

    #[test]
    fn unknown_clean_selection_does_not_mutate_output() {
        let temporary = tempfile::tempdir().unwrap();
        let marker = temporary.path().join("generated/unknown/working/marker");
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(&marker, b"retained").unwrap();
        assert!(clean(temporary.path(), "unknown").is_err());
        assert_eq!(fs::read(marker).unwrap(), b"retained");
    }
}
