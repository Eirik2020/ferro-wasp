//! Evidence records (schema 4), written only by `reqtool record` and
//! `reqtool test`, and the entry statuses computed from them (design
//! section 20).
//!
//! A record is one session: one or more methods run together, with a result
//! for every requirement entry that names each method. Each result carries
//! the claim it proved. Host evidence names the modules it exercised (the
//! test's file and the files it imports directly), each with its direct
//! imports, and the record holds the identity of every one of those. Board
//! evidence carries the whole configuration it ran in.
//!
//! A host result counts for a module while the claim, the toolchain, the
//! module, its direct imports and their identities are all as recorded. A
//! board result counts for a configuration whose identity is the one
//! recorded. Records of earlier schemas are left beside these untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::configuration::{Configuration, Configurations};
use crate::details::{self, Details};
use crate::identity;
use crate::items::Resolver;
use crate::project::Project;
use crate::schema::{Entry, Level, Method, Requirement, Setup, Target};
use crate::status::{Evidence, Status};
use crate::trace::{Import, Trace};

pub const RUNS: &str = "project_meta/testing/evidence/runs";
pub const RECORD_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub run_id: String,
    pub recorded_at_utc: String,
    /// The board's firmware app, for target-level evidence.
    pub board: Option<String>,
    pub revision: String,
    /// `clean` or `dirty`: uncommitted changes to tracked files.
    pub working_tree: String,
    /// What board evidence ran in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<Configuration>,
    /// For host evidence: the direct imports of every module a result
    /// exercised.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, Vec<String>>,
    /// For host evidence: the identity of every module a result exercised
    /// and of everything those import, by import key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub identities: BTreeMap<String, String>,
    pub results: Vec<RunResult>,
    #[serde(default)]
    pub note: Option<String>,
    /// Earlier runs this one supersedes, on the user's explicit ruling.
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default)]
    pub ruling: Option<String>,
    /// How the run was done; required for board evidence ([`details::check`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Details>,
    /// For a record derived from an earlier schema's: where from, and what
    /// could not be derived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retraced: Option<Retraced>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Retraced {
    pub from: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunResult {
    pub requirement: String,
    pub method: Method,
    pub level: Level,
    pub target: Target,
    pub setup: Setup,
    /// What the entry claimed when it ran ([`identity::claim`]).
    pub claim: String,
    /// For host evidence: the toolchain it ran on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<String>,
    /// For host evidence: each module exercised; the record holds their
    /// imports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exercised: Vec<String>,
    pub result: Outcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
}

/// Every schema 4 record under `runs/`, minus those another record
/// supersedes.
pub fn load(repository: &Path) -> Result<Vec<Record>, String> {
    let mut files = Vec::new();
    collect_json(&repository.join(RUNS), &mut files);
    files.sort();
    let mut records = Vec::new();
    for file in files {
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        if value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::from(RECORD_SCHEMA_VERSION))
        {
            continue;
        }
        let record: Record =
            serde_json::from_value(value).map_err(|e| format!("{}: {e}", file.display()))?;
        details::check(&record.results, record.details.as_ref())
            .map_err(|why| format!("{}: {why}", file.display()))?;
        if let Some(details) = &record.details {
            details
                .verify_artifacts(repository)
                .map_err(|why| format!("{}: {why}", file.display()))?;
        }
        records.push(record);
    }
    let superseded: BTreeSet<String> = records
        .iter()
        .flat_map(|record| record.supersedes.iter().cloned())
        .collect();
    records.retain(|record| !superseded.contains(&record.run_id));
    Ok(records)
}

pub(crate) fn collect_json(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_json(&path, out);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            out.push(path);
        }
    }
}

fn same_entry(result: &RunResult, requirement: &Requirement, entry: &Entry) -> bool {
    result.requirement == requirement.id
        && result.method == entry.method
        && result.level == entry.level
        && result.target == entry.target
        && result.setup == entry.setup
}

/// Leaves out passes already on record with the same coverage, so re-running
/// unchanged code adds nothing. A fail is always kept.
pub fn drop_repeats(record: &mut Record, existing: &[Record]) {
    let new = record.clone();
    record.results.retain(|result| {
        result.result == Outcome::Fail
            || !existing.iter().any(|old| {
                old.board == new.board
                    && old.results.iter().any(|before| {
                        before.result == Outcome::Pass
                            && before.requirement == result.requirement
                            && before.method == result.method
                            && before.level == result.level
                            && before.target == result.target
                            && before.setup == result.setup
                            && same_coverage(old, before, &new, result)
                    })
            })
    });
}

fn same_coverage(old: &Record, before: &RunResult, new: &Record, after: &RunResult) -> bool {
    if before.claim != after.claim
        || before.toolchain != after.toolchain
        || before.exercised != after.exercised
    {
        return false;
    }
    if after.level.on_target() {
        return old.configuration.as_ref().map(Configuration::identity)
            == new.configuration.as_ref().map(Configuration::identity);
    }
    after.exercised.iter().all(|module| {
        old.modules.get(module) == new.modules.get(module)
            && std::iter::once(module)
                .chain(new.modules.get(module).into_iter().flatten())
                .all(|key| old.identities.get(key) == new.identities.get(key))
    })
}

/// One entry's status from whether each relevant result still counts:
/// a fail that counts is `failed`, whatever passed beside it;
/// otherwise a pass that counts is `verified`;
/// otherwise any earlier pass is `stale`; otherwise `unverified`.
pub fn entry_status(results: impl Iterator<Item = (bool, Outcome)>) -> Status {
    let mut passed_now = false;
    let mut passed_before = false;
    for (counts, outcome) in results {
        match (counts, outcome) {
            (true, Outcome::Fail) => return Status::Failed,
            (true, Outcome::Pass) => passed_now = true,
            (false, Outcome::Pass) => passed_before = true,
            (false, Outcome::Fail) => {}
        }
    }
    if passed_now {
        Status::Verified
    } else if passed_before {
        Status::Stale
    } else {
        Status::Unverified
    }
}

/// The current state of the code that results are judged against.
pub struct Current<'a> {
    pub project: &'a Project,
    pub trace: &'a Trace,
    pub configurations: &'a dyn Configurations,
}

impl Current<'_> {
    fn claim(&self, requirement: &Requirement, entry: &Entry) -> Result<String, String> {
        identity::claim(&self.project.repository, requirement, entry)
    }

    /// The toolchain host evidence for a method runs on: the test crate's
    /// workspace's, or the repository's for a procedure.
    fn host_toolchain(&self, method: &Method) -> Result<String, String> {
        let directory = match method {
            Method::Test(path) => {
                let crate_name = path.split("::").next().unwrap_or(path);
                self.project
                    .workspace
                    .crates
                    .get(crate_name)
                    .and_then(|found| found.lockfile.parent())
                    .map_or_else(|| self.project.repository.clone(), Path::to_path_buf)
            }
            Method::Procedure(_) => self.project.repository.clone(),
        };
        identity::toolchain(&self.project.repository, &directory)
    }

    /// The file a test is defined in.
    fn test_file(&self, path: &str) -> Result<String, String> {
        let resolved = Resolver::new(&self.project.workspace.crates).resolve(path)?;
        self.trace
            .locate(&self.project.repository, &resolved.file)
            .ok_or_else(|| format!("{path}: its file is in no module tree"))
    }

    /// The modules a host run of this entry exercises.
    fn exercised(&self, requirement: &Requirement, entry: &Entry) -> Result<Vec<String>, String> {
        Ok(match &entry.method {
            Method::Test(path) => {
                let file = self.test_file(path)?;
                let mut modules = vec![file.clone()];
                if let Some(source) = self.trace.files.get(&file) {
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
            Method::Procedure(_) => self
                .trace
                .declaring(&requirement.id)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        })
    }

    /// Whether a host result still counts for `module`; with no module, for
    /// a requirement no file declares, whether its claim and every module
    /// it exercised are unchanged.
    fn host_counts(
        &self,
        record: &Record,
        result: &RunResult,
        claim: &str,
        toolchain: &str,
        module: Option<&str>,
    ) -> bool {
        if result.claim != claim || result.toolchain.as_deref() != Some(toolchain) {
            return false;
        }
        let unchanged = |module: &str| {
            let imports = self.trace.imports(module);
            result.exercised.iter().any(|exercised| exercised == module)
                && record.modules.get(module) == Some(&imports)
                && std::iter::once(module)
                    .chain(imports.iter().map(String::as_str))
                    .all(|key| {
                        record.identities.contains_key(key)
                            && record.identities.get(key).cloned() == self.trace.identity(key)
                    })
        };
        match module {
            Some(module) => unchanged(module),
            // A run that exercised nothing proves nothing.
            None => {
                !result.exercised.is_empty()
                    && result.exercised.iter().all(|module| unchanged(module))
            }
        }
    }
}

/// Statuses from the records and the current code.
///
/// A host entry takes the worst of the modules that declare its requirement,
/// or, when no module declares it, of what its runs exercised. A pass counts
/// for a module only if it exercised that module. A board entry
/// is per board, and its overall status is the worst board with evidence,
/// until gates name the boards they require. With a configuration in scope,
/// only the modules it compiles in count, and only board evidence from that
/// exact configuration.
pub struct Recorded {
    statuses: BTreeMap<(String, usize), Status>,
    boards: BTreeMap<(String, usize), Vec<(String, Status)>>,
}

impl Recorded {
    pub fn new(
        current: &Current<'_>,
        records: &[Record],
        scope: Option<&Configuration>,
    ) -> Result<Self, String> {
        let project = current.project;
        let mut statuses = BTreeMap::new();
        let mut boards = BTreeMap::new();
        for requirement in project.requirements.values() {
            for (index, entry) in requirement.verification.entries.iter().enumerate() {
                let key = (requirement.id.clone(), index);
                let relevant: Vec<(&Record, &RunResult)> = records
                    .iter()
                    .flat_map(|record| record.results.iter().map(move |result| (record, result)))
                    .filter(|(_, result)| same_entry(result, requirement, entry))
                    .collect();
                let claim = current.claim(requirement, entry)?;
                if !entry.level.on_target() {
                    let toolchain = current.host_toolchain(&entry.method)?;
                    let mut modules: Vec<Option<String>> = current
                        .trace
                        .declaring(&requirement.id)
                        .into_iter()
                        .filter(|module| {
                            scope.is_none_or(|configuration| {
                                configuration.files.contains_key(*module)
                            })
                        })
                        .map(|module| Some(module.to_owned()))
                        .collect();
                    if modules.is_empty() {
                        modules.push(None);
                    }
                    // A pass that does not count for a module, because it
                    // exercised other code or the code changed, leaves it
                    // stale: it passed before, and needs running again.
                    let status = modules
                        .iter()
                        .map(|module| {
                            entry_status(relevant.iter().map(|(record, result)| {
                                let counts = current.host_counts(
                                    record,
                                    result,
                                    &claim,
                                    &toolchain,
                                    module.as_deref(),
                                );
                                (counts, result.result)
                            }))
                        })
                        .max()
                        .unwrap_or(Status::Unverified);
                    statuses.insert(key, status);
                    continue;
                }
                let names: BTreeSet<&str> = relevant
                    .iter()
                    .filter_map(|(record, _)| record.board.as_deref())
                    .filter(|board| scope.is_none_or(|configuration| configuration.board == *board))
                    .collect();
                let mut per_board = Vec::new();
                for board in names {
                    let mut judged = Vec::new();
                    for (record, result) in relevant
                        .iter()
                        .filter(|(record, _)| record.board.as_deref() == Some(board))
                    {
                        let counts = result.claim == claim
                            && match &record.configuration {
                                None => false,
                                Some(ran) => {
                                    let now: Rc<Configuration> = match scope {
                                        Some(configuration) => Rc::new(configuration.clone()),
                                        None => current
                                            .configurations
                                            .configuration(board, &ran.features)?,
                                    };
                                    ran.identity() == now.identity()
                                }
                            };
                        judged.push((counts, result.result));
                    }
                    per_board.push((board.to_owned(), entry_status(judged.into_iter())));
                }
                let overall = per_board
                    .iter()
                    .map(|(_, status)| *status)
                    .max()
                    .unwrap_or(Status::Unverified);
                statuses.insert(key.clone(), overall);
                boards.insert(key, per_board);
            }
        }
        Ok(Self { statuses, boards })
    }
}

impl Evidence for Recorded {
    fn entry(&self, requirement: &Requirement, entry: usize) -> Status {
        self.statuses
            .get(&(requirement.id.clone(), entry))
            .copied()
            .unwrap_or(Status::Unverified)
    }

    fn boards(&self, requirement: &Requirement, entry: usize) -> Vec<(String, Status)> {
        self.boards
            .get(&(requirement.id.clone(), entry))
            .cloned()
            .unwrap_or_default()
    }
}

/// What one session covers: each method that ran, with its result.
pub struct RunRequest<'a> {
    pub runs: Vec<(Method, Outcome)>,
    pub board: Option<&'a str>,
    /// For board evidence: the features the image was built with, beyond its
    /// defaults.
    pub features: &'a [String],
    /// Requirements that failed although their method as a whole passed.
    pub failed: &'a [String],
    pub note: Option<&'a str>,
    pub supersedes: &'a [String],
    pub ruling: Option<&'a str>,
    /// How the run was done; its artifacts are hashed here.
    pub details: Option<Details>,
}

/// Builds the record for one session: a result for every entry, in every
/// requirement, that names one of the methods that ran.
pub fn build(
    current: &Current<'_>,
    request: &RunRequest<'_>,
    now: Timestamp,
) -> Result<Record, String> {
    let project = current.project;
    if request.supersedes.is_empty() != request.ruling.is_none() {
        return Err(
            "superseding a run needs a ruling, and a ruling needs a run to supersede".into(),
        );
    }
    if request.runs.is_empty() {
        return Err("nothing ran".into());
    }
    let mut results = Vec::new();
    let mut modules = BTreeMap::new();
    let mut identities = BTreeMap::new();
    let mut configuration = None;
    let mut named = BTreeSet::new();
    for (method, method_outcome) in &request.runs {
        let before = results.len();
        for requirement in project.requirements.values() {
            for entry in &requirement.verification.entries {
                if &entry.method != method {
                    continue;
                }
                let mut exercised = Vec::new();
                let mut toolchain = None;
                if entry.level.on_target() {
                    let board = request.board.ok_or_else(|| {
                        format!(
                            "{} has {} evidence, which needs --board",
                            requirement.id,
                            entry.level.name()
                        )
                    })?;
                    if configuration.is_none() {
                        configuration = Some(
                            current
                                .configurations
                                .configuration(board, request.features)?
                                .as_ref()
                                .clone(),
                        );
                    }
                } else {
                    toolchain = Some(current.host_toolchain(method)?);
                    for module in current.exercised(requirement, entry)? {
                        let imports = current.trace.imports(&module);
                        for key in std::iter::once(&module).chain(&imports) {
                            if let Some(identity) = current.trace.identity(key) {
                                identities.insert(key.clone(), identity);
                            }
                        }
                        exercised.push(module.clone());
                        modules.insert(module, imports);
                    }
                    exercised.sort();
                    exercised.dedup();
                }
                named.insert(requirement.id.as_str());
                let outcome = if request.failed.contains(&requirement.id) {
                    Outcome::Fail
                } else {
                    *method_outcome
                };
                results.push(RunResult {
                    requirement: requirement.id.clone(),
                    method: method.clone(),
                    level: entry.level,
                    target: entry.target,
                    setup: entry.setup,
                    claim: current.claim(requirement, entry)?,
                    toolchain,
                    exercised,
                    result: outcome,
                });
            }
        }
        if results.len() == before {
            return Err(format!("no requirement names {}", describe(method)));
        }
    }
    for id in request.failed {
        if !named.contains(id.as_str()) {
            return Err(format!("{id} does not name this method"));
        }
    }
    let host_only = results.iter().all(|result| !result.level.on_target());
    if host_only && request.board.is_some() {
        return Err("--board applies only to mcu, bench and flight evidence".into());
    }
    if host_only && !request.features.is_empty() {
        return Err("--features applies only to mcu, bench and flight evidence".into());
    }
    let mut sealed = request.details.clone();
    if let Some(details) = &mut sealed {
        details.seal_artifacts(&project.repository)?;
    }
    details::check(&results, sealed.as_ref())?;
    let (revision, working_tree) = git_state(&project.repository)?;
    let scope = request.board.unwrap_or("host");
    let slug = match request.runs.as_slice() {
        [(method, _)] => method_slug(method),
        runs => format!("{}-methods", runs.len()),
    };
    let run_id = next_run_id(&project.repository, &now, &slug, scope);
    Ok(Record {
        schema_version: RECORD_SCHEMA_VERSION,
        run_id,
        recorded_at_utc: now.iso(),
        board: request.board.map(str::to_owned),
        revision,
        working_tree,
        configuration,
        modules,
        identities,
        results,
        note: request.note.map(str::to_owned),
        supersedes: request.supersedes.to_vec(),
        ruling: request.ruling.map(str::to_owned),
        details: sealed,
        retraced: None,
    })
}

/// Writes a record under `runs/YYYY/MM/`, never over an existing file.
pub fn write(repository: &Path, record: &Record, now: &Timestamp) -> Result<PathBuf, String> {
    let directory = repository
        .join(RUNS)
        .join(format!("{:04}", now.year))
        .join(format!("{:02}", now.month));
    std::fs::create_dir_all(&directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    let path = directory.join(format!("{}.json", record.run_id));
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())? + "\n";
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::Write::write_all(&mut file, text.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn git_state(repository: &Path) -> Result<(String, String), String> {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(repository)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let revision =
        git(&["rev-parse", "HEAD"]).ok_or("not a git checkout: the revision is unknown")?;
    let status = git(&["status", "--porcelain", "--untracked-files=no"]).unwrap_or_default();
    Ok((
        revision,
        if status.is_empty() { "clean" } else { "dirty" }.to_owned(),
    ))
}

fn describe(method: &Method) -> String {
    match method {
        Method::Test(path) => format!("test {path}"),
        Method::Procedure(reference) => format!("procedure {reference}"),
    }
}

fn method_slug(method: &Method) -> String {
    let raw = match method {
        Method::Test(path) => path.rsplit("::").next().unwrap_or(path).to_owned(),
        Method::Procedure(reference) => {
            reference.rsplit('#').next().unwrap_or(reference).to_owned()
        }
    };
    let slug: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    slug.chars().take(48).collect()
}

fn next_run_id(repository: &Path, now: &Timestamp, slug: &str, scope: &str) -> String {
    let directory = repository
        .join(RUNS)
        .join(format!("{:04}", now.year))
        .join(format!("{:02}", now.month));
    (1..)
        .map(|n| format!("{}__{slug}__{scope}__{n:02}", now.compact()))
        .find(|id| !directory.join(format!("{id}.json")).exists())
        .unwrap_or_default()
}

/// A UTC time, without a date library.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timestamp {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Timestamp {
    pub fn now() -> Self {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        Self::from_unix(seconds)
    }

    pub fn from_unix(seconds: u64) -> Self {
        let days = (seconds / 86_400) as i64;
        let rest = seconds % 86_400;
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self {
            year,
            month,
            day,
            hour: (rest / 3_600) as u32,
            minute: (rest % 3_600 / 60) as u32,
            second: (rest % 60) as u32,
        }
    }

    pub fn iso(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    fn compact(&self) -> String {
        format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fail_that_counts_is_never_outvoted() {
        let runs = [
            (true, Outcome::Pass),
            (true, Outcome::Fail),
            (true, Outcome::Pass),
        ];
        assert_eq!(entry_status(runs.into_iter()), Status::Failed);
    }

    #[test]
    fn a_fail_is_cleared_by_a_pass_once_the_code_changed() {
        let runs = [(false, Outcome::Fail), (true, Outcome::Pass)];
        assert_eq!(entry_status(runs.into_iter()), Status::Verified);
    }

    #[test]
    fn an_old_pass_is_stale_and_an_old_fail_alone_is_unverified() {
        assert_eq!(
            entry_status([(false, Outcome::Pass)].into_iter()),
            Status::Stale
        );
        assert_eq!(
            entry_status([(false, Outcome::Fail)].into_iter()),
            Status::Unverified
        );
        assert_eq!(entry_status([].into_iter()), Status::Unverified);
    }

    #[test]
    fn timestamps_convert_from_unix_seconds() {
        assert_eq!(Timestamp::from_unix(0).iso(), "1970-01-01T00:00:00Z");
        assert_eq!(
            Timestamp::from_unix(951_782_400).iso(),
            "2000-02-29T00:00:00Z"
        );
        assert_eq!(
            Timestamp::from_unix(1_792_195_200).iso(),
            "2026-10-17T00:00:00Z"
        );
    }
}
