use std::{env, fs, path::PathBuf};

use serde::Serialize;

use crate::{
    config::FerroConfig,
    error::{FerroError, Result},
};

const MAX_PROFILE_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileInfo {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    pub fn for_current_user() -> Result<Self> {
        let base = env::var_os("FERRO_CONFIGURATOR_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
            .ok_or_else(|| {
                FerroError::Profile(
                    "neither FERRO_CONFIGURATOR_HOME nor APPDATA is available".to_owned(),
                )
            })?;
        let root = if env::var_os("FERRO_CONFIGURATOR_HOME").is_some() {
            base.join("profiles")
        } else {
            base.join("FerroConfigurator").join("profiles")
        };
        Ok(Self { root })
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn store(&self, name: &str, config: &FerroConfig, force: bool) -> Result<ProfileInfo> {
        let path = self.profile_path(name)?;
        fs::create_dir_all(&self.root).map_err(|error| {
            FerroError::Profile(format!(
                "could not create profile directory {}: {error}",
                self.root.display()
            ))
        })?;
        config.write_toml_file(&path, force)?;
        Ok(ProfileInfo {
            name: name.to_owned(),
            path,
        })
    }

    pub fn load(&self, name: &str) -> Result<FerroConfig> {
        FerroConfig::from_toml_file(&self.profile_path(name)?)
    }

    pub fn list(&self) -> Result<Vec<ProfileInfo>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(&self.root).map_err(|error| {
            FerroError::Profile(format!(
                "could not read profile directory {}: {error}",
                self.root.display()
            ))
        })?;
        let mut profiles = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| FerroError::Profile(error.to_string()))?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("toml")
            {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if validate_name(name).is_ok() {
                profiles.push(ProfileInfo {
                    name: name.to_owned(),
                    path,
                });
            }
        }
        profiles.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(profiles)
    }

    fn profile_path(&self, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        Ok(self.root.join(format!("{name}.toml")))
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > MAX_PROFILE_NAME_LEN {
        return Err(FerroError::Profile(format!(
            "profile name must contain 1 through {MAX_PROFILE_NAME_LEN} characters"
        )));
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(FerroError::Profile(
            "profile names may contain only letters, numbers, '-' and '_'".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_store_list_and_load_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProfileStore::at(directory.path().to_path_buf());
        let mut config = FerroConfig::default();
        config.roll.p = 0.42;

        let saved = store.store("five-inch_1", &config, false).unwrap();
        assert!(saved.path.is_file());
        assert_eq!(store.list().unwrap(), vec![saved]);
        assert_eq!(store.load("five-inch_1").unwrap(), config);
    }

    #[test]
    fn profile_names_cannot_escape_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProfileStore::at(directory.path().to_path_buf());
        for name in ["", "../escape", "drone/name", "name.toml", "has space"] {
            assert!(store.store(name, &FerroConfig::default(), false).is_err());
        }
    }

    #[test]
    fn existing_profiles_require_force_to_replace() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProfileStore::at(directory.path().to_path_buf());
        store.store("quad", &FerroConfig::default(), false).unwrap();
        assert!(store.store("quad", &FerroConfig::default(), false).is_err());
        assert!(store.store("quad", &FerroConfig::default(), true).is_ok());
    }
}
