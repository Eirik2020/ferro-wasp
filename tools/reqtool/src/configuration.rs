//! Configurations (design section 20): what one board image is built from.
//!
//! A configuration is a board's firmware app with a feature set. Its identity
//! covers:
//! - every repository file the compiler read for it, from the dep-info cargo
//!   writes for each crate, and every file a build script reruns on, except
//!   under `.git`;
//! - every environment variable the compiler read, such as `DEFMT_LOG`,
//!   except those a build script sets, such as the git revision;
//! - the app's manifest and its `.cargo/config.toml`;
//! - the version of every external crate compiled in;
//! - the toolchain.
//!
//! The app shell's source is among those files, so its scheduling table
//! (priorities and bindings) is covered, task names included until the
//! `app!` parser can leave them out.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::identity;
use crate::trace::relative;
use crate::workspace::Workspace;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub board: String,
    pub features: Vec<String>,
    /// Repository-relative path to identity.
    pub files: BTreeMap<String, String>,
    /// Package to version.
    pub externals: BTreeMap<String, String>,
    /// Environment variables the compiler read, to their values.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    pub toolchain: String,
}

impl Configuration {
    pub fn identity(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "board {}", self.board);
        for feature in &self.features {
            let _ = writeln!(text, "feature {feature}");
        }
        for (path, identity) in &self.files {
            let _ = writeln!(text, "file {path} {identity}");
        }
        for (package, version) in &self.externals {
            let _ = writeln!(text, "extern {package} {version}");
        }
        for (name, value) in &self.environment {
            let _ = writeln!(text, "env {name}={value}");
        }
        let _ = writeln!(text, "toolchain {}", self.toolchain);
        identity::sha256(text.as_bytes())
    }

    /// The files whose identity differs from `other`'s, or that only one of
    /// the two compiles in.
    pub fn differences(&self, other: &Self) -> Vec<String> {
        let paths: BTreeSet<&String> = self.files.keys().chain(other.files.keys()).collect();
        paths
            .into_iter()
            .filter(|path| self.files.get(*path) != other.files.get(*path))
            .cloned()
            .collect()
    }
}

/// Where configurations come from: cargo ([`Cargo`]), or a fixed set in
/// tests.
pub trait Configurations {
    fn configuration(&self, board: &str, features: &[String]) -> Result<Rc<Configuration>, String>;
}

/// A board and its sorted features.
type Key = (String, Vec<String>);

/// Configurations from `cargo check`, one run per board and feature set.
pub struct Cargo<'a> {
    repository: &'a Path,
    workspace: &'a Workspace,
    cache: RefCell<HashMap<Key, Rc<Configuration>>>,
}

impl<'a> Cargo<'a> {
    pub fn new(repository: &'a Path, workspace: &'a Workspace) -> Self {
        Self {
            repository,
            workspace,
            cache: RefCell::new(HashMap::new()),
        }
    }
}

impl Configurations for Cargo<'_> {
    fn configuration(&self, board: &str, features: &[String]) -> Result<Rc<Configuration>, String> {
        let mut features = features.to_vec();
        features.sort();
        features.dedup();
        let key = (board.to_owned(), features.clone());
        if let Some(found) = self.cache.borrow().get(&key) {
            return Ok(Rc::clone(found));
        }
        let app = self
            .workspace
            .boards
            .get(board)
            .ok_or_else(|| format!("no firmware app for board {board}"))?;
        let mut command = Command::new("cargo");
        // The app's own `rust-toolchain.toml` decides, not whichever
        // toolchain is running this tool.
        command
            .args(["check", "--locked", "--quiet", "--message-format=json"])
            .env_remove("RUSTUP_TOOLCHAIN")
            .env_remove("RUSTC")
            .env_remove("CARGO")
            .current_dir(&app.directory);
        if !features.is_empty() {
            command.arg("--features").arg(features.join(","));
        }
        let output = command
            .output()
            .map_err(|error| format!("{board}: cannot run cargo: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(15).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            return Err(format!(
                "{board}: cargo check failed, so its configuration is unknown:\n{}",
                tail.join("\n")
            ));
        }
        let compiled = from_messages(&String::from_utf8_lossy(&output.stdout), &app.directory)?;
        let mut sources: BTreeSet<PathBuf> = compiled.files;
        sources.insert(app.directory.join("Cargo.toml"));
        let config = app.directory.join(".cargo/config.toml");
        if config.is_file() {
            sources.insert(config);
        }
        let mut files = BTreeMap::new();
        for source in sources {
            if !source.starts_with(self.repository)
                || source
                    .components()
                    .any(|part| part.as_os_str() == "target" || part.as_os_str() == ".git")
            {
                continue;
            }
            files.insert(relative(self.repository, &source), identity::file(&source)?);
        }
        let configuration = Rc::new(Configuration {
            board: board.to_owned(),
            features,
            files,
            externals: compiled.externals,
            environment: compiled.environment,
            toolchain: identity::toolchain(self.repository, &app.directory)?,
        });
        self.cache
            .borrow_mut()
            .insert(key, Rc::clone(&configuration));
        Ok(configuration)
    }
}

/// What cargo's JSON messages say was compiled.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct Compiled {
    /// Every file in a crate's dep-info, and every file a build script
    /// reruns on.
    pub files: BTreeSet<PathBuf>,
    /// Registry and git packages, to their versions.
    pub externals: BTreeMap<String, String>,
    /// Environment variables the compiler read and no build script set.
    pub environment: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Message {
    reason: String,
    #[serde(default)]
    package_id: String,
    #[serde(default)]
    filenames: Vec<PathBuf>,
    #[serde(default)]
    out_dir: Option<PathBuf>,
    /// For a build script: the variables it set for the compiler.
    #[serde(default)]
    env: Vec<(String, String)>,
}

/// Reads cargo's messages; `base` is the workspace root, which relative
/// dep-info paths start from.
pub fn from_messages(stdout: &str, base: &Path) -> Result<Compiled, String> {
    let mut compiled = Compiled::default();
    let mut set_by_scripts = BTreeSet::new();
    for line in stdout.lines().filter(|line| line.starts_with('{')) {
        let message: Message =
            serde_json::from_str(line).map_err(|e| format!("cargo message: {e}"))?;
        match message.reason.as_str() {
            "compiler-artifact" => {
                if let Some((name, version)) = external(&message.package_id) {
                    compiled.externals.insert(name, version);
                    continue;
                }
                for artifact in &message.filenames {
                    let Some(dep_info) = dep_info_of(artifact) else {
                        continue;
                    };
                    let text = std::fs::read_to_string(&dep_info)
                        .map_err(|e| format!("{}: {e}", dep_info.display()))?;
                    compiled.files.extend(
                        dep_info_files(&text)
                            .into_iter()
                            .map(|file| base.join(file)),
                    );
                    compiled.environment.extend(dep_info_environment(&text));
                }
            }
            "build-script-executed" => {
                set_by_scripts.extend(message.env.iter().map(|(name, _)| name.clone()));
                let (Some(out_dir), Some(package)) =
                    (&message.out_dir, local_directory(&message.package_id))
                else {
                    continue;
                };
                let output = out_dir.with_file_name("output");
                if let Ok(text) = std::fs::read_to_string(&output) {
                    compiled.files.extend(rerun_files(&text, &package));
                }
            }
            _ => {}
        }
    }
    compiled
        .environment
        .retain(|name, _| !set_by_scripts.contains(name));
    Ok(compiled)
}

/// `# env-dep:NAME=value` lines, as rustc writes them into dep-info.
pub fn dep_info_environment(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("# env-dep:"))
        .map(|dependency| match dependency.split_once('=') {
            Some((name, value)) => (name.to_owned(), value.to_owned()),
            None => (dependency.to_owned(), String::new()),
        })
        .collect()
}

/// A path package's directory, from its ID:
/// `path+file:///dir#name@version`.
fn local_directory(package_id: &str) -> Option<PathBuf> {
    let rest = package_id.strip_prefix("path+file://")?;
    let directory = rest
        .split_once('#')
        .map_or(rest, |(directory, _)| directory);
    Some(PathBuf::from(directory))
}

/// `name` and `version` of a registry or git package ID; `None` for a path
/// package.
fn external(package_id: &str) -> Option<(String, String)> {
    if package_id.starts_with("path+") {
        return None;
    }
    let (_, name_version) = package_id.rsplit_once('#')?;
    let (name, version) = name_version.rsplit_once('@')?;
    Some((name.to_owned(), version.to_owned()))
}

/// The `.d` file beside a `lib<name>-<hash>.rmeta` (or `.rlib`, or a binary).
fn dep_info_of(artifact: &Path) -> Option<PathBuf> {
    let stem = artifact.file_stem()?.to_string_lossy().into_owned();
    let stem = stem.strip_prefix("lib").unwrap_or(&stem).to_owned();
    let candidate = artifact.with_file_name(format!("{stem}.d"));
    candidate.is_file().then_some(candidate)
}

/// The dependencies on the first line of a Makefile-style dep-info file:
/// `target: a b\ c`, with spaces in names escaped.
pub fn dep_info_files(text: &str) -> BTreeSet<PathBuf> {
    let Some(first) = text.lines().next() else {
        return BTreeSet::new();
    };
    let Some((_, list)) = first.split_once(": ") else {
        return BTreeSet::new();
    };
    let mut files = BTreeSet::new();
    let mut current = String::new();
    let mut chars = list.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&' ') => {
                current.push(' ');
                chars.next();
            }
            ' ' => {
                if !current.is_empty() {
                    files.insert(PathBuf::from(std::mem::take(&mut current)));
                }
            }
            other => current.push(other),
        }
    }
    if !current.is_empty() {
        files.insert(PathBuf::from(current));
    }
    files
}

/// `cargo:rerun-if-changed=` (or `cargo::`) paths in a build script's output,
/// relative to its package.
pub fn rerun_files(text: &str, package: &Path) -> BTreeSet<PathBuf> {
    text.lines()
        .filter_map(|line| {
            line.strip_prefix("cargo::rerun-if-changed=")
                .or_else(|| line.strip_prefix("cargo:rerun-if-changed="))
        })
        .map(|path| package.join(path.trim()))
        .filter(|path| path.is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dep_info_lists_the_first_rules_dependencies() {
        let text =
            "/t/deps/a-1.d: /r/src/lib.rs /r/src/my\\ file.rs /r/memory.x\n\n/r/src/lib.rs:\n";
        let files: Vec<String> = dep_info_files(text)
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, ["/r/memory.x", "/r/src/lib.rs", "/r/src/my file.rs"]);
    }

    #[test]
    fn environment_the_compiler_read_is_kept_unless_a_build_script_set_it() {
        let text = "a.d: src/main.rs\n\n# env-dep:DEFMT_LOG=info\n# env-dep:FWSP_GIT_REV=abc\n";
        let read = dep_info_environment(text);
        assert_eq!(read.len(), 2);
        assert_eq!(read["DEFMT_LOG"], "info");
    }

    #[test]
    fn package_ids_tell_external_from_local() {
        assert_eq!(
            external("registry+https://github.com/rust-lang/crates.io-index#heapless@0.8.0"),
            Some(("heapless".into(), "0.8.0".into()))
        );
        assert_eq!(external("path+file:///r/crates/a#0.1.0"), None);
        assert_eq!(
            local_directory("path+file:///r/crates/a#0.1.0"),
            Some(PathBuf::from("/r/crates/a"))
        );
    }

    #[test]
    fn a_configuration_identity_changes_with_any_part() {
        let base = Configuration {
            board: "b".into(),
            features: vec![],
            files: [("a.rs".to_owned(), "1".to_owned())].into(),
            externals: BTreeMap::new(),
            environment: BTreeMap::new(),
            toolchain: "t".into(),
        };
        let mut other = base.clone();
        other.files.insert("a.rs".into(), "2".into());
        assert_ne!(base.identity(), other.identity());
        assert_eq!(base.differences(&other), ["a.rs"]);
        let mut featured = base.clone();
        featured.features.push("x".into());
        assert_ne!(base.identity(), featured.identity());
    }
}
