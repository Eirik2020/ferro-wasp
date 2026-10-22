//! One-off: derive a schema 4 record from every schema 3 record, beside it
//! as `<name>__retraced.json`, leaving the original untouched.
//!
//! - Each result's claim is computed from the requirement text and the
//!   procedure at the commit that added the record, when the claim was made.
//! - Host evidence takes its identities from the code at the record's own
//!   revision: a test's file and its direct imports, or for a procedure, the
//!   files of the items its requirement was allocated to.
//! - Board evidence takes its configuration from `cargo check` at that
//!   revision, with the features the record's details name.
//!
//! Whatever cannot be derived is listed as a limitation, and a result without
//! identities never counts as current.
//!
//! `cargo run --release --example retrace -- <repository> <scratch directory>`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use reqtool::configuration::{Cargo, Configurations};
use reqtool::evidence::{RECORD_SCHEMA_VERSION, Record, Retraced, RunResult};
use reqtool::identity;
use reqtool::items::{Kind, Resolver};
use reqtool::schema::{Entry, Method, Requirement, Verification};
use reqtool::trace::{Import, Trace, relative};
use reqtool::workspace;
use serde::Deserialize;

#[derive(Deserialize)]
struct OldRecord {
    run_id: String,
    recorded_at_utc: String,
    board: Option<String>,
    revision: String,
    working_tree: String,
    results: Vec<OldResult>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    supersedes: Vec<String>,
    #[serde(default)]
    ruling: Option<String>,
    #[serde(default)]
    details: Option<reqtool::details::Details>,
}

#[derive(Deserialize)]
struct OldResult {
    requirement: String,
    method: Method,
    level: reqtool::schema::Level,
    target: reqtool::schema::Target,
    setup: reqtool::schema::Setup,
    result: reqtool::evidence::Outcome,
}

#[derive(Deserialize)]
struct OldFile {
    requirements: Vec<OldRequirement>,
}

#[derive(Deserialize)]
struct OldRequirement {
    id: String,
    statement: String,
    #[serde(default)]
    allocated_to: Vec<String>,
    verification: OldVerification,
}

#[derive(Deserialize)]
struct OldVerification {
    #[serde(default)]
    entries: Vec<Entry>,
}

fn git(repository: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A detached worktree at `revision`, made once.
fn worktree(repository: &Path, scratch: &Path, revision: &str) -> Option<PathBuf> {
    let path = scratch.join(format!("wt-{revision}"));
    if path.is_dir() {
        return Some(path);
    }
    git(
        repository,
        &["worktree", "add", "--detach", path.to_str()?, revision],
    )?;
    Some(path)
}

/// Requirements as they stood in a tree, by ID, with their allocations.
fn requirements_at(tree: &Path) -> BTreeMap<String, (Requirement, Vec<String>)> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(tree.join("project_meta/requirements")) else {
        return out;
    };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if path.extension().is_none_or(|extension| extension != "yaml")
            || path.ends_with("gates.yaml")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(file) = serde_saphyr::from_str::<OldFile>(&text) else {
            continue;
        };
        for old in file.requirements {
            let requirement = Requirement {
                id: old.id.clone(),
                title: String::new(),
                statement: old.statement,
                rationale: String::new(),
                derived_from: Vec::new(),
                verification: Verification {
                    entries: old.verification.entries,
                },
            };
            out.insert(old.id, (requirement, old.allocated_to));
        }
    }
    out
}

/// The files an allocation covers: an item's file, or a module's file and
/// every file below it.
fn allocation_files(
    tree: &Path,
    trace: &Trace,
    resolver: &Resolver<'_>,
    path: &str,
) -> Vec<String> {
    let Ok(resolved) = resolver.resolve(path) else {
        return Vec::new();
    };
    let file = relative(tree, &resolved.file);
    let mut files = vec![file.clone()];
    if resolved.kind == Kind::Module {
        let directory = if !path.contains("::") || file.ends_with("/mod.rs") {
            Path::new(&file)
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf()
        } else {
            Path::new(&file).with_extension("")
        };
        files.extend(
            trace
                .files
                .keys()
                .filter(|candidate| Path::new(candidate).starts_with(&directory))
                .cloned(),
        );
    }
    files.sort();
    files.dedup();
    files
}

fn main() {
    let mut args = std::env::args().skip(1);
    let repository = PathBuf::from(args.next().expect("repository"));
    let scratch = PathBuf::from(args.next().expect("scratch directory"));
    std::fs::create_dir_all(&scratch).unwrap();
    let mut files = Vec::new();
    collect(&repository.join(reqtool::evidence::RUNS), &mut files);
    files.sort();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        if value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(3)
        {
            continue;
        }
        let old: OldRecord = serde_json::from_value(value).unwrap();
        let from = relative(&repository, &file);
        let mut limitations = Vec::new();

        let added = git(
            &repository,
            &["log", "--diff-filter=A", "--format=%H", "-1", "--", &from],
        )
        .filter(|commit| !commit.is_empty());
        let claims_tree = added
            .as_deref()
            .and_then(|commit| worktree(&repository, &scratch, commit));
        let claimed = claims_tree
            .as_deref()
            .map(requirements_at)
            .unwrap_or_default();
        if claims_tree.is_none() {
            limitations
                .push("the commit that added the record is unknown: no claims derived".into());
        }

        let code_tree = git(&repository, &["cat-file", "-t", &old.revision])
            .filter(|kind| kind == "commit")
            .and_then(|_| worktree(&repository, &scratch, &old.revision));
        if code_tree.is_none() {
            limitations.push(format!(
                "revision {} is not in this repository: no identities derived",
                old.revision
            ));
        }
        let code = code_tree.as_ref().and_then(|tree| {
            let workspace = workspace::discover(tree).ok()?;
            let trace = Trace::build(tree, &workspace).ok()?;
            Some((tree.clone(), workspace, trace))
        });
        if code_tree.is_some() && code.is_none() {
            limitations.push("the code at the revision could not be traced".into());
        }

        let mut identities = BTreeMap::new();
        let mut configuration = None;
        let mut results = Vec::new();
        for result in &old.results {
            let entry = Entry {
                level: result.level,
                target: result.target,
                setup: result.setup,
                method: result.method.clone(),
            };
            let claim = match (claims_tree.as_deref(), claimed.get(&result.requirement)) {
                (Some(tree), Some((requirement, _))) => identity::claim(tree, requirement, &entry)
                    .unwrap_or_else(|why| format!("unavailable: {why}")),
                _ => format!(
                    "unavailable: {} had no requirement text when recorded",
                    result.requirement
                ),
            };
            let mut exercised = BTreeMap::new();
            let mut toolchain = None;
            if let Some((tree, workspace, trace)) = &code {
                if result.level.on_target() {
                    if configuration.is_none() {
                        configuration =
                            board_configuration(tree, workspace, &old, &mut limitations);
                    }
                } else {
                    let resolver = Resolver::new(&workspace.crates);
                    let modules: Vec<String> = match &result.method {
                        Method::Test(path) => match resolver.resolve(path) {
                            Ok(resolved) => {
                                let file = relative(tree, &resolved.file);
                                let mut modules = vec![file.clone()];
                                if let Some(source) = trace.files.get(&file) {
                                    modules.extend(
                                        source
                                            .imports
                                            .iter()
                                            .filter(|(_, import)| **import == Import::File)
                                            .map(|(key, _)| key.clone()),
                                    );
                                }
                                modules
                            }
                            Err(why) => {
                                limitations.push(format!("{path} did not resolve: {why}"));
                                Vec::new()
                            }
                        },
                        Method::Procedure(_) => {
                            let at_revision = requirements_at(tree);
                            at_revision
                                .get(&result.requirement)
                                .map(|(_, allocated)| {
                                    allocated
                                        .iter()
                                        .flat_map(|path| {
                                            allocation_files(tree, trace, &resolver, path)
                                        })
                                        .collect()
                                })
                                .unwrap_or_default()
                        }
                    };
                    if modules.is_empty() {
                        limitations.push(format!(
                            "{} {}: no code to cover was found at the revision",
                            result.requirement,
                            result.level.name()
                        ));
                    }
                    for module in modules {
                        let imports = trace.imports(&module);
                        for key in std::iter::once(&module).chain(&imports) {
                            if let Some(identity) = trace.identity(key) {
                                identities.insert(key.clone(), identity);
                            }
                        }
                        exercised.insert(module, imports);
                    }
                    let directory = match &result.method {
                        Method::Test(path) => workspace
                            .crates
                            .get(path.split("::").next().unwrap_or(path))
                            .and_then(|found| found.lockfile.parent())
                            .map_or_else(|| tree.clone(), Path::to_path_buf),
                        Method::Procedure(_) => tree.clone(),
                    };
                    toolchain = identity::toolchain(tree, &directory).ok();
                }
            }
            results.push(RunResult {
                requirement: result.requirement.clone(),
                method: result.method.clone(),
                level: result.level,
                target: result.target,
                setup: result.setup,
                claim,
                toolchain,
                exercised,
                result: result.result,
            });
        }
        limitations.sort();
        limitations.dedup();
        let record = Record {
            schema_version: RECORD_SCHEMA_VERSION,
            run_id: old.run_id,
            recorded_at_utc: old.recorded_at_utc,
            board: old.board,
            revision: old.revision,
            working_tree: old.working_tree,
            configuration,
            identities,
            results,
            note: old.note,
            supersedes: old.supersedes,
            ruling: old.ruling,
            details: old.details,
            retraced: Some(Retraced { from, limitations }),
        };
        let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
        let target = file.with_file_name(format!("{stem}__retraced.json"));
        let text = serde_json::to_string_pretty(&record).unwrap() + "\n";
        std::fs::write(&target, text).unwrap();
        println!(
            "{} ({} limitation(s))",
            relative(&repository, &target),
            record.retraced.as_ref().map_or(0, |r| r.limitations.len())
        );
    }
}

/// The board's configuration at the revision, from `cargo check` there.
fn board_configuration(
    tree: &Path,
    workspace: &workspace::Workspace,
    old: &OldRecord,
    limitations: &mut Vec<String>,
) -> Option<reqtool::configuration::Configuration> {
    let board = old.board.as_deref()?;
    if !workspace.boards.contains_key(board) {
        limitations.push(format!(
            "no firmware app for {board} at the revision: no configuration derived"
        ));
        return None;
    }
    let features: Vec<String> = old
        .details
        .as_ref()
        .and_then(|details| details.candidate.as_ref())
        .map(|candidate| candidate.features.clone())
        .unwrap_or_default();
    match Cargo::new(tree, workspace).configuration(board, &features) {
        Ok(configuration) => Some(configuration.as_ref().clone()),
        Err(why) => {
            limitations.push(format!(
                "no configuration derived: {}",
                why.lines().next().unwrap_or("")
            ));
            None
        }
    }
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
        {
            out.push(path);
        }
    }
}
