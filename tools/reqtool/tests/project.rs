use std::path::{Path, PathBuf};

use reqtool::project::Project;
use reqtool::report;
use reqtool::status::{self, NoEvidence, Status};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/repo")
}

/// A copy of the fixture whose only requirement file is `yaml`.
fn with_requirements(yaml: &str) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    copy_tree(&fixture(), copy.path());
    let requirements = copy.path().join("project_meta/requirements");
    std::fs::remove_dir_all(&requirements).unwrap();
    std::fs::create_dir_all(&requirements).unwrap();
    std::fs::write(requirements.join("case.yaml"), yaml).unwrap();
    copy
}

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

fn problems(yaml: &str) -> Vec<String> {
    let copy = with_requirements(yaml);
    let (_, problems) = Project::load(copy.path()).unwrap();
    problems.iter().map(ToString::to_string).collect()
}

fn assert_problem(yaml: &str, expected: &str) {
    let found = problems(yaml);
    assert!(
        found.iter().any(|problem| problem.contains(expected)),
        "expected {expected:?} in {found:#?}"
    );
}

const VALID_ONE: &str = "schema_version: 1
requirements:
  - id: INPUT-001
    title: Reject stale RC input
    statement: Command Input shall reject stale frames.
    rationale: Stale input is never a live command.
    derived_from: [W2]
    allocated_to: [alpha::input]
    verification:
      mode: direct
      entries:
        - level: unit
          target: host
          setup: none
          method:
            test: alpha::input::tests::rejects_stale
";

#[test]
fn the_fixture_is_valid_and_everything_starts_unverified() {
    let (project, problems) = Project::load(&fixture()).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(project.requirements.len(), 4);
    let statuses = status::compute(&project.requirements, &NoEvidence);
    assert!(
        statuses
            .values()
            .all(|status| *status == Status::Unverified)
    );
}

#[test]
fn the_real_repository_is_valid() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (_, problems) = Project::load(&repository).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn check_by_item_finds_requirements_allocated_around_it() {
    let (project, _) = Project::load(&fixture()).unwrap();
    let inside = report::check(&project, "alpha::input::validate", &NoEvidence).unwrap();
    assert!(inside.contains("INPUT-001"), "{inside}");
    let around = report::check(&project, "alpha", &NoEvidence).unwrap();
    for id in ["INPUT-001", "INPUT-002", "INPUT-004"] {
        assert!(around.contains(id), "{id} in {around}");
    }
    let none = report::check(&project, "beta::plain", &NoEvidence).unwrap();
    assert!(none.starts_with("no requirement is allocated"), "{none}");
    assert!(report::check(&project, "alpha::missing", &NoEvidence).is_err());
}

#[test]
fn check_reports_mandatory_linking_in_a_critical_crate() {
    let copy = with_requirements("schema_version: 1\nrequirements: []\n");
    let (project, _) = Project::load(copy.path()).unwrap();
    let text = report::check(&project, "alpha::input::validate", &NoEvidence).unwrap();
    assert!(text.contains("alpha is safety-critical"), "{text}");
    let text = report::check(&project, "beta::plain", &NoEvidence).unwrap();
    assert!(!text.contains("safety-critical"), "{text}");
}

#[test]
fn check_by_id_and_by_root() {
    let (project, _) = Project::load(&fixture()).unwrap();
    let text = report::check(&project, "INPUT-003", &NoEvidence).unwrap();
    assert!(text.contains("verification: via_children"), "{text}");
    assert!(text.contains("children: INPUT-002, INPUT-004"), "{text}");
    let text = report::check(&project, "W2", &NoEvidence).unwrap();
    assert!(text.contains("INPUT-001"), "{text}");
}

#[test]
fn a_valid_single_requirement_has_no_problems() {
    assert_eq!(problems(VALID_ONE), Vec::<String>::new());
}

#[test]
fn unknown_fields_are_refused() {
    let status_field = VALID_ONE.replace("    title:", "    status: verified\n    title:");
    assert!(!problems(&status_field).is_empty());
}

#[test]
fn ids_must_be_well_formed_unique_and_not_roots() {
    assert_problem(
        &VALID_ONE.replace("INPUT-001", "input-1"),
        "is not <AREA>-<NNN>",
    );
    let twice = format!(
        "{VALID_ONE}{}",
        &VALID_ONE["schema_version: 1\nrequirements:\n".len()..]
    );
    assert_problem(&twice, "duplicate ID");
}

#[test]
fn references_must_resolve() {
    assert_problem(
        &VALID_ONE.replace("[W2]", "[W9]"),
        "derives from unknown W9",
    );
    assert_problem(&VALID_ONE.replace("[W2]", "[]"), "derived_from is empty");
    assert_problem(
        &VALID_ONE.replace("[alpha::input]", "[alpha::output]"),
        "does not resolve",
    );
    assert_problem(
        &VALID_ONE.replace("[alpha::input]", "[gamma::x]"),
        "no crate gamma",
    );
    assert_problem(
        &VALID_ONE.replace("tests::rejects_stale", "tests::helper"),
        "is not a #[test] function",
    );
}

#[test]
fn levels_targets_setups_and_methods_must_fit() {
    assert_problem(
        &VALID_ONE.replace("target: host", "target: fcu"),
        "target fcu does not fit level unit",
    );
    assert_problem(
        &VALID_ONE.replace("level: unit", "level: bench"),
        "setup none does not fit level bench",
    );
    let procedure = VALID_ONE.replace(
        "test: alpha::input::tests::rejects_stale",
        "procedure: project_meta/testing/procedures.md#Bench Gate",
    );
    assert_problem(&procedure, "level unit is verified by a host test");
    let missing_heading = procedure
        .replace("level: unit", "level: bench")
        .replace("target: host", "target: fcu")
        .replace("setup: none", "setup: props-off")
        .replace("#Bench Gate", "#No Such Gate");
    assert_problem(&missing_heading, "has no heading \"No Such Gate\"");
}

#[test]
fn modes_need_their_reasons_and_relations() {
    assert_problem(
        &VALID_ONE.replace("mode: direct", "mode: via_children"),
        "no child verifies itself",
    );
    assert_problem(
        &VALID_ONE.replace("mode: direct", "mode: covered_by_parent"),
        "derives from no parent requirement",
    );
    assert_problem(
        &VALID_ONE.replace("mode: direct", "mode: direct\n      observed_by: counter"),
        "observed_by only applies to covered_by_parent",
    );
}

#[test]
fn a_requirement_without_a_method_is_valid_and_shows_as_a_gap() {
    let no_entries = VALID_ONE.split("      entries:").next().unwrap().to_owned();
    let copy = with_requirements(&no_entries);
    let (project, problems) = Project::load(copy.path()).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    let detail = report::check(&project, "INPUT-001", &NoEvidence).unwrap();
    assert!(detail.contains("no verification method yet"), "{detail}");
    let summary = report::summary(&project, &NoEvidence);
    assert!(
        summary.contains("1 with no verification method yet"),
        "{summary}"
    );
}

#[test]
fn derivation_cycles_are_refused() {
    let cycle = "schema_version: 1
requirements:
  - id: A-001
    title: a
    statement: a
    rationale: a
    derived_from: [B-001]
    verification:
      mode: covered_by_parent
      observed_by: x
      reason: x
  - id: B-001
    title: b
    statement: b
    rationale: b
    derived_from: [A-001]
    verification:
      mode: covered_by_parent
      observed_by: x
      reason: x
";
    assert_problem(cycle, "derived_from forms a cycle");
}

#[test]
fn a_parent_cannot_be_proven_only_by_children_it_covers() {
    let only_covered = "schema_version: 1
requirements:
  - id: P-001
    title: p
    statement: p
    rationale: p
    derived_from: [W7]
    verification:
      mode: via_children
      reason: x
  - id: C-001
    title: c
    statement: c
    rationale: c
    derived_from: [P-001]
    verification:
      mode: covered_by_parent
      observed_by: x
      reason: x
";
    assert_problem(only_covered, "no child verifies itself");
}
