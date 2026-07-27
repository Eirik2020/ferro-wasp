use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    error::{FerroError, Result},
    firmware::board::BoardProfile,
};

const MANIFEST_VERSION: u16 = 1;
const MANIFEST_RELATIVE_PATH: &str = "firmware/manifest.json";
const FOXEER_REQUIRED_FEATURES: &[&str] = &[
    "board-foxeer-f405-v2",
    "dshot",
    "esc_telemetry",
    "flash_storage",
    "flash_writes",
    "flash_blackbox",
    "usb_serial",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub manifest_version: u16,
    pub release_version: String,
    pub git_commit: String,
    pub artifacts: Vec<FirmwareArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareArtifact {
    pub board: String,
    pub target: String,
    pub features: Vec<String>,
    pub file: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundledFirmware {
    pub release_version: String,
    pub git_commit: String,
    pub board: String,
    pub target: String,
    pub features: Vec<String>,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}

pub fn find_bundled_firmware(profile: &BoardProfile) -> Result<BundledFirmware> {
    let executable = env::current_exe().map_err(|error| FerroError::InvalidFirmware {
        reason: format!("could not locate FerroConfigurator executable: {error}"),
    })?;
    let root = executable
        .parent()
        .ok_or_else(|| FerroError::InvalidFirmware {
            reason: "FerroConfigurator executable has no parent directory".to_owned(),
        })?;
    load_bundled_firmware(&root.join(MANIFEST_RELATIVE_PATH), profile)
}

pub fn load_bundled_firmware(
    manifest_path: &Path,
    profile: &BoardProfile,
) -> Result<BundledFirmware> {
    let encoded = fs::read(manifest_path).map_err(|error| FerroError::InvalidFirmware {
        reason: format!(
            "could not read bundled firmware manifest {}: {error}",
            manifest_path.display()
        ),
    })?;
    let manifest: ReleaseManifest =
        serde_json::from_slice(&encoded).map_err(|error| FerroError::InvalidFirmware {
            reason: format!(
                "bundled firmware manifest {} is invalid: {error}",
                manifest_path.display()
            ),
        })?;
    if manifest.manifest_version != MANIFEST_VERSION {
        return invalid(format!(
            "firmware manifest version {} is unsupported",
            manifest.manifest_version
        ));
    }
    if manifest.release_version.trim().is_empty() || manifest.git_commit.trim().is_empty() {
        return invalid("firmware manifest lacks release provenance".to_owned());
    }
    let matches = manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.board == profile.id)
        .collect::<Vec<_>>();
    let artifact = match matches.as_slice() {
        [artifact] => *artifact,
        [] => {
            return invalid(format!("firmware manifest has no image for {}", profile.id));
        }
        _ => {
            return invalid(format!(
                "firmware manifest has multiple images for {}",
                profile.id
            ));
        }
    };
    if artifact.target != "thumbv7em-none-eabihf" {
        return invalid(format!(
            "bundled {} image targets {}, expected thumbv7em-none-eabihf",
            profile.id, artifact.target
        ));
    }
    for required in FOXEER_REQUIRED_FEATURES {
        if !artifact.features.iter().any(|feature| feature == required) {
            return invalid(format!(
                "bundled {} image lacks required feature {required}",
                profile.id
            ));
        }
    }
    let relative = Path::new(&artifact.file);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid("firmware manifest image path is not a safe relative path".to_owned());
    }
    let root = manifest_path
        .parent()
        .ok_or_else(|| FerroError::InvalidFirmware {
            reason: "firmware manifest has no parent directory".to_owned(),
        })?;
    let path = root.join(relative);
    let metadata = fs::metadata(&path).map_err(|error| FerroError::InvalidFirmware {
        reason: format!(
            "could not inspect bundled image {}: {error}",
            path.display()
        ),
    })?;
    if metadata.len() != artifact.bytes {
        return invalid(format!(
            "bundled image size is {}, manifest requires {}",
            metadata.len(),
            artifact.bytes
        ));
    }
    let digest = sha256_file(&path)?;
    if !digest.eq_ignore_ascii_case(&artifact.sha256) {
        return invalid(format!(
            "bundled image SHA-256 {digest} does not match manifest {}",
            artifact.sha256
        ));
    }
    Ok(BundledFirmware {
        release_version: manifest.release_version,
        git_commit: manifest.git_commit,
        board: artifact.board.clone(),
        target: artifact.target.clone(),
        features: artifact.features.clone(),
        path,
        sha256: digest,
        bytes: artifact.bytes,
    })
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|error| FerroError::InvalidFirmware {
        reason: format!("could not read {} for SHA-256: {error}", path.display()),
    })?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02X}")).collect())
}

fn invalid<T>(reason: String) -> Result<T> {
    Err(FerroError::InvalidFirmware { reason })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn manifest(path: &Path, image: &[u8], hash: String) {
        let firmware = path.join("foxeer-f405-v2");
        fs::create_dir_all(&firmware).unwrap();
        fs::write(firmware.join("FerroWaspFoxeerF405V2.elf"), image).unwrap();
        let manifest = ReleaseManifest {
            manifest_version: 1,
            release_version: "0.2.0".to_owned(),
            git_commit: "0123456789abcdef".to_owned(),
            artifacts: vec![FirmwareArtifact {
                board: "foxeer-f405-v2".to_owned(),
                target: "thumbv7em-none-eabihf".to_owned(),
                features: FOXEER_REQUIRED_FEATURES
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
                file: "foxeer-f405-v2/FerroWaspFoxeerF405V2.elf".to_owned(),
                sha256: hash,
                bytes: image.len() as u64,
            }],
        };
        fs::write(
            path.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn validates_bundled_image_hash_features_and_board() {
        let root = tempdir().unwrap();
        let image = b"firmware image";
        let image_path = root.path().join("foxeer-f405-v2/FerroWaspFoxeerF405V2.elf");
        fs::create_dir_all(image_path.parent().unwrap()).unwrap();
        fs::write(&image_path, image).unwrap();
        let hash = sha256_file(&image_path).unwrap();
        manifest(root.path(), image, hash.clone());

        let bundled = load_bundled_firmware(
            &root.path().join("manifest.json"),
            &BoardProfile::FOXEER_F405_V2,
        )
        .unwrap();
        assert_eq!(bundled.sha256, hash);
        assert_eq!(bundled.bytes, image.len() as u64);
    }

    #[test]
    fn rejects_hash_mismatch_and_parent_paths() {
        let root = tempdir().unwrap();
        manifest(root.path(), b"firmware", "00".repeat(32));
        assert!(
            load_bundled_firmware(
                &root.path().join("manifest.json"),
                &BoardProfile::FOXEER_F405_V2,
            )
            .is_err()
        );

        let mut decoded: ReleaseManifest =
            serde_json::from_slice(&fs::read(root.path().join("manifest.json")).unwrap()).unwrap();
        decoded.artifacts[0].file = "../outside.elf".to_owned();
        fs::write(
            root.path().join("manifest.json"),
            serde_json::to_vec_pretty(&decoded).unwrap(),
        )
        .unwrap();
        assert!(
            load_bundled_firmware(
                &root.path().join("manifest.json"),
                &BoardProfile::FOXEER_F405_V2,
            )
            .is_err()
        );
    }
}
