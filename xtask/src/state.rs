//! Persistent build checkpoints, deterministic fingerprints, and safe directory
//! transitions for generated applications.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const BUILD_STATE_SCHEMA_VERSION: u32 = 1;

/// The complete set of inputs and checkpoint metadata required for safe resume.
///
/// This intentionally has no defaults and rejects unknown fields. A state written
/// by a different schema must never be mistaken for a usable checkpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildState {
    pub schema_version: u32,
    pub generator_version: String,
    pub backend_version: String,
    pub toolchain: String,
    pub cargo_lock_hash: String,
    pub template_hash: String,
    pub manifest_hash: String,
    pub ordered_feature_hash: String,
    /// Aggregate hash of every relevant generator input, including feature
    /// metadata and fragments not represented by the summary hashes above.
    pub input_hash: String,
    /// Hash of the complete successfully rendered working checkpoint.
    pub checkpoint_hash: String,
    pub successful_features: Vec<String>,
    pub next_feature_index: usize,
    /// Empty when the latest candidate did not fail, matching the on-disk schema
    /// from the implementation plan.
    pub failed_after_inserting: String,
}

impl BuildState {
    pub fn read_from(path: &Path) -> Result<Self> {
        read_build_state(path)
    }

    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        write_build_state(path, self)
    }
}

pub fn read_build_state(path: &Path) -> Result<BuildState> {
    require_regular_file_if_present(path, "build state")?;
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read build state {}", path.display()))?;
    toml::from_str(&source)
        .with_context(|| format!("failed to parse strict build state {}", path.display()))
}

/// Serialize state into a same-directory temporary file and atomically rename it.
///
/// `tempfile::NamedTempFile::persist` uses `MoveFileExW` with replace semantics on
/// Windows, so an existing state file is never deleted before its replacement is
/// ready.
pub fn write_build_state(path: &Path, state: &BuildState) -> Result<()> {
    let mut serialized =
        toml::to_string_pretty(state).context("failed to serialize build state")?;
    if !serialized.ends_with('\n') {
        serialized.push('\n');
    }
    write_atomic_file(path, serialized.as_bytes(), "build state")
}

pub fn write_atomic_file(path: &Path, contents: &[u8], label: &str) -> Result<()> {
    require_regular_file_if_present(path, label)?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {label} directory {}", parent.display()))?;

    let mut temporary = tempfile::Builder::new()
        .prefix(".atomic-write-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .with_context(|| {
            format!(
                "failed to create temporary {label} file in {}",
                parent.display()
            )
        })?;
    temporary
        .as_file_mut()
        .write_all(contents)
        .with_context(|| format!("failed to write temporary {label} for {}", path.display()))?;
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("failed to flush temporary {label} for {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to atomically replace {label} {}", path.display()))?;
    Ok(())
}

fn require_regular_file_if_present(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            bail!("{label} path is not a regular file: {}", path.display())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {label} {}", path.display()))
        }
    }
}

/// SHA-256 encoded as all 64 lowercase hexadecimal digits.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Hash labeled byte strings after sorting them by label and content.
///
/// Length framing makes the encoding unambiguous (`ab` + `c` cannot collide with
/// `a` + `bc`). Labels let callers build stable aggregate fingerprints without
/// incorporating machine-specific absolute paths.
pub fn hash_labeled_bytes<I, L, B>(inputs: I) -> String
where
    I: IntoIterator<Item = (L, B)>,
    L: AsRef<str>,
    B: AsRef<[u8]>,
{
    let mut inputs = inputs
        .into_iter()
        .map(|(label, bytes)| (label.as_ref().as_bytes().to_vec(), bytes.as_ref().to_vec()))
        .collect::<Vec<_>>();
    inputs.sort();

    let mut hasher = Sha256::new();
    hasher.update(b"rtic-app-builder:labeled-bytes:v1\0");
    for (label, bytes) in inputs {
        update_framed(&mut hasher, &label);
        update_framed(&mut hasher, &bytes);
    }
    hex::encode(hasher.finalize())
}

/// Read and hash labeled files deterministically, independent of iterator order.
pub fn hash_labeled_files<I, L, P>(files: I) -> Result<String>
where
    I: IntoIterator<Item = (L, P)>,
    L: AsRef<str>,
    P: AsRef<Path>,
{
    let mut contents = Vec::new();
    for (label, path) in files {
        let label = label.as_ref().to_owned();
        let path = path.as_ref();
        let bytes = fs::read(path).with_context(|| {
            format!(
                "failed to read fingerprint input {} ({label})",
                path.display()
            )
        })?;
        contents.push((label, bytes));
    }
    Ok(hash_labeled_bytes(contents))
}

fn update_framed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// Result of installing a validated candidate as the working application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionOutcome {
    pub working: PathBuf,
    /// A successfully displaced old checkpoint that could not be deleted (most
    /// commonly because a Windows process still has a file open). It is safe to
    /// remove later; the new `working` directory is already installed.
    pub retained_backup: Option<PathBuf>,
}

/// Promote `candidate` without risking the previous `working` checkpoint.
///
/// The old working directory is first renamed to a unique sibling backup. If the
/// candidate rename fails, it is renamed back. No directory replacement semantics
/// are assumed, which keeps the operation safe on Windows.
pub fn promote_candidate(candidate: &Path, working: &Path) -> Result<PromotionOutcome> {
    if candidate == working {
        bail!(
            "candidate and working paths must differ: {}",
            candidate.display()
        );
    }
    if !real_directory_exists(candidate, "candidate")? {
        bail!("candidate does not exist: {}", candidate.display());
    }
    let working_exists = real_directory_exists(working, "working")?;

    if !working_exists {
        fs::rename(candidate, working).with_context(|| {
            format!(
                "failed to promote candidate {} to {}",
                candidate.display(),
                working.display()
            )
        })?;
        return Ok(PromotionOutcome {
            working: working.to_path_buf(),
            retained_backup: None,
        });
    }

    let backup = next_sibling_backup_path(working)?;
    fs::rename(working, &backup).with_context(|| {
        format!(
            "failed to move working checkpoint {} to backup {}",
            working.display(),
            backup.display()
        )
    })?;

    if let Err(install_error) = fs::rename(candidate, working) {
        // Do not replace an unexpected path created between the two renames.
        let destination_reappeared = working.try_exists().unwrap_or(true);
        if destination_reappeared {
            return Err(anyhow!(
                "failed to promote candidate {} to {}: {install_error}; old working checkpoint remains at {} because the destination reappeared",
                candidate.display(),
                working.display(),
                backup.display()
            ));
        }

        return match fs::rename(&backup, working) {
            Ok(()) => Err(anyhow!(install_error)).with_context(|| {
                format!(
                    "failed to promote candidate {}; previous working checkpoint was restored",
                    candidate.display()
                )
            }),
            Err(rollback_error) => Err(anyhow!(
                "failed to promote candidate {} to {}: {install_error}; rollback also failed: {rollback_error}; old working checkpoint remains at {}",
                candidate.display(),
                working.display(),
                backup.display()
            )),
        };
    }

    let retained_backup = match fs::remove_dir_all(&backup) {
        Ok(()) => None,
        Err(_) => Some(backup),
    };
    Ok(PromotionOutcome {
        working: working.to_path_buf(),
        retained_backup,
    })
}

fn next_sibling_backup_path(working: &Path) -> Result<PathBuf> {
    let parent = working.parent().ok_or_else(|| {
        anyhow!(
            "working path has no parent for sibling backup: {}",
            working.display()
        )
    })?;
    let file_name = working.file_name().ok_or_else(|| {
        anyhow!(
            "working path has no file name for sibling backup: {}",
            working.display()
        )
    })?;

    let mut base = file_name.to_os_string();
    base.push(".backup");
    next_available_path(parent, &base)
}

/// Choose `failed/<feature>` and then `failed/<feature>-2`, `-3`, ... without
/// selecting an existing evidence directory.
pub fn next_failed_candidate_path(failed_root: &Path, feature: &str) -> Result<PathBuf> {
    let mut components = Path::new(feature).components();
    let is_one_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !is_one_normal_component || feature.is_empty() {
        bail!("feature name is not a safe path component: {feature:?}");
    }
    next_available_path(failed_root, &OsString::from(feature))
}

fn next_available_path(parent: &Path, base_name: &OsString) -> Result<PathBuf> {
    let base = parent.join(base_name);
    if !path_entry_exists(&base)? {
        return Ok(base);
    }

    for suffix in 2_u64.. {
        let mut name = base_name.clone();
        name.push(format!("-{suffix}"));
        let candidate = parent.join(name);
        if !path_entry_exists(&candidate)? {
            return Ok(candidate);
        }
    }
    unreachable!("u64 suffix space was exhausted")
}

fn path_entry_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect path {}", path.display()))
        }
    }
}

fn real_directory_exists(path: &Path, label: &str) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {label} {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} is not a real directory: {}", path.display());
    }
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "{label} path has no parent for confinement: {}",
            path.display()
        )
    })?;
    let name = path.file_name().ok_or_else(|| {
        anyhow!(
            "{label} path has no file name for confinement: {}",
            path.display()
        )
    })?;
    let canonical_parent = fs::canonicalize(parent)
        .with_context(|| format!("failed to resolve {label} parent {}", parent.display()))?;
    let canonical_path = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve {label} {}", path.display()))?;
    if canonical_path != canonical_parent.join(name) {
        bail!(
            "{label} resolves through a symlink or junction: {} -> {}",
            path.display(),
            canonical_path.display()
        );
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> BuildState {
        BuildState {
            schema_version: BUILD_STATE_SCHEMA_VERSION,
            generator_version: "0.1.0".into(),
            backend_version: "0.1.0".into(),
            toolchain: "1.85.0".into(),
            cargo_lock_hash: "lock".into(),
            template_hash: "template".into(),
            manifest_hash: "manifest".into(),
            ordered_feature_hash: "features".into(),
            input_hash: "all-inputs".into(),
            checkpoint_hash: "working-tree".into(),
            successful_features: vec!["blink_led".into()],
            next_feature_index: 1,
            failed_after_inserting: String::new(),
        }
    }

    #[test]
    fn state_round_trips_and_atomically_replaces_an_existing_file() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("nested/build-state.toml");
        let mut state = sample_state();

        write_build_state(&path, &state).unwrap();
        assert_eq!(read_build_state(&path).unwrap(), state);

        state.failed_after_inserting = "blink_led".into();
        write_build_state(&path, &state).unwrap();
        assert_eq!(read_build_state(&path).unwrap(), state);

        let entries = fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(entries, 1, "same-directory temporary file was left behind");
    }

    #[test]
    fn state_rejects_unknown_and_missing_fields() {
        let valid = toml::to_string(&sample_state()).unwrap();
        let unknown = format!("{valid}unknown = true\n");
        assert!(toml::from_str::<BuildState>(&unknown).is_err());

        let missing = valid
            .lines()
            .filter(|line| !line.starts_with("checkpoint_hash ="))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(toml::from_str::<BuildState>(&missing).is_err());
    }

    #[test]
    fn hashes_are_full_framed_and_independent_of_input_order() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let forward = hash_labeled_bytes([("manifest", b"abc"), ("template", b"def")]);
        let reverse = hash_labeled_bytes([("template", b"def"), ("manifest", b"abc")]);
        assert_eq!(forward, reverse);
        assert_eq!(forward.len(), 64);
        assert_ne!(
            hash_labeled_bytes([("ab", b"c")]),
            hash_labeled_bytes([("a", b"bc")])
        );
    }

    #[test]
    fn labeled_file_hashes_use_labels_and_contents_not_iteration_order() {
        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        fs::write(&first, b"one").unwrap();
        fs::write(&second, b"two").unwrap();

        let forward = hash_labeled_files([("a", &first), ("b", &second)]).unwrap();
        let reverse = hash_labeled_files([("b", &second), ("a", &first)]).unwrap();
        assert_eq!(forward, reverse);

        fs::write(&second, b"changed").unwrap();
        assert_ne!(
            forward,
            hash_labeled_files([("a", &first), ("b", &second)]).unwrap()
        );
    }

    #[test]
    fn promotion_replaces_working_and_removes_only_its_own_backup() {
        let temporary = tempfile::tempdir().unwrap();
        let candidate = temporary.path().join("candidate");
        let working = temporary.path().join("working");
        let stale_backup = temporary.path().join("working.backup");
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join("new"), b"new").unwrap();
        fs::create_dir(&working).unwrap();
        fs::write(working.join("old"), b"old").unwrap();
        fs::create_dir(&stale_backup).unwrap();
        fs::write(stale_backup.join("evidence"), b"keep").unwrap();

        let outcome = promote_candidate(&candidate, &working).unwrap();

        assert_eq!(outcome.retained_backup, None);
        assert!(working.join("new").is_file());
        assert!(!working.join("old").exists());
        assert!(!candidate.exists());
        assert_eq!(fs::read(stale_backup.join("evidence")).unwrap(), b"keep");
        assert!(!temporary.path().join("working.backup-2").exists());
    }

    #[test]
    fn failed_path_never_selects_existing_evidence() {
        let temporary = tempfile::tempdir().unwrap();
        let failed = temporary.path().join("failed");
        fs::create_dir(&failed).unwrap();

        assert_eq!(
            next_failed_candidate_path(&failed, "blink_led").unwrap(),
            failed.join("blink_led")
        );
        fs::create_dir(failed.join("blink_led")).unwrap();
        assert_eq!(
            next_failed_candidate_path(&failed, "blink_led").unwrap(),
            failed.join("blink_led-2")
        );
        fs::create_dir(failed.join("blink_led-2")).unwrap();
        assert_eq!(
            next_failed_candidate_path(&failed, "blink_led").unwrap(),
            failed.join("blink_led-3")
        );
        assert!(next_failed_candidate_path(&failed, "../escape").is_err());
    }

    #[test]
    fn missing_candidate_leaves_working_untouched() {
        let temporary = tempfile::tempdir().unwrap();
        let candidate = temporary.path().join("missing");
        let working = temporary.path().join("working");
        fs::create_dir(&working).unwrap();
        fs::write(working.join("checkpoint"), b"valid").unwrap();

        assert!(promote_candidate(&candidate, &working).is_err());
        assert_eq!(fs::read(working.join("checkpoint")).unwrap(), b"valid");
    }
}
