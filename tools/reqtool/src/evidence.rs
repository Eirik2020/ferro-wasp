//! Evidence records (schema 3), written only by `reqtool record` and
//! `reqtool test`, and the entry statuses computed from them (ADR-0050).
//!
//! A record is one session: one or more methods run together, with a result
//! and fingerprint for every requirement entry that names each method.
//! Records live beside the old schema 2 run records in
//! `project_meta/testing/evidence/runs/YYYY/MM/`; only schema 3 is read here,
//! and the old validator leaves schema 3 to this tool.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::details::{self, Details};
use crate::fingerprint::Fingerprinter;
use crate::project::Project;
use crate::schema::{Entry, Level, Method, Requirement, Setup, Target};
use crate::status::{Evidence, Status};

pub const RUNS: &str = "project_meta/testing/evidence/runs";
pub const RECORD_SCHEMA_VERSION: u32 = 3;

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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunResult {
    pub requirement: String,
    pub method: Method,
    pub level: Level,
    pub target: Target,
    pub setup: Setup,
    pub fingerprint: String,
    pub result: Outcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
}

/// Every schema 3 record under `runs/`, minus those another record
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

fn collect_json(directory: &Path, out: &mut Vec<PathBuf>) {
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

/// Leaves out passes already on record at the same fingerprint, so re-running
/// unchanged code adds nothing. A fail is always kept.
pub fn drop_repeats(record: &mut Record, existing: &[Record]) {
    record.results.retain(|result| {
        result.result == Outcome::Fail
            || !existing.iter().any(|old| {
                old.board == record.board
                    && old.results.iter().any(|before| {
                        before.result == Outcome::Pass
                            && before.requirement == result.requirement
                            && before.method == result.method
                            && before.level == result.level
                            && before.target == result.target
                            && before.setup == result.setup
                            && before.fingerprint == result.fingerprint
                    })
            })
    });
}

/// One entry's status from its records at one fingerprint:
/// a fail at the current fingerprint is `failed`, whatever passed beside it;
/// otherwise a pass at the current fingerprint is `verified`;
/// otherwise any earlier pass is `stale`; otherwise `unverified`.
pub fn entry_status<'r>(results: impl Iterator<Item = &'r RunResult>, current: &str) -> Status {
    let mut passed_now = false;
    let mut passed_before = false;
    for result in results {
        match (result.fingerprint == current, result.result) {
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

/// Statuses from the records and the current code. Target-level entries are
/// per board; an entry's overall status is the worst board that has
/// evidence, until gates name which boards are required.
pub struct Recorded {
    statuses: BTreeMap<(String, usize), Status>,
    boards: BTreeMap<(String, usize), Vec<(String, Status)>>,
}

impl Recorded {
    pub fn new(project: &Project, records: &[Record]) -> Result<Self, String> {
        let fingerprints = Fingerprinter::new(&project.repository, &project.workspace);
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
                if !entry.level.on_target() {
                    let current = fingerprints.entry(requirement, entry, None)?;
                    let status = entry_status(relevant.iter().map(|(_, result)| *result), &current);
                    statuses.insert(key, status);
                    continue;
                }
                let names: BTreeSet<&str> = relevant
                    .iter()
                    .filter_map(|(record, _)| record.board.as_deref())
                    .collect();
                let mut per_board = Vec::new();
                for board in names {
                    let current = fingerprints.entry(requirement, entry, Some(board))?;
                    let status = entry_status(
                        relevant
                            .iter()
                            .filter(|(record, _)| record.board.as_deref() == Some(board))
                            .map(|(_, result)| *result),
                        &current,
                    );
                    per_board.push((board.to_owned(), status));
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
    project: &Project,
    request: &RunRequest<'_>,
    now: Timestamp,
) -> Result<Record, String> {
    if request.supersedes.is_empty() != request.ruling.is_none() {
        return Err(
            "superseding a run needs a ruling, and a ruling needs a run to supersede".into(),
        );
    }
    let fingerprints = Fingerprinter::new(&project.repository, &project.workspace);
    let mut results = Vec::new();
    let mut named = BTreeSet::new();
    if request.runs.is_empty() {
        return Err("nothing ran".into());
    }
    for (method, method_outcome) in &request.runs {
        let before = results.len();
        for requirement in project.requirements.values() {
            for entry in &requirement.verification.entries {
                if &entry.method != method {
                    continue;
                }
                let board = if entry.level.on_target() {
                    Some(request.board.ok_or_else(|| {
                        format!(
                            "{} has {} evidence, which needs --board",
                            requirement.id,
                            entry.level.name()
                        )
                    })?)
                } else {
                    None
                };
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
                    fingerprint: fingerprints.entry(requirement, entry, board)?,
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
        results,
        note: request.note.map(str::to_owned),
        supersedes: request.supersedes.to_vec(),
        ruling: request.ruling.map(str::to_owned),
        details: sealed,
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

    fn result(fingerprint: &str, result: Outcome) -> RunResult {
        RunResult {
            requirement: "A-001".into(),
            method: Method::Test("a::tests::t".into()),
            level: Level::Unit,
            target: Target::Host,
            setup: Setup::None,
            fingerprint: fingerprint.into(),
            result,
        }
    }

    #[test]
    fn a_fail_at_the_current_fingerprint_is_never_outvoted() {
        let runs = [
            result("now", Outcome::Pass),
            result("now", Outcome::Fail),
            result("now", Outcome::Pass),
        ];
        assert_eq!(entry_status(runs.iter(), "now"), Status::Failed);
    }

    #[test]
    fn a_fail_is_cleared_by_a_pass_once_the_code_changed() {
        let runs = [
            result("before", Outcome::Fail),
            result("now", Outcome::Pass),
        ];
        assert_eq!(entry_status(runs.iter(), "now"), Status::Verified);
    }

    #[test]
    fn an_old_pass_is_stale_and_an_old_fail_alone_is_unverified() {
        assert_eq!(
            entry_status([result("before", Outcome::Pass)].iter(), "now"),
            Status::Stale
        );
        assert_eq!(
            entry_status([result("before", Outcome::Fail)].iter(), "now"),
            Status::Unverified
        );
        assert_eq!(entry_status([].iter(), "now"), Status::Unverified);
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
