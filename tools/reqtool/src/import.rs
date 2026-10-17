//! `reqtool import`: carry the old schema 1 and 2 run records over as schema
//! 3 evidence, re-fingerprinted at each record's own git revision
//! (ADR-0053).
//!
//! An old record proves a catalog test, which names a procedure. Each
//! requirement entry that names the same procedure gets a result, with its
//! fingerprint computed in a temporary git worktree of the record's revision:
//! the code and the procedure text as they were, and the requirement's claim
//! as it is now. A record whose revision is not in this repository, or whose
//! items did not exist yet, gets an unavailable fingerprint, so a pass shows
//! as stale rather than as current. The originals are never changed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::details::Details;
use crate::evidence::{self, Outcome, RUNS, Record, RunResult};
use crate::fingerprint::Fingerprinter;
use crate::project::Project;
use crate::schema::Method;
use crate::workspace;

pub const CATALOG: &str = "project_meta/testing/TEST_CATALOG.json";

/// Catalog tests whose records have no requirement to carry over to.
const NOT_IMPORTED: &[(&str, &str)] = &[
    (
        "SW-COMMON-001",
        "the software suite, superseded by `reqtool test`",
    ),
    (
        "PREFLIGHT-",
        "preflight is a gate, not a verification level",
    ),
];

/// What happened to one old record.
#[derive(Debug, Eq, PartialEq)]
pub enum Imported {
    /// Written, with this many results.
    Written { path: PathBuf, results: usize },
    /// Already imported earlier.
    Present(PathBuf),
    /// Not carried over, and why.
    Skipped(String),
}

pub struct Plan {
    pub source: PathBuf,
    pub outcome: Result<Option<Record>, String>,
}

/// Builds the schema 3 record for every old record, without writing.
pub fn plan(project: &Project) -> Result<Vec<Plan>, String> {
    let catalog = read_json(&project.repository.join(CATALOG))?;
    let mut procedures = BTreeMap::new();
    let mut retired = BTreeSet::new();
    for test in catalog["tests"].as_array().into_iter().flatten() {
        let id = test["id"].as_str().unwrap_or_default().to_owned();
        if test["status"] == "retired" {
            retired.insert(id.clone());
        }
        let path = test["procedure"]["path"].as_str().unwrap_or_default();
        let heading = test["procedure"]["heading"].as_str().unwrap_or_default();
        procedures.insert(id, format!("{path}#{heading}"));
    }

    let mut sources = Vec::new();
    collect(&project.repository.join(RUNS), &mut sources);
    sources.sort();
    let mut worktrees = Worktrees::new(&project.repository);
    let mut plans = Vec::new();
    for source in sources {
        let old = read_json(&source)?;
        let version = old["schema_version"].as_u64().unwrap_or(0);
        if version == u64::from(evidence::RECORD_SCHEMA_VERSION) {
            continue;
        }
        let outcome = (|| {
            let test = old["test_id"].as_str().ok_or("no test_id")?;
            if let Some((_, why)) = NOT_IMPORTED
                .iter()
                .find(|(prefix, _)| test.starts_with(prefix))
            {
                return Err(format!("{test}: {why}"));
            }
            if retired.contains(test) {
                return Err(format!("{test}: the catalog test is retired"));
            }
            let procedure = procedures
                .get(test)
                .ok_or_else(|| format!("{test}: not in the catalog"))?;
            let outcome = match old["result"].as_str() {
                Some("pass") => Outcome::Pass,
                Some("fail") => Outcome::Fail,
                other => return Err(format!("{test}: result {other:?} is neither pass nor fail")),
            };
            let target = old["target"].as_str().unwrap_or("common");
            let revision = old["candidate"]["revision"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let method = Method::Procedure(procedure.clone());
            let run_id = format!("{}__imported", old["run_id"].as_str().ok_or("no run_id")?);
            let root = worktrees.at(&revision);

            let mut results = Vec::new();
            let mut board_used = None;
            for requirement in project.requirements.values() {
                for entry in &requirement.verification.entries {
                    if entry.method != method {
                        continue;
                    }
                    let board = entry.level.on_target().then_some(target);
                    if board.is_some() {
                        board_used = board.map(str::to_owned);
                    }
                    let fingerprint = match &root {
                        Ok(root) => workspace::discover(root)
                            .and_then(|old_workspace| {
                                Fingerprinter::new(root, &old_workspace).entry(
                                    requirement,
                                    entry,
                                    board,
                                )
                            })
                            .unwrap_or_else(|why| format!("unavailable: {why}")),
                        Err(why) => format!("unavailable: {why}"),
                    };
                    results.push(RunResult {
                        requirement: requirement.id.clone(),
                        method: method.clone(),
                        level: entry.level,
                        target: entry.target,
                        setup: entry.setup,
                        fingerprint,
                        result: outcome,
                    });
                }
            }
            if results.is_empty() {
                return Err(format!("{test}: no requirement names {procedure}"));
            }
            let relative = source.strip_prefix(&project.repository).unwrap_or(&source);
            Ok(Some(Record {
                schema_version: evidence::RECORD_SCHEMA_VERSION,
                run_id,
                recorded_at_utc: old["completed_at_utc"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                board: board_used,
                revision,
                working_tree: old["candidate"]["working_tree"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned(),
                results,
                note: Some(format!(
                    "imported from {} ({test}, schema {version})",
                    relative.display()
                )),
                supersedes: Vec::new(),
                ruling: None,
                details: Some(old_details(&old).map_err(|why| format!("{test}: {why}"))?),
            }))
        })();
        plans.push(Plan { source, outcome });
    }
    worktrees.remove_all();
    Ok(plans)
}

/// Writes each planned record beside its original's month, never over an
/// existing file.
pub fn write(project: &Project, plans: Vec<Plan>) -> Vec<(PathBuf, Imported)> {
    plans
        .into_iter()
        .map(|plan| {
            let imported = match plan.outcome {
                Err(why) => Imported::Skipped(why),
                Ok(None) => Imported::Skipped("nothing to import".into()),
                Ok(Some(record)) => {
                    let directory = plan
                        .source
                        .parent()
                        .unwrap_or(&project.repository)
                        .to_path_buf();
                    let path = directory.join(format!("{}.json", record.run_id));
                    if path.exists() {
                        Imported::Present(path)
                    } else {
                        match serde_json::to_string_pretty(&record)
                            .map_err(|e| e.to_string())
                            .and_then(|text| {
                                std::fs::write(&path, text + "\n").map_err(|e| e.to_string())
                            }) {
                            Ok(()) => Imported::Written {
                                path,
                                results: record.results.len(),
                            },
                            Err(why) => Imported::Skipped(format!("could not write: {why}")),
                        }
                    }
                }
            };
            (plan.source, imported)
        })
        .collect()
}

/// The old record's account of how the run was done, carried over as is.
fn old_details(old: &Value) -> Result<Details, String> {
    let mut candidate = old["candidate"].clone();
    if let Some(fields) = candidate.as_object_mut() {
        fields.remove("revision");
        fields.remove("working_tree");
    }
    let mut details = serde_json::Map::new();
    for key in [
        "execution",
        "conditions",
        "commands",
        "artifacts",
        "measurements",
        "observations",
        "limitations",
        "stop_conditions_triggered",
    ] {
        if let Some(value) = old.get(key)
            && !value.is_null()
        {
            details.insert(key.to_owned(), value.clone());
        }
    }
    if candidate
        .get("firmware_sha256")
        .is_some_and(|sha| sha.is_string())
    {
        details.insert("candidate".to_owned(), candidate);
    }
    serde_json::from_value(Value::Object(details))
        .map_err(|e| format!("its details do not fit schema 3: {e}"))
}

/// Temporary detached worktrees, one per revision.
struct Worktrees<'a> {
    repository: &'a Path,
    made: BTreeMap<String, Result<PathBuf, String>>,
}

impl<'a> Worktrees<'a> {
    fn new(repository: &'a Path) -> Self {
        Self {
            repository,
            made: BTreeMap::new(),
        }
    }

    fn at(&mut self, revision: &str) -> Result<PathBuf, String> {
        if let Some(made) = self.made.get(revision) {
            return made.clone();
        }
        let made = (|| {
            let known = git(
                self.repository,
                &["cat-file", "-e", &format!("{revision}^{{commit}}")],
            );
            if revision.is_empty() || known.is_err() {
                return Err(format!("revision {revision:.10} is not in this repository"));
            }
            let directory = std::env::temp_dir().join(format!(
                "reqtool-import-{revision:.12}-{}",
                std::process::id()
            ));
            let path = directory.to_string_lossy().into_owned();
            git(
                self.repository,
                &["worktree", "add", "--detach", "--quiet", &path, revision],
            )?;
            Ok(directory)
        })();
        self.made.insert(revision.to_owned(), made.clone());
        made
    }

    fn remove_all(&mut self) {
        for directory in self.made.values().filter_map(|made| made.as_ref().ok()) {
            let path = directory.to_string_lossy().into_owned();
            let _ = git(self.repository, &["worktree", "remove", "--force", &path]);
        }
        let _ = git(self.repository, &["worktree", "prune"]);
    }
}

fn git(repository: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn collect(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && !path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().ends_with("__imported"))
        {
            out.push(path);
        }
    }
}
