//! Injectable, shell-free process execution for firmware build and hardware commands.

use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

/// A complete child-process description that can be inspected before execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    /// Executable name without shell interpretation.
    pub program: OsString,
    /// Ordered argument vector.
    pub args: Vec<OsString>,
    /// Child-process working directory.
    pub cwd: PathBuf,
    /// Explicit environment overrides; all other entries are inherited.
    pub env_overrides: Vec<(OsString, OsString)>,
}

impl CommandSpec {
    /// Creates an empty command in the given working directory.
    pub fn new(program: impl Into<OsString>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env_overrides: Vec::new(),
        }
    }

    /// Appends one literal argument.
    pub fn arg(mut self, argument: impl Into<OsString>) -> Self {
        self.args.push(argument.into());
        self
    }

    /// Appends literal arguments in order.
    pub fn args<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Adds one explicit environment override.
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env_overrides.push((key.into(), value.into()));
        self
    }

    /// Returns the exact executable and arguments without lossy shell rendering.
    pub fn argv(&self) -> Vec<&OsStr> {
        std::iter::once(self.program.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
            .collect()
    }
}

/// Captured result of one child process.
#[derive(Debug)]
pub struct CommandOutput {
    /// Exact command that was run.
    pub command: CommandSpec,
    /// Process exit status.
    pub status: ExitStatus,
    /// Captured standard output.
    pub stdout: Vec<u8>,
    /// Captured standard error.
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    /// Returns whether the process exited successfully.
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Returns standard output as a lossy diagnostic string.
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Returns standard error as a lossy diagnostic string.
    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Injectable process boundary used by operations and unit tests.
pub trait CommandRunner {
    /// Executes one exact command without invoking a shell.
    fn run(&self, command: CommandSpec) -> io::Result<CommandOutput>;
}

/// Real child-process executor.
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

/// Constructs `cargo fmt --check` for a generated crate.
pub fn cargo_fmt_check_command(working: &Path, target_dir: &Path) -> CommandSpec {
    CommandSpec::new("cargo", working)
        .args([
            OsString::from("fmt"),
            OsString::from("--manifest-path"),
            working.join("Cargo.toml").into_os_string(),
            OsString::from("--"),
            OsString::from("--check"),
        ])
        .env("CARGO_TARGET_DIR", target_dir.as_os_str())
}

/// Constructs the locked embedded check command for a generated crate.
pub fn cargo_check_command(working: &Path, target: &str, target_dir: &Path) -> CommandSpec {
    CommandSpec::new("cargo", working)
        .args([
            OsString::from("check"),
            OsString::from("--manifest-path"),
            working.join("Cargo.toml").into_os_string(),
            OsString::from("--target"),
            OsString::from(target),
            OsString::from("--locked"),
        ])
        .env("CARGO_TARGET_DIR", target_dir.as_os_str())
}

/// Constructs the locked release-build command for a generated crate.
pub fn cargo_release_build_command(working: &Path, target: &str, target_dir: &Path) -> CommandSpec {
    CommandSpec::new("cargo", working)
        .args([
            OsString::from("build"),
            OsString::from("--manifest-path"),
            working.join("Cargo.toml").into_os_string(),
            OsString::from("--target"),
            OsString::from(target),
            OsString::from("--release"),
            OsString::from("--locked"),
        ])
        .env("CARGO_TARGET_DIR", target_dir.as_os_str())
}

/// Constructs the explicit probe-rs flash command for an existing ELF.
pub fn flash_command(builder_root: &Path, chip: &str, binary: &Path) -> CommandSpec {
    CommandSpec::new("probe-rs", builder_root).args([
        OsString::from("download"),
        OsString::from("--chip"),
        OsString::from(chip),
        OsString::from("--protocol"),
        OsString::from("swd"),
        OsString::from("--verify"),
        OsString::from("--reset"),
        binary.as_os_str().to_owned(),
    ])
}

/// Constructs the explicit cargo-embed command for an existing ELF.
pub fn embed_command(builder_root: &Path, chip: &str, binary: &Path) -> CommandSpec {
    CommandSpec::new("cargo", builder_root).args([
        OsString::from("embed"),
        OsString::from("--chip"),
        OsString::from(chip),
        OsString::from("--path"),
        binary.as_os_str().to_owned(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn release_build_is_locked_targeted_and_confined() {
        let working = Path::new("generated/foxeer-f405-v2/working");
        let target_dir = Path::new("generated/foxeer-f405-v2/target");
        let command = cargo_release_build_command(working, "thumbv7em-none-eabihf", target_dir);
        assert_eq!(command.program, OsString::from("cargo"));
        assert_eq!(
            command.args,
            strings(&[
                "build",
                "--manifest-path",
                "generated/foxeer-f405-v2/working/Cargo.toml",
                "--target",
                "thumbv7em-none-eabihf",
                "--release",
                "--locked"
            ])
        );
        assert_eq!(command.cwd, working);
        assert_eq!(
            command.env_overrides,
            vec![(
                OsString::from("CARGO_TARGET_DIR"),
                target_dir.as_os_str().to_owned()
            )]
        );
    }

    #[test]
    fn formatting_and_check_commands_are_exact() {
        let working = Path::new("generated/app/staging");
        let target_dir = Path::new("generated/app/target");
        assert_eq!(
            cargo_fmt_check_command(working, target_dir).args,
            strings(&[
                "fmt",
                "--manifest-path",
                "generated/app/staging/Cargo.toml",
                "--",
                "--check"
            ])
        );
        assert_eq!(
            cargo_check_command(working, "thumbv7em-none-eabihf", target_dir).args,
            strings(&[
                "check",
                "--manifest-path",
                "generated/app/staging/Cargo.toml",
                "--target",
                "thumbv7em-none-eabihf",
                "--locked"
            ])
        );
    }

    #[test]
    fn hardware_commands_are_exact_and_shell_free() {
        let root = Path::new("builder");
        let binary = Path::new("target/thumbv7em-none-eabihf/release/firmware");
        let flash = flash_command(root, "STM32F405RG", binary);
        assert_eq!(flash.program, OsString::from("probe-rs"));
        assert_eq!(
            flash.args,
            strings(&[
                "download",
                "--chip",
                "STM32F405RG",
                "--protocol",
                "swd",
                "--verify",
                "--reset",
                "target/thumbv7em-none-eabihf/release/firmware"
            ])
        );

        let embed = embed_command(root, "STM32F401RE", binary);
        assert_eq!(embed.program, OsString::from("cargo"));
        assert_eq!(
            embed.args,
            strings(&[
                "embed",
                "--chip",
                "STM32F401RE",
                "--path",
                "target/thumbv7em-none-eabihf/release/firmware"
            ])
        );
    }
}
