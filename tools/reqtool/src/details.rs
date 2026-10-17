//! What a run record says about how the run was done: who ran it, on which
//! image, in what conditions, with which artifacts and observations. Board
//! evidence must carry it; the rules are the old run-record validator's
//! hardware rules, plus the propeller state each setup implies.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::evidence::{Outcome, RunResult};
use crate::fingerprint::hex;
use crate::schema::{Level, Setup};

/// Artifacts live in the ignored `logs/` tree; records hold only their path,
/// size and hash, never their content.
pub const ARTIFACT_ROOT: &str = "logs";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Details {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<Execution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<Candidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Conditions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub measurements: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_conditions_triggered: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub performed_by: Performer,
    pub user_confirmed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Performer {
    Agent,
    User,
    UserWithAgentAssistance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub firmware_sha256: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_triple: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub configuration: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    pub propellers: Propellers,
    pub actuator_power: ActuatorPower,
    #[serde(default)]
    pub equipment: Vec<String>,
    #[serde(default)]
    pub environment: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Propellers {
    Installed,
    Removed,
    NotApplicable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActuatorPower {
    Connected,
    Disconnected,
    NotApplicable,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRun {
    pub command: String,
    pub exit_code: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub kind: ArtifactKind,
    /// Repository-relative, under `logs/`.
    pub path: String,
    /// Filled in by `reqtool record` from the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Analysis,
    CommandOutput,
    Configuration,
    Firmware,
    Image,
    Log,
    Other,
    Report,
    Trace,
}

impl Details {
    /// Hashes every artifact from its file, refusing a path outside `logs/`,
    /// a missing file, or a stated hash that does not match.
    pub fn seal_artifacts(&mut self, repository: &Path) -> Result<(), String> {
        for artifact in &mut self.artifacts {
            let (bytes, sha256) = hash_artifact(repository, &artifact.path)?
                .ok_or_else(|| format!("artifact {} does not exist", artifact.path))?;
            if artifact
                .sha256
                .as_ref()
                .is_some_and(|stated| *stated != sha256)
            {
                return Err(format!(
                    "artifact {}: the stated SHA-256 does not match the file",
                    artifact.path
                ));
            }
            artifact.bytes = Some(bytes);
            artifact.sha256 = Some(sha256);
        }
        Ok(())
    }

    /// For a record read back: an artifact present on this machine must
    /// still match its hash. An absent one is fine; logs stay local.
    pub fn verify_artifacts(&self, repository: &Path) -> Result<(), String> {
        for artifact in &self.artifacts {
            if let Some((_, sha256)) = hash_artifact(repository, &artifact.path)?
                && artifact.sha256.as_deref() != Some(sha256.as_str())
            {
                return Err(format!(
                    "artifact {} no longer matches its recorded SHA-256",
                    artifact.path
                ));
            }
        }
        Ok(())
    }
}

fn hash_artifact(repository: &Path, path: &str) -> Result<Option<(u64, String)>, String> {
    let relative = Path::new(path);
    let inside = relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    if !inside || relative.components().next() != Some(Component::Normal(ARTIFACT_ROOT.as_ref())) {
        return Err(format!(
            "artifact {path} must be a relative path under {ARTIFACT_ROOT}/"
        ));
    }
    let full = repository.join(relative);
    if !full.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&full).map_err(|e| format!("artifact {path}: {e}"))?;
    Ok(Some((bytes.len() as u64, hex(&Sha256::digest(&bytes)))))
}

/// The hardware rules: whatever the method, a pass never carries a triggered
/// stop condition, and evidence on a board says who confirmed it, which image
/// ran, and the power and propeller state, consistent with each entry's
/// setup.
pub fn check(results: &[RunResult], details: Option<&Details>) -> Result<(), String> {
    let passes = results.iter().any(|result| result.result == Outcome::Pass);
    if let Some(details) = details
        && passes
        && !details.stop_conditions_triggered.is_empty()
    {
        return Err("a pass cannot include triggered stop conditions".into());
    }
    let on_board: Vec<&RunResult> = results
        .iter()
        .filter(|result| result.level.on_target())
        .collect();
    if on_board.is_empty() {
        return Ok(());
    }
    let details =
        details.ok_or("board evidence needs details: execution, candidate and conditions")?;
    let execution = details
        .execution
        .as_ref()
        .ok_or("board evidence needs execution")?;
    if execution.performed_by == Performer::Agent {
        return Err("board evidence cannot be agent-only".into());
    }
    if !execution.user_confirmed {
        return Err("board evidence needs the user's confirmation".into());
    }
    let candidate = details
        .candidate
        .as_ref()
        .ok_or("board evidence needs the candidate image")?;
    let sha = &candidate.firmware_sha256;
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(
            "board evidence needs the firmware image's SHA-256 (64 lowercase hex digits)".into(),
        );
    }
    let conditions = details
        .conditions
        .as_ref()
        .ok_or("board evidence needs conditions")?;
    if conditions.propellers == Propellers::NotApplicable
        || conditions.actuator_power == ActuatorPower::NotApplicable
    {
        return Err("board evidence needs the power and propeller state".into());
    }
    for result in on_board
        .into_iter()
        .filter(|result| result.result == Outcome::Pass)
    {
        if conditions.propellers == Propellers::Unknown
            || conditions.actuator_power == ActuatorPower::Unknown
        {
            return Err("a board pass needs a known power and propeller state".into());
        }
        if result.setup == Setup::PropsOff && conditions.propellers != Propellers::Removed {
            return Err(format!(
                "{}: a props-off pass needs propellers removed",
                result.requirement
            ));
        }
        if result.level == Level::Flight && conditions.propellers != Propellers::Installed {
            return Err(format!(
                "{}: a flight pass needs propellers installed",
                result.requirement
            ));
        }
    }
    Ok(())
}
