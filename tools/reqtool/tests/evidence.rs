//! Recording evidence and reading status back, in a throwaway git copy of the
//! fixture repository.

use std::path::Path;
use std::process::Command;

use reqtool::evidence::{self, Outcome, Recorded, RunRequest, Timestamp};
use reqtool::fingerprint::Fingerprinter;
use reqtool::project::Project;
use reqtool::schema::Method;
use reqtool::status::{self, Status};

const TEST: &str = "alpha::input::tests::rejects_stale";
const BENCH: &str = "project_meta/testing/procedures.md#Bench Gate";

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn git(repository: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=reqtool",
            "-c",
            "user.email=reqtool@example.invalid",
        ])
        .args(args)
        .current_dir(repository)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

fn repository() -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/repo"),
        copy.path(),
    );
    git(copy.path(), &["init", "-q"]);
    git(copy.path(), &["add", "-A"]);
    git(copy.path(), &["commit", "-q", "-m", "fixture"]);
    copy
}

fn load(repository: &Path) -> Project {
    let (project, problems) = Project::load(repository).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    project
}

fn request<'a>(method: Method, board: Option<&'a str>, outcome: Outcome) -> RunRequest<'a> {
    RunRequest {
        runs: vec![(method, outcome)],
        board,
        failed: &[],
        note: None,
        supersedes: &[],
        ruling: None,
    }
}

/// Records a run; each call gets its own second so run IDs and order are
/// deterministic.
fn record(repository: &Path, request: &RunRequest<'_>, second: u64) -> String {
    let project = load(repository);
    let now = Timestamp::from_unix(1_792_195_200 + second);
    let built = evidence::build(&project, request, now).unwrap();
    evidence::write(repository, &built, &now).unwrap();
    built.run_id
}

fn entry_status(repository: &Path, id: &str, entry: usize) -> Status {
    let project = load(repository);
    let records = evidence::load(repository).unwrap();
    let recorded = Recorded::new(&project, &records).unwrap();
    use reqtool::status::Evidence;
    recorded.entry(&project.requirements[id], entry)
}

fn requirement_status(repository: &Path, id: &str) -> Status {
    let project = load(repository);
    let records = evidence::load(repository).unwrap();
    let recorded = Recorded::new(&project, &records).unwrap();
    status::compute(&project.requirements, &recorded)[id]
}

fn edit(repository: &Path, file: &str, from: &str, to: &str) {
    let path = repository.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{from} in {file}");
    std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
}

#[test]
fn a_pass_verifies_until_the_covered_code_changes() {
    let repo = repository();
    let path = repo.path();
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Unverified);
    record(
        path,
        &request(Method::Test(TEST.into()), None, Outcome::Pass),
        1,
    );
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Verified);

    edit(
        path,
        "crates/beta/src/lib.rs",
        "pub fn plain() {}",
        "pub fn plain() { let _ = 1; }",
    );
    assert_eq!(
        entry_status(path, "INPUT-001", 0),
        Status::Verified,
        "beta is outside the closure"
    );

    edit(
        path,
        "crates/alpha/src/hidden.rs",
        "pub struct Original;",
        "pub struct Original(u8);",
    );
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Stale);
}

#[test]
fn a_fail_stands_until_the_code_changes_or_a_ruling_supersedes_it() {
    let repo = repository();
    let path = repo.path();
    let failed = record(
        path,
        &request(Method::Test(TEST.into()), None, Outcome::Fail),
        1,
    );
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Failed);

    record(
        path,
        &request(Method::Test(TEST.into()), None, Outcome::Pass),
        2,
    );
    assert_eq!(
        entry_status(path, "INPUT-001", 0),
        Status::Failed,
        "a re-run pass does not outvote"
    );

    let supersedes = [failed];
    let ruling = RunRequest {
        supersedes: &supersedes,
        ruling: Some("the fail was a harness fault"),
        ..request(Method::Test(TEST.into()), None, Outcome::Pass)
    };
    record(path, &ruling, 3);
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Verified);

    let other = repository();
    record(
        other.path(),
        &request(Method::Test(TEST.into()), None, Outcome::Fail),
        1,
    );
    edit(
        other.path(),
        "crates/alpha/src/input.rs",
        "pub fn validate() {}",
        "pub fn validate() { let _ = 2; }",
    );
    record(
        other.path(),
        &request(Method::Test(TEST.into()), None, Outcome::Pass),
        2,
    );
    assert_eq!(
        entry_status(other.path(), "INPUT-001", 0),
        Status::Verified,
        "changed code clears the fail"
    );
}

#[test]
fn one_run_can_fail_one_requirement_and_pass_another() {
    let repo = repository();
    let path = repo.path();
    let failed = ["INPUT-002".to_owned()];
    let run = RunRequest {
        failed: &failed,
        ..request(Method::Test(TEST.into()), None, Outcome::Pass)
    };
    record(path, &run, 1);
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Verified);
    assert_eq!(entry_status(path, "INPUT-002", 0), Status::Failed);
}

#[test]
fn target_evidence_belongs_to_a_board() {
    let repo = repository();
    let path = repo.path();
    let project = load(path);
    let now = Timestamp::from_unix(1_792_195_200);
    let without = evidence::build(
        &project,
        &request(Method::Procedure(BENCH.into()), None, Outcome::Pass),
        now,
    );
    assert!(without.unwrap_err().contains("needs --board"));
    let host_with_board = evidence::build(
        &project,
        &request(Method::Test(TEST.into()), Some("demo-board"), Outcome::Pass),
        now,
    );
    assert!(
        host_with_board
            .unwrap_err()
            .contains("--board applies only")
    );

    record(
        path,
        &request(
            Method::Procedure(BENCH.into()),
            Some("demo-board"),
            Outcome::Pass,
        ),
        1,
    );
    assert_eq!(entry_status(path, "INPUT-001", 1), Status::Verified);
    let project = load(path);
    let recorded = Recorded::new(&project, &evidence::load(path).unwrap()).unwrap();
    use reqtool::status::Evidence;
    assert_eq!(
        recorded.boards(&project.requirements["INPUT-001"], 1),
        vec![("demo-board".to_owned(), Status::Verified)]
    );

    edit(
        path,
        "firmware/demo-board/src/lib.rs",
        "pub fn wiring() {}",
        "pub fn wiring() { let _ = 3; }",
    );
    assert_eq!(
        entry_status(path, "INPUT-001", 1),
        Status::Stale,
        "the board's app is in the closure"
    );
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Unverified);
}

#[test]
fn the_procedure_section_and_the_requirement_text_are_fingerprinted() {
    let repo = repository();
    let path = repo.path();
    record(
        path,
        &request(
            Method::Procedure(BENCH.into()),
            Some("demo-board"),
            Outcome::Pass,
        ),
        1,
    );
    edit(
        path,
        "project_meta/testing/procedures.md",
        "# Procedures\n",
        "# Procedures\n\nUnrelated preamble.\n",
    );
    assert_eq!(
        entry_status(path, "INPUT-001", 1),
        Status::Verified,
        "text outside the section"
    );
    edit(
        path,
        "project_meta/testing/procedures.md",
        "Run it.",
        "Run it twice.",
    );
    assert_eq!(entry_status(path, "INPUT-001", 1), Status::Stale);

    let repo = repository();
    let path = repo.path();
    record(
        path,
        &request(Method::Test(TEST.into()), None, Outcome::Pass),
        1,
    );
    let yaml = "project_meta/requirements/input.yaml";
    edit(
        path,
        yaml,
        "      Command Input shall treat RC frames older than the link's limit as\n      invalid",
        "      Command Input shall treat RC frames older than the link's\n      limit as invalid",
    );
    assert_eq!(
        entry_status(path, "INPUT-001", 0),
        Status::Verified,
        "re-wrapping changes nothing"
    );
    edit(
        path,
        yaml,
        "older than the link's",
        "older than twice the link's",
    );
    assert_eq!(
        entry_status(path, "INPUT-001", 0),
        Status::Stale,
        "the claim changed"
    );
}

#[test]
fn parents_and_children_follow_recorded_evidence() {
    let repo = repository();
    let path = repo.path();
    assert_eq!(requirement_status(path, "INPUT-003"), Status::Unverified);
    record(
        path,
        &request(Method::Test(TEST.into()), None, Outcome::Pass),
        1,
    );
    assert_eq!(requirement_status(path, "INPUT-002"), Status::Verified);
    assert_eq!(
        requirement_status(path, "INPUT-003"),
        Status::Verified,
        "its only self-verifying child passed"
    );
    assert_eq!(
        requirement_status(path, "INPUT-004"),
        Status::Verified,
        "covered by its parent"
    );
}

#[test]
fn fingerprints_cover_the_lockfile_and_toolchain() {
    let repo = repository();
    let path = repo.path();
    let fingerprint = || {
        let project = load(path);
        let requirement = &project.requirements["INPUT-001"];
        Fingerprinter::new(&project.repository, &project.workspace)
            .entry(requirement, &requirement.verification.entries[0], None)
            .unwrap()
    };
    let before = fingerprint();
    edit(path, "Cargo.lock", "0bfb9eb6", "0bfb9eb7");
    let after_lock = fingerprint();
    assert_ne!(before, after_lock);
    edit(path, "rust-toolchain.toml", "1.93.1", "1.94.0");
    assert_ne!(after_lock, fingerprint());
}

#[test]
fn a_session_records_several_methods_and_repeats_add_nothing() {
    let repo = repository();
    let path = repo.path();
    let session = RunRequest {
        runs: vec![
            (Method::Test(TEST.into()), Outcome::Pass),
            (Method::Procedure(BENCH.into()), Outcome::Pass),
        ],
        ..request(Method::Test(TEST.into()), Some("demo-board"), Outcome::Pass)
    };
    let project = load(path);
    let now = Timestamp::from_unix(1_792_195_200);
    let built = evidence::build(&project, &session, now).unwrap();
    assert!(
        built.run_id.contains("__2-methods__demo-board__"),
        "{}",
        built.run_id
    );
    assert_eq!(built.results.len(), 3, "INPUT-001 twice, INPUT-002 once");
    evidence::write(path, &built, &now).unwrap();
    assert_eq!(entry_status(path, "INPUT-001", 0), Status::Verified);
    assert_eq!(entry_status(path, "INPUT-001", 1), Status::Verified);

    let existing = evidence::load(path).unwrap();
    let mut again = evidence::build(&load(path), &session, now).unwrap();
    evidence::drop_repeats(&mut again, &existing);
    assert!(
        again.results.is_empty(),
        "unchanged passes are already on record"
    );

    edit(
        path,
        "crates/alpha/src/input.rs",
        "pub fn validate() {}",
        "pub fn validate() { let _ = 4; }",
    );
    let existing = evidence::load(path).unwrap();
    let mut changed = evidence::build(&load(path), &session, now).unwrap();
    evidence::drop_repeats(&mut changed, &existing);
    assert!(!changed.results.is_empty(), "changed code is new evidence");

    let failing = request(Method::Test(TEST.into()), None, Outcome::Fail);
    let mut fail = evidence::build(&load(path), &failing, now).unwrap();
    let before = fail.results.len();
    evidence::drop_repeats(&mut fail, &evidence::load(path).unwrap());
    assert_eq!(fail.results.len(), before, "a fail is always kept");
}

fn head(repository: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A catalog naming the fixture's bench procedure, and an old schema 2 record
/// of it at `revision`.
fn old_record(repository: &Path, run_id: &str, test_id: &str, revision: &str, result: &str) {
    let catalog = r#"{"schema_version": 1, "tests": [
        {"id": "BENCH-DEMO-001", "status": "active",
         "procedure": {"path": "project_meta/testing/procedures.md", "heading": "Bench Gate"}},
        {"id": "SW-COMMON-001", "status": "active",
         "procedure": {"path": "project_meta/testing/procedures.md", "heading": "Bench Gate"}}
    ]}"#;
    std::fs::write(
        repository.join("project_meta/testing/TEST_CATALOG.json"),
        catalog,
    )
    .unwrap();
    let directory = repository.join("project_meta/testing/evidence/runs/2026/10");
    std::fs::create_dir_all(&directory).unwrap();
    let record = format!(
        r#"{{"schema_version": 2, "run_id": "{run_id}", "test_id": "{test_id}",
            "target": "demo-board", "result": "{result}", "completed_at_utc": "2026-10-01T12:00:00Z",
            "candidate": {{"revision": "{revision}", "working_tree": "clean"}}}}"#
    );
    std::fs::write(directory.join(format!("{run_id}.json")), record).unwrap();
}

#[test]
fn import_carries_old_records_over_at_their_own_revision() {
    let repo = repository();
    let path = repo.path();
    let revision = head(path);
    old_record(
        path,
        "20261001T120000Z__BENCH-DEMO-001__demo-board__01",
        "BENCH-DEMO-001",
        &revision,
        "pass",
    );
    old_record(
        path,
        "20261001T120000Z__SW-COMMON-001__common__01",
        "SW-COMMON-001",
        &revision,
        "pass",
    );

    let project = load(path);
    let written = reqtool::import::write(&project, reqtool::import::plan(&project).unwrap());
    let imported: Vec<_> = written
        .iter()
        .filter(|(_, outcome)| matches!(outcome, reqtool::import::Imported::Written { .. }))
        .collect();
    assert_eq!(imported.len(), 1, "{written:?}");
    assert!(written.iter().any(|(_, outcome)| matches!(outcome,
        reqtool::import::Imported::Skipped(why) if why.contains("superseded by `reqtool test`"))));
    assert_eq!(
        entry_status(path, "INPUT-001", 1),
        Status::Verified,
        "nothing changed since the old run"
    );

    let again = reqtool::import::write(&load(path), reqtool::import::plan(&load(path)).unwrap());
    assert!(
        again
            .iter()
            .any(|(_, outcome)| matches!(outcome, reqtool::import::Imported::Present(_)))
    );

    edit(
        path,
        "crates/alpha/src/input.rs",
        "pub fn validate() {}",
        "pub fn validate() { let _ = 5; }",
    );
    assert_eq!(
        entry_status(path, "INPUT-001", 1),
        Status::Stale,
        "the code changed since"
    );
}

#[test]
fn import_of_an_unknown_revision_is_never_current() {
    let repo = repository();
    let path = repo.path();
    old_record(
        path,
        "20261001T120000Z__BENCH-DEMO-001__demo-board__01",
        "BENCH-DEMO-001",
        "0123456789abcdef0123456789abcdef01234567",
        "pass",
    );
    let project = load(path);
    let plans = reqtool::import::plan(&project).unwrap();
    let record = plans[0].outcome.as_ref().unwrap().as_ref().unwrap();
    assert!(
        record
            .results
            .iter()
            .all(|r| r.fingerprint.starts_with("unavailable:"))
    );
    reqtool::import::write(&project, plans);
    assert_eq!(entry_status(path, "INPUT-001", 1), Status::Stale);
}

const GATES: &str = "schema_version: 1
gates:
  - name: host
    description: host evidence
    requires:
      - requirements: [\"*\"]
        levels: [unit]
  - name: bench
    description: board evidence
    includes: [host]
    requires:
      - requirements: [INPUT-001]
        levels: [bench]
";

fn gate(
    path: &Path,
    name: &str,
    board: Option<&str>,
) -> Result<Vec<reqtool::gates::Missing>, String> {
    let project = load(path);
    let recorded = Recorded::new(&project, &evidence::load(path).unwrap()).unwrap();
    reqtool::gates::check(
        &project.gates,
        name,
        board,
        &project.requirements,
        &recorded,
    )
}

#[test]
fn a_gate_opens_only_when_everything_it_includes_is_verified() {
    let repo = repository();
    let path = repo.path();
    std::fs::write(path.join("project_meta/requirements/gates.yaml"), GATES).unwrap();
    assert_eq!(
        gate(path, "host", None).unwrap().len(),
        2,
        "two unit entries, none verified"
    );
    assert!(
        gate(path, "bench", None)
            .unwrap_err()
            .contains("pass --board")
    );

    record(
        path,
        &request(Method::Test(TEST.into()), None, Outcome::Pass),
        1,
    );
    assert!(
        gate(path, "host", None).unwrap().is_empty(),
        "host gate open"
    );
    let bench = gate(path, "bench", Some("demo-board")).unwrap();
    assert_eq!(bench.len(), 1, "{bench:?}");
    assert_eq!(bench[0].requirement, "INPUT-001");

    record(
        path,
        &request(
            Method::Procedure(BENCH.into()),
            Some("demo-board"),
            Outcome::Pass,
        ),
        2,
    );
    assert!(
        gate(path, "bench", Some("demo-board")).unwrap().is_empty(),
        "bench gate open"
    );

    edit(
        path,
        "crates/alpha/src/input.rs",
        "pub fn validate() {}",
        "pub fn validate() { let _ = 6; }",
    );
    let stale = gate(path, "bench", Some("demo-board")).unwrap();
    assert!(
        stale.iter().all(|m| m.status == Status::Stale) && stale.len() == 3,
        "{stale:?}"
    );
}

#[test]
fn gates_are_validated_against_the_requirements() {
    let repo = repository();
    let path = repo.path();
    let broken = "schema_version: 1
gates:
  - name: a
    description: a
    includes: [b, missing]
    requires:
      - requirements: [NOPE-*, INPUT-003]
        levels: [unit]
  - name: b
    description: b
    includes: [a]
";
    std::fs::write(path.join("project_meta/requirements/gates.yaml"), broken).unwrap();
    let (_, problems) = Project::load(path).unwrap();
    let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
    for expected in [
        "includes unknown gate missing",
        "NOPE-* selects no requirement",
        "INPUT-003 has no entry at unit",
        "gate a includes itself",
    ] {
        assert!(
            text.iter().any(|p| p.contains(expected)),
            "{expected} in {text:#?}"
        );
    }
}
