//! Fingerprints (ADR-0052): the identity of what a piece of evidence covered,
//! derived from the code and never listed by hand.
//!
//! A fingerprint is a SHA-256 over sorted lines naming everything the
//! evidence depends on:
//! - every source file of every path crate in the dependency closure, from
//!   the governing `Cargo.lock`, which also lists dev-dependencies;
//! - each registry crate in that closure, by version and checksum;
//! - the toolchain file;
//! - the requirement's own statement and verification entry;
//! - for a procedure, the text of its section.
//!
//! Host-level entries take the closure of the allocated items' crates and the
//! test's crate. Target-level entries take the whole closure of the board's
//! firmware app: coarse, but it can never miss a change. The narrower path
//! scope waits for the `app!` parser.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::schema::{Entry, Method, Requirement};
use crate::workspace::Workspace;

#[derive(Deserialize)]
struct LockFile {
    #[serde(default)]
    package: Vec<LockPackage>,
}

#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

impl LockFile {
    /// A dependency as `Cargo.lock` writes it: `name`, `name version`, or
    /// `name version (source)`.
    fn find(&self, spec: &str) -> Option<&LockPackage> {
        let mut parts = spec.split_whitespace();
        let name = parts.next()?;
        let version = parts.next();
        self.package
            .iter()
            .find(|package| package.name == name && version.is_none_or(|v| package.version == v))
    }

    fn closure(&self, start: &str) -> Result<Vec<&LockPackage>, String> {
        let first = self
            .find(start)
            .ok_or_else(|| format!("{start} is not in its Cargo.lock"))?;
        let mut seen = BTreeSet::new();
        let mut queue = vec![first];
        let mut out = Vec::new();
        while let Some(package) = queue.pop() {
            if !seen.insert((package.name.as_str(), package.version.as_str())) {
                continue;
            }
            for dependency in &package.dependencies {
                let found = self
                    .find(dependency)
                    .ok_or_else(|| format!("Cargo.lock names {dependency} but does not list it"))?;
                queue.push(found);
            }
            out.push(package);
        }
        Ok(out)
    }
}

pub struct Fingerprinter<'a> {
    repository: &'a Path,
    workspace: &'a Workspace,
    locks: RefCell<HashMap<PathBuf, Rc<LockFile>>>,
    digests: RefCell<HashMap<String, Rc<Vec<String>>>>,
}

impl<'a> Fingerprinter<'a> {
    pub fn new(repository: &'a Path, workspace: &'a Workspace) -> Self {
        Self {
            repository,
            workspace,
            locks: RefCell::new(HashMap::new()),
            digests: RefCell::new(HashMap::new()),
        }
    }

    /// The current fingerprint of one verification entry. `board` is required
    /// for a target-level entry and refused for a host-level one.
    pub fn entry(
        &self,
        requirement: &Requirement,
        entry: &Entry,
        board: Option<&str>,
    ) -> Result<String, String> {
        let mut lines = BTreeSet::new();
        lines.insert(format!(
            "requirement {}",
            requirement_digest(requirement, entry)
        ));

        let mut roots: Vec<(PathBuf, String)> = Vec::new();
        match (entry.level.on_target(), board) {
            (true, Some(board)) => {
                let app = self
                    .workspace
                    .boards
                    .get(board)
                    .ok_or_else(|| format!("no firmware app for board {board}"))?;
                roots.push((app.lockfile.clone(), app.package.clone()));
            }
            (true, None) => {
                return Err(format!("{} evidence needs a board", entry.level.name()));
            }
            (false, Some(_)) => {
                return Err(format!("{} evidence has no board", entry.level.name()));
            }
            (false, None) => {}
        }
        let mut crate_names: Vec<&str> = requirement
            .allocated_to
            .iter()
            .filter_map(|path| path.split("::").next())
            .collect();
        if let Method::Test(path) = &entry.method {
            crate_names.extend(path.split("::").next());
        }
        for name in crate_names {
            let found = self
                .workspace
                .crates
                .get(name)
                .ok_or_else(|| format!("no crate {name} in the workspace"))?;
            roots.push((found.lockfile.clone(), found.package.clone()));
        }

        for (lockfile, package) in &roots {
            let lock = self.lock(lockfile)?;
            for member in lock.closure(package)? {
                match (&member.source, &member.checksum) {
                    (None, _) => {
                        for line in self.package_digest(&member.name)?.iter() {
                            lines.insert(line.clone());
                        }
                    }
                    (Some(source), checksum) => {
                        let identity = checksum.as_deref().unwrap_or(source);
                        lines.insert(format!(
                            "registry {} {} {identity}",
                            member.name, member.version
                        ));
                    }
                }
            }
            lines.insert(format!("toolchain {}", self.toolchain(lockfile)?));
        }

        if let Method::Procedure(reference) = &entry.method {
            lines.insert(format!("procedure {}", self.procedure_digest(reference)?));
        }

        let mut hasher = Sha256::new();
        for line in &lines {
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
        Ok(format!("sha256:{}", hex(&hasher.finalize())))
    }

    fn lock(&self, path: &Path) -> Result<Rc<LockFile>, String> {
        if let Some(lock) = self.locks.borrow().get(path) {
            return Ok(Rc::clone(lock));
        }
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", self.relative(path)))?;
        let lock: LockFile =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", self.relative(path)))?;
        let lock = Rc::new(lock);
        self.locks
            .borrow_mut()
            .insert(path.to_path_buf(), Rc::clone(&lock));
        Ok(lock)
    }

    /// `path <package> <file> <sha256>` for every file of a path package,
    /// leaving out nested packages and build output.
    fn package_digest(&self, package: &str) -> Result<Rc<Vec<String>>, String> {
        if let Some(digest) = self.digests.borrow().get(package) {
            return Ok(Rc::clone(digest));
        }
        let found = self
            .workspace
            .packages
            .get(package)
            .ok_or_else(|| format!("path crate {package} is not a workspace package"))?;
        let nested: Vec<&Path> = self
            .workspace
            .packages
            .values()
            .map(|other| other.directory.as_path())
            .filter(|other| *other != found.directory && other.starts_with(&found.directory))
            .collect();
        let mut lines = Vec::new();
        for file in self.files(&found.directory)? {
            if nested.iter().any(|inner| file.starts_with(inner))
                || file.components().any(|part| part.as_os_str() == "target")
            {
                continue;
            }
            let bytes =
                std::fs::read(&file).map_err(|e| format!("{}: {e}", self.relative(&file)))?;
            lines.push(format!(
                "path {package} {} {}",
                self.relative(&file),
                hex(&Sha256::digest(&bytes))
            ));
        }
        lines.sort();
        let lines = Rc::new(lines);
        self.digests
            .borrow_mut()
            .insert(package.to_owned(), Rc::clone(&lines));
        Ok(lines)
    }

    /// Tracked and untracked-but-not-ignored files under `directory`, from
    /// git; a plain directory walk outside a git checkout.
    fn files(&self, directory: &Path) -> Result<Vec<PathBuf>, String> {
        let listed = Command::new("git")
            .args([
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "--",
            ])
            .arg(directory)
            .current_dir(self.repository)
            .output();
        if let Ok(output) = listed
            && output.status.success()
        {
            let mut files: Vec<PathBuf> = output
                .stdout
                .split(|byte| *byte == 0)
                .filter(|name| !name.is_empty())
                .map(|name| self.repository.join(String::from_utf8_lossy(name).as_ref()))
                .filter(|path| path.is_file())
                .collect();
            files.sort();
            files.dedup();
            return Ok(files);
        }
        let mut files = Vec::new();
        walk(directory, &mut files).map_err(|e| format!("{}: {e}", self.relative(directory)))?;
        files.sort();
        Ok(files)
    }

    /// The nearest `rust-toolchain.toml` at or above the lockfile, up to the
    /// repository root.
    fn toolchain(&self, lockfile: &Path) -> Result<String, String> {
        for directory in lockfile.ancestors().skip(1) {
            let candidate = directory.join("rust-toolchain.toml");
            if candidate.is_file() {
                let bytes = std::fs::read(&candidate)
                    .map_err(|e| format!("{}: {e}", self.relative(&candidate)))?;
                return Ok(hex(&Sha256::digest(&bytes)));
            }
            if directory == self.repository {
                break;
            }
        }
        Ok("none".to_owned())
    }

    /// The procedure's section: its heading line up to the next heading of
    /// the same or a higher level.
    fn procedure_digest(&self, reference: &str) -> Result<String, String> {
        let (path, heading) = reference
            .split_once('#')
            .ok_or_else(|| format!("procedure {reference:?} is not path#heading"))?;
        let text = std::fs::read_to_string(self.repository.join(path))
            .map_err(|e| format!("procedure {path}: {e}"))?;
        let level = |line: &str| {
            let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
            (hashes > 0 && line[hashes..].starts_with(' ')).then_some(hashes)
        };
        let mut section = String::new();
        let mut depth = None;
        for line in text.lines() {
            match (depth, level(line)) {
                (None, Some(found)) if line[found..].trim() == heading => depth = Some(found),
                (Some(own), Some(found)) if found <= own => break,
                _ => {}
            }
            if depth.is_some() {
                section.push_str(line.trim_end());
                section.push('\n');
            }
        }
        if depth.is_none() {
            return Err(format!("procedure {path} has no heading {heading:?}"));
        }
        Ok(hex(&Sha256::digest(section.as_bytes())))
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(self.repository)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

/// The parts of a requirement that define what this entry claims. Whitespace
/// is collapsed, so re-wrapping the YAML leaves the fingerprint alone.
fn requirement_digest(requirement: &Requirement, entry: &Entry) -> String {
    let squash = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let verification = &requirement.verification;
    let mut text = String::new();
    let _ = writeln!(text, "id {}", requirement.id);
    let _ = writeln!(text, "statement {}", squash(&requirement.statement));
    let _ = writeln!(text, "mode {}", verification.mode.name());
    let _ = writeln!(
        text,
        "reason {}",
        squash(verification.reason.as_deref().unwrap_or(""))
    );
    let _ = writeln!(
        text,
        "observed_by {}",
        squash(verification.observed_by.as_deref().unwrap_or(""))
    );
    let method = match &entry.method {
        Method::Test(path) => format!("test {path}"),
        Method::Procedure(reference) => format!("procedure {reference}"),
    };
    let _ = writeln!(
        text,
        "entry {} {} {} {method}",
        entry.level.name(),
        entry.target.name(),
        entry.setup.name()
    );
    hex(&Sha256::digest(text.as_bytes()))
}

fn walk(directory: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if entry.file_name() != "target" && entry.file_name() != ".git" {
                walk(&path, out)?;
            }
        } else {
            out.push(path);
        }
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}
