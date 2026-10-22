//! Gates (design §3): named checkpoints, each declaring which requirements,
//! at which levels, must be verified before it opens. Only gates block.
//!
//! Gates live in `project_meta/requirements/gates.yaml`. A gate may include
//! other gates. A requirement selector is an ID, a prefix ending in `*`, or
//! `*` for every requirement; a selected requirement contributes each of its
//! entries at the gate's levels, and must have at least one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::schema::{Level, Requirement};
use crate::status::{Evidence, Status};

pub const GATES: &str = "project_meta/requirements/gates.yaml";
pub const GATES_FILE_NAME: &str = "gates.yaml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateFile {
    pub schema_version: u32,
    pub gates: Vec<Gate>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(default)]
    pub requires: Vec<Clause>,
}

/// One clause: these requirements, verified at these levels.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clause {
    pub requirements: Vec<String>,
    pub levels: Vec<Level>,
}

pub fn load(repository: &Path) -> Result<Vec<Gate>, String> {
    let path = repository.join(GATES);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    let file: GateFile = serde_saphyr::from_str(&text).map_err(|e| format!("{GATES}: {e}"))?;
    if file.schema_version != 1 {
        return Err(format!(
            "{GATES}: schema_version {} is not 1",
            file.schema_version
        ));
    }
    Ok(file.gates)
}

pub fn selects(selector: &str, id: &str) -> bool {
    match selector.strip_suffix('*') {
        Some(prefix) => id.starts_with(prefix),
        None => selector == id,
    }
}

/// Problems with the gates: unknown or repeated names, include cycles,
/// selectors that select nothing, and selected requirements with no entry at
/// the clause's levels.
pub fn validate(gates: &[Gate], requirements: &BTreeMap<String, Requirement>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut names = BTreeSet::new();
    for gate in gates {
        if !names.insert(gate.name.as_str()) {
            problems.push(format!("gate {} is declared twice", gate.name));
        }
    }
    for gate in gates {
        for included in &gate.includes {
            if !names.contains(included.as_str()) {
                problems.push(format!(
                    "gate {} includes unknown gate {included}",
                    gate.name
                ));
            }
        }
        for clause in &gate.requires {
            if clause.levels.is_empty() {
                problems.push(format!("gate {}: a clause names no level", gate.name));
            }
            for selector in &clause.requirements {
                let selected: Vec<&Requirement> = requirements
                    .values()
                    .filter(|r| selects(selector, &r.id))
                    .collect();
                if selected.is_empty() {
                    problems.push(format!(
                        "gate {}: {selector} selects no requirement",
                        gate.name
                    ));
                }
                if selector.ends_with('*') {
                    continue;
                }
                for requirement in selected {
                    let has_entry = requirement
                        .verification
                        .entries
                        .iter()
                        .any(|entry| clause.levels.contains(&entry.level));
                    if !has_entry {
                        problems.push(format!(
                            "gate {}: {} has no entry at {}",
                            gate.name,
                            requirement.id,
                            level_names(&clause.levels)
                        ));
                    }
                }
            }
        }
    }
    for gate in gates {
        if expand(gates, &gate.name).is_err() {
            problems.push(format!("gate {} includes itself", gate.name));
        }
    }
    problems
}

/// Every clause of a gate and the gates it includes.
fn expand<'g>(gates: &'g [Gate], name: &str) -> Result<Vec<&'g Clause>, String> {
    fn walk<'g>(
        gates: &'g [Gate],
        name: &str,
        path: &mut Vec<String>,
        out: &mut Vec<&'g Clause>,
    ) -> Result<(), String> {
        if path.iter().any(|seen| seen == name) {
            return Err(format!("include cycle through {name}"));
        }
        let gate = gates
            .iter()
            .find(|gate| gate.name == name)
            .ok_or_else(|| format!("no gate {name}"))?;
        path.push(name.to_owned());
        for included in &gate.includes {
            walk(gates, included, path, out)?;
        }
        path.pop();
        out.extend(gate.requires.iter());
        Ok(())
    }
    let mut out = Vec::new();
    walk(gates, name, &mut Vec::new(), &mut out)?;
    Ok(out)
}

/// What stands between a gate and opening, entry by entry.
#[derive(Debug, Eq, PartialEq)]
pub struct Missing {
    pub requirement: String,
    pub level: Level,
    pub status: Status,
}

/// Checks a gate for one board. Target-level entries are judged on that
/// board; host-level entries need no board.
pub fn check(
    gates: &[Gate],
    name: &str,
    board: Option<&str>,
    requirements: &BTreeMap<String, Requirement>,
    evidence: &dyn Evidence,
) -> Result<Vec<Missing>, String> {
    let clauses = expand(gates, name)?;
    let needs_board = clauses
        .iter()
        .any(|clause| clause.levels.iter().any(|level| level.on_target()));
    if needs_board && board.is_none() {
        return Err(format!(
            "gate {name} has board-level requirements; pass --board"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut missing = Vec::new();
    for clause in clauses {
        for requirement in requirements.values() {
            if !clause
                .requirements
                .iter()
                .any(|selector| selects(selector, &requirement.id))
            {
                continue;
            }
            for (index, entry) in requirement.verification.entries.iter().enumerate() {
                if !clause.levels.contains(&entry.level)
                    || !seen.insert((requirement.id.clone(), index))
                {
                    continue;
                }
                let status = if entry.level.on_target() {
                    let board = board.unwrap_or_default();
                    evidence
                        .boards(requirement, index)
                        .into_iter()
                        .find(|(name, _)| name == board)
                        .map_or(Status::Unverified, |(_, status)| status)
                } else {
                    evidence.entry(requirement, index)
                };
                if status != Status::Verified {
                    missing.push(Missing {
                        requirement: requirement.id.clone(),
                        level: entry.level,
                        status,
                    });
                }
            }
        }
    }
    Ok(missing)
}

fn level_names(levels: &[Level]) -> String {
    levels
        .iter()
        .map(|level| level.name())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::selects;

    #[test]
    fn selectors_match_ids_prefixes_and_everything() {
        assert!(selects("INPUT-001", "INPUT-001"));
        assert!(!selects("INPUT-001", "INPUT-0010"));
        assert!(selects("INPUT-*", "INPUT-010"));
        assert!(!selects("INPUT-*", "ARM-001"));
        assert!(selects("*", "ARM-001"));
    }
}
