//! Process execution for generated firmware crates.
//!
//! A command is represented separately from its execution so tests can assert the
//! exact program, arguments, working directory, and environment overrides.  A
//! non-zero exit status is an ordinary [`CommandOutput`]; callers need the captured
//! compiler diagnostics in order to preserve a failed candidate.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

/// The only embedded compilation target supported by the MVP backend.
pub const FIRMWARE_TARGET: &str = "thumbv7em-none-eabihf";

/// A complete, replayable description of a child process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Environment entries explicitly overridden by the assembler.
    ///
    /// The child otherwise inherits the assembler's environment.
    pub env_overrides: Vec<(OsString, OsString)>,
}

impl CommandSpec {
    pub fn new(program: impl Into<OsString>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env_overrides: Vec::new(),
        }
    }

    pub fn arg(mut self, argument: impl Into<OsString>) -> Self {
        self.args.push(argument.into());
        self
    }

    pub fn args<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(arguments.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env_overrides.push((key.into(), value.into()));
        self
    }

    /// The exact executable and argument vector, without lossy shell rendering.
    pub fn argv(&self) -> Vec<&OsStr> {
        std::iter::once(self.program.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
            .collect()
    }
}

/// Everything needed to report or preserve a completed command.
#[derive(Debug)]
pub struct CommandOutput {
    pub command: CommandSpec,
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn status_code(&self) -> Option<i32> {
        self.status.code()
    }

    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Injectable process boundary used by the assembler and unit tests.
pub trait CommandRunner {
    fn run(&self, command: CommandSpec) -> io::Result<CommandOutput>;
}

/// Executes real child processes with piped stdout and stderr.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, command: CommandSpec) -> io::Result<CommandOutput> {
        let output = Command::new(&command.program)
            .args(&command.args)
            .current_dir(&command.cwd)
            .envs(
                command
                    .env_overrides
                    .iter()
                    .map(|(key, value)| (key, value)),
            )
            .output()?;

        Ok(CommandOutput {
            command,
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn cargo_command(cwd: &Path, manifest_path: &Path, cargo_target_dir: Option<&Path>) -> CommandSpec {
    let command = CommandSpec::new("cargo", cwd)
        .arg("--manifest-path")
        .arg(manifest_path.as_os_str());

    match cargo_target_dir {
        Some(target_dir) => command.env("CARGO_TARGET_DIR", target_dir.as_os_str()),
        None => command,
    }
}

/// `cargo fmt --manifest-path <manifest>`.
pub fn cargo_fmt_command(
    cwd: &Path,
    manifest_path: &Path,
    cargo_target_dir: Option<&Path>,
) -> CommandSpec {
    CommandSpec::new("cargo", cwd)
        .arg("fmt")
        .arg("--manifest-path")
        .arg(manifest_path.as_os_str())
        .pipe_target_dir(cargo_target_dir)
}

/// `cargo fmt --manifest-path <manifest> -- --check`.
pub fn cargo_fmt_check_command(
    cwd: &Path,
    manifest_path: &Path,
    cargo_target_dir: Option<&Path>,
) -> CommandSpec {
    cargo_fmt_command(cwd, manifest_path, cargo_target_dir)
        .arg("--")
        .arg("--check")
}

/// `cargo check --manifest-path <manifest> --target <target> --locked`.
pub fn cargo_check_command(
    cwd: &Path,
    manifest_path: &Path,
    target: &str,
    cargo_target_dir: Option<&Path>,
) -> CommandSpec {
    cargo_command(cwd, manifest_path, cargo_target_dir)
        .arg("check")
        // Keep the documented command ordering even though Cargo accepts either
        // position for the subcommand.
        .reorder_cargo_subcommand()
        .arg("--target")
        .arg(target)
        .arg("--locked")
}

/// `cargo build --manifest-path <manifest> --target <target> --release --locked`.
pub fn cargo_release_build_command(
    cwd: &Path,
    manifest_path: &Path,
    target: &str,
    cargo_target_dir: Option<&Path>,
) -> CommandSpec {
    cargo_command(cwd, manifest_path, cargo_target_dir)
        .arg("build")
        .reorder_cargo_subcommand()
        .arg("--target")
        .arg(target)
        .arg("--release")
        .arg("--locked")
}

trait CommandSpecExt {
    fn pipe_target_dir(self, cargo_target_dir: Option<&Path>) -> Self;
    fn reorder_cargo_subcommand(self) -> Self;
}

impl CommandSpecExt for CommandSpec {
    fn pipe_target_dir(self, cargo_target_dir: Option<&Path>) -> Self {
        match cargo_target_dir {
            Some(target_dir) => self.env("CARGO_TARGET_DIR", target_dir.as_os_str()),
            None => self,
        }
    }

    fn reorder_cargo_subcommand(mut self) -> Self {
        // `cargo_command` starts with `--manifest-path <path>` so callers can
        // append their subcommand. Cargo's desired argv has the subcommand first.
        let subcommand = self
            .args
            .pop()
            .expect("cargo subcommand was appended before reordering");
        self.args.insert(0, subcommand);
        self
    }
}

pub fn run_cargo_fmt<R: CommandRunner + ?Sized>(
    runner: &R,
    cwd: &Path,
    manifest_path: &Path,
    cargo_target_dir: Option<&Path>,
) -> io::Result<CommandOutput> {
    runner.run(cargo_fmt_command(cwd, manifest_path, cargo_target_dir))
}

pub fn run_cargo_fmt_check<R: CommandRunner + ?Sized>(
    runner: &R,
    cwd: &Path,
    manifest_path: &Path,
    cargo_target_dir: Option<&Path>,
) -> io::Result<CommandOutput> {
    runner.run(cargo_fmt_check_command(
        cwd,
        manifest_path,
        cargo_target_dir,
    ))
}

pub fn run_cargo_check<R: CommandRunner + ?Sized>(
    runner: &R,
    cwd: &Path,
    manifest_path: &Path,
    target: &str,
    cargo_target_dir: Option<&Path>,
) -> io::Result<CommandOutput> {
    runner.run(cargo_check_command(
        cwd,
        manifest_path,
        target,
        cargo_target_dir,
    ))
}

pub fn run_cargo_release_build<R: CommandRunner + ?Sized>(
    runner: &R,
    cwd: &Path,
    manifest_path: &Path,
    target: &str,
    cargo_target_dir: Option<&Path>,
) -> io::Result<CommandOutput> {
    runner.run(cargo_release_build_command(
        cwd,
        manifest_path,
        target,
        cargo_target_dir,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn fmt_check_command_is_exact_and_keeps_external_target_dir() {
        let cwd = Path::new("candidate");
        let manifest = Path::new("C:/generated/candidate/Cargo.toml");
        let external_target = Path::new("C:/build-cache/rtic-target");

        let command = cargo_fmt_check_command(cwd, manifest, Some(external_target));

        assert_eq!(command.program, OsString::from("cargo"));
        assert_eq!(
            command.args,
            strings(&[
                "fmt",
                "--manifest-path",
                "C:/generated/candidate/Cargo.toml",
                "--",
                "--check"
            ])
        );
        assert_eq!(command.cwd, cwd);
        assert_eq!(
            command.env_overrides,
            vec![(
                OsString::from("CARGO_TARGET_DIR"),
                external_target.as_os_str().to_owned()
            )]
        );
    }

    #[test]
    fn check_and_release_commands_follow_the_required_order() {
        let cwd = Path::new("candidate");
        let manifest = Path::new("candidate/Cargo.toml");

        let check = cargo_check_command(cwd, manifest, FIRMWARE_TARGET, None);
        assert_eq!(
            check.args,
            strings(&[
                "check",
                "--manifest-path",
                "candidate/Cargo.toml",
                "--target",
                FIRMWARE_TARGET,
                "--locked"
            ])
        );

        let build = cargo_release_build_command(cwd, manifest, FIRMWARE_TARGET, None);
        assert_eq!(
            build.args,
            strings(&[
                "build",
                "--manifest-path",
                "candidate/Cargo.toml",
                "--target",
                FIRMWARE_TARGET,
                "--release",
                "--locked"
            ])
        );
    }

    #[test]
    fn process_runner_captures_command_cwd_status_and_both_streams() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let command = CommandSpec::new(executable, temporary.path())
            .arg("--exact")
            .arg("runner::tests::process_runner_child")
            .arg("--ignored")
            .arg("--nocapture")
            .env("RTIC_RUNNER_CHILD", "1");

        let output = ProcessRunner.run(command.clone()).unwrap();

        assert!(output.success(), "{}", output.stderr_lossy());
        assert_eq!(output.command, command);
        assert_eq!(output.command.cwd, temporary.path());
        assert!(output.stdout_lossy().contains("runner-child-stdout"));
        assert!(output.stderr_lossy().contains("runner-child-stderr"));
    }

    #[test]
    #[ignore = "executed as a subprocess by process_runner_captures_command_cwd_status_and_both_streams"]
    fn process_runner_child() {
        assert_eq!(std::env::var("RTIC_RUNNER_CHILD").as_deref(), Ok("1"));
        println!("runner-child-stdout");
        eprintln!("runner-child-stderr");
    }
}
