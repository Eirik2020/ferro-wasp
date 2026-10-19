//! Which crates and boards exist, read from their manifests.
//!
//! The manifests are read directly rather than through `cargo metadata`: the
//! root workspace pins a nightly toolchain, and running cargo there would make
//! a host-only run install it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A package in the repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Crate {
    /// The name Rust code uses, such as `ferrowasp_flight`.
    pub name: String,
    pub package: String,
    /// The directory holding its `Cargo.toml`.
    pub directory: PathBuf,
    /// The library's root source file, if it has a library.
    pub lib: Option<PathBuf>,
    /// The `Cargo.lock` that resolves its dependencies.
    pub lockfile: PathBuf,
    /// `[package.metadata.ferrowasp] critical = true`.
    pub critical: bool,
}

/// Every package, and every board's firmware app.
#[derive(Clone, Debug, Default)]
pub struct Workspace {
    /// Library crates, keyed by crate name.
    pub crates: BTreeMap<String, Crate>,
    /// Every package, keyed by package name.
    pub packages: BTreeMap<String, Crate>,
    /// Firmware apps, keyed by board: the directory name under `firmware/`.
    pub boards: BTreeMap<String, Crate>,
}

#[derive(Deserialize)]
struct Manifest {
    package: Option<Package>,
    lib: Option<LibTarget>,
    workspace: Option<ManifestWorkspace>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    metadata: Option<PackageMetadata>,
}

#[derive(Deserialize)]
struct PackageMetadata {
    ferrowasp: Option<FerroWaspMetadata>,
}

#[derive(Deserialize)]
struct FerroWaspMetadata {
    #[serde(default)]
    critical: bool,
}

#[derive(Deserialize)]
struct LibTarget {
    name: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
struct ManifestWorkspace {
    #[serde(default)]
    members: Vec<String>,
}

/// The root workspace's members plus each firmware app under `firmware/`.
pub fn discover(repository: &Path) -> Result<Workspace, String> {
    let root_lock = repository.join("Cargo.lock");
    let mut manifests = Vec::new();
    let root = read_manifest(&repository.join("Cargo.toml"))?;
    if let Some(workspace) = root.workspace {
        for member in workspace.members {
            if member.contains('*') {
                return Err(format!(
                    "Cargo.toml: glob member {member:?} is not supported"
                ));
            }
            manifests.push((
                repository.join(member).join("Cargo.toml"),
                root_lock.clone(),
                None,
            ));
        }
    }
    if let Ok(entries) = std::fs::read_dir(repository.join("firmware")) {
        let mut apps: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("Cargo.toml").is_file())
            .collect();
        apps.sort();
        for app in apps {
            let board = app
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
            manifests.push((app.join("Cargo.toml"), app.join("Cargo.lock"), board));
        }
    }

    let mut workspace = Workspace::default();
    for (path, lockfile, board) in manifests {
        let manifest = read_manifest(&path)?;
        let Some(package) = manifest.package else {
            continue;
        };
        let directory = path.parent().unwrap_or(repository).to_path_buf();
        let lib_path = manifest
            .lib
            .as_ref()
            .and_then(|lib| lib.path.as_deref())
            .map_or_else(
                || directory.join("src/lib.rs"),
                |custom| directory.join(custom),
            );
        let name = manifest
            .lib
            .and_then(|lib| lib.name)
            .unwrap_or_else(|| package.name.replace('-', "_"));
        let critical = package
            .metadata
            .and_then(|metadata| metadata.ferrowasp)
            .is_some_and(|ferrowasp| ferrowasp.critical);
        let found = Crate {
            name: name.clone(),
            package: package.name.clone(),
            directory,
            lib: lib_path.is_file().then_some(lib_path),
            lockfile,
            critical,
        };
        if let Some(board) = board {
            workspace.boards.insert(board, found.clone());
        }
        if found.lib.is_some()
            && let Some(previous) = workspace.crates.insert(name.clone(), found.clone())
        {
            return Err(format!(
                "crate name {name} is used by both {} and {}",
                previous.package, found.package
            ));
        }
        workspace.packages.insert(package.name, found);
    }
    Ok(workspace)
}

fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}
