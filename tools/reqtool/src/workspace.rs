//! Which crates exist, read from their manifests.
//!
//! The manifests are read directly rather than through `cargo metadata`: the
//! root workspace pins a nightly toolchain, and running cargo there would make
//! a host-only run install it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A library crate that items can be allocated in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Crate {
    /// The name Rust code uses, such as `ferrowasp_flight`.
    pub name: String,
    pub package: String,
    /// The library's root source file.
    pub lib: PathBuf,
    /// `[package.metadata.ferrowasp] critical = true`.
    pub critical: bool,
}

#[derive(Deserialize)]
struct Manifest {
    package: Option<Package>,
    lib: Option<LibTarget>,
    workspace: Option<Workspace>,
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
struct Workspace {
    #[serde(default)]
    members: Vec<String>,
}

/// The root workspace's members plus each firmware app under `firmware/`,
/// keyed by crate name. Packages without a library are left out.
pub fn discover(repository: &Path) -> Result<BTreeMap<String, Crate>, String> {
    let mut manifests = Vec::new();
    let root = read_manifest(&repository.join("Cargo.toml"))?;
    if let Some(workspace) = root.workspace {
        for member in workspace.members {
            if member.contains('*') {
                return Err(format!(
                    "Cargo.toml: glob member {member:?} is not supported"
                ));
            }
            manifests.push(repository.join(member).join("Cargo.toml"));
        }
    }
    if let Ok(entries) = std::fs::read_dir(repository.join("firmware")) {
        let mut apps: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("Cargo.toml"))
            .filter(|path| path.is_file())
            .collect();
        apps.sort();
        manifests.extend(apps);
    }

    let mut crates = BTreeMap::new();
    for path in manifests {
        let manifest = read_manifest(&path)?;
        let Some(package) = manifest.package else {
            continue;
        };
        let directory = path.parent().unwrap_or(repository);
        let lib_path = manifest
            .lib
            .as_ref()
            .and_then(|lib| lib.path.as_deref())
            .map_or_else(
                || directory.join("src/lib.rs"),
                |custom| directory.join(custom),
            );
        if !lib_path.is_file() {
            continue;
        }
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
            package: package.name,
            lib: lib_path,
            critical,
        };
        if let Some(previous) = crates.insert(name.clone(), found) {
            return Err(format!(
                "crate name {name} is used by both {} and another package",
                previous.package
            ));
        }
    }
    Ok(crates)
}

fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}
