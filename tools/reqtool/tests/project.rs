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

const VALID_ONE: &str = "schema_version: 2
requirements:
  - id: INPUT-001
    title: Reject stale RC input
    statement: Command Input shall reject stale frames.
    rationale: Stale input is never a live command.
    derived_from: [W2]
    verification:
      entries:
        - level: unit
          target: host
          setup: none
          method:
            test: alpha::input::tests::rejects_stale
  - id: INPUT-002
    title: a
    statement: a
    rationale: a
    derived_from: [W2]
    verification:
      entries: []
  - id: INPUT-004
    title: b
    statement: b
    rationale: b
    derived_from: [W2]
    verification:
      entries: []
";

fn edit(repository: &Path, file: &str, from: &str, to: &str) {
    let path = repository.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{from} in {file}");
    std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
}

fn findings(repository: &Path) -> Vec<String> {
    let (project, problems) = Project::load(repository).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    project
        .trace_findings()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn the_fixture_is_valid_and_everything_starts_unverified() {
    let (project, problems) = Project::load(&fixture()).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(project.trace_findings().is_empty());
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
    let (project, problems) = Project::load(&repository).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    let findings = project.trace_findings();
    assert!(findings.is_empty(), "{findings:#?}");
    let text = reqtool::lint::project(&project);
    assert!(text.is_empty(), "{text:#?}");
}

#[test]
fn files_declare_requirements_and_imports_resolve_to_files() {
    let (project, _) = Project::load(&fixture()).unwrap();
    let input = &project.trace.files["crates/alpha/src/input.rs"];
    assert!(input.critical);
    assert_eq!(input.requirements, ["INPUT-001", "INPUT-002"]);
    let imports: Vec<&str> = input.imports.keys().map(String::as_str).collect();
    assert_eq!(
        imports,
        ["crates/alpha/src/hidden.rs"],
        "its own module is not an import"
    );

    let root: Vec<&str> = project.trace.files["crates/alpha/src/lib.rs"]
        .imports
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        root,
        [
            "crates/alpha/src/hidden.rs",
            "crates/alpha/src/input/nested.rs"
        ],
        "a glob covers the module it names, not the modules on its path"
    );
    assert!(!project.trace.files["crates/alpha/src/lib.rs"].defines_code);

    let beta = &project.trace.files["crates/beta/src/lib.rs"];
    assert!(!beta.critical);
    let imports: Vec<&str> = beta.imports.keys().map(String::as_str).collect();
    assert_eq!(
        imports,
        ["crates/alpha/src/input.rs", "extern:heapless@0.8.0"],
        "a re-export resolves to where the item is defined; an external crate to its locked version"
    );
}

#[test]
fn check_by_file_or_module_shows_what_it_declares() {
    let (project, _) = Project::load(&fixture()).unwrap();
    let by_file = report::check(&project, "crates/alpha/src/input.rs", &NoEvidence).unwrap();
    assert!(by_file.contains("safety-critical"), "{by_file}");
    assert!(
        by_file.contains("declares: INPUT-001, INPUT-002"),
        "{by_file}"
    );
    let by_module = report::check(&project, "alpha::input::validate", &NoEvidence).unwrap();
    assert_eq!(by_file, by_module);
    let none = report::check(&project, "beta::plain", &NoEvidence).unwrap();
    assert!(none.contains("not safety-critical"), "{none}");
    assert!(none.contains("declares no requirement"), "{none}");
    assert!(report::check(&project, "alpha::missing", &NoEvidence).is_err());
}

#[test]
fn check_by_id_and_by_root() {
    let (project, _) = Project::load(&fixture()).unwrap();
    let text = report::check(&project, "INPUT-003", &NoEvidence).unwrap();
    assert!(text.contains("children: INPUT-002, INPUT-004"), "{text}");
    assert!(text.contains("no verification method yet"), "{text}");
    let text = report::check(&project, "INPUT-002", &NoEvidence).unwrap();
    assert!(
        text.contains("declared by: crates/alpha/src/hidden.rs, crates/alpha/src/input.rs"),
        "{text}"
    );
    let text = report::check(&project, "W2", &NoEvidence).unwrap();
    assert!(text.contains("INPUT-001"), "{text}");
}

#[test]
fn a_valid_single_requirement_has_no_problems() {
    assert_eq!(problems(VALID_ONE), Vec::<String>::new());
}

#[test]
fn unknown_fields_are_refused() {
    for field in [
        "    status: verified\n",
        "    allocated_to: [alpha::input]\n",
    ] {
        let extra = VALID_ONE.replacen("    title:", &format!("{field}    title:"), 1);
        assert!(!problems(&extra).is_empty(), "{field}");
    }
    let mode = VALID_ONE.replacen("      entries:", "      mode: direct\n      entries:", 1);
    assert!(!problems(&mode).is_empty(), "verification modes are gone");
}

#[test]
fn ids_must_be_well_formed_unique_and_not_roots() {
    assert_problem(
        &VALID_ONE.replace("INPUT-001", "input-1"),
        "is not <AREA>-<NNN>",
    );
    let twice = format!(
        "{VALID_ONE}{}",
        &VALID_ONE["schema_version: 2\nrequirements:\n".len()..]
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
        &VALID_ONE.replace("tests::rejects_stale", "tests::helper"),
        "is not a #[test] function",
    );
    assert_problem(
        &VALID_ONE.replace("alpha::input::tests", "gamma::tests"),
        "no crate gamma",
    );
}

#[test]
fn declarations_must_name_known_requirements() {
    let copy = with_requirements(VALID_ONE);
    edit(
        copy.path(),
        "crates/alpha/src/input.rs",
        "INPUT-001, INPUT-002",
        "INPUT-001, INPUT-001, INPUT-099",
    );
    let (_, problems) = Project::load(copy.path()).unwrap();
    let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
    for expected in [
        "crates/alpha/src/input.rs: declares INPUT-001 twice",
        "crates/alpha/src/input.rs: declares unknown requirement INPUT-099",
    ] {
        assert!(
            text.iter().any(|p| p == expected),
            "{expected} in {text:#?}"
        );
    }
}

#[test]
fn a_critical_file_declares_a_requirement_and_imports_only_verified_code() {
    let copy = with_requirements(VALID_ONE);
    let path = copy.path();
    edit(
        path,
        "crates/alpha/src/input/nested.rs",
        "//! Requirements: INPUT-004\n",
        "",
    );
    assert_eq!(
        findings(path),
        [
            "crates/alpha/src/input/nested.rs: is safety-critical, and declares no requirement it implements"
        ]
    );

    let copy = with_requirements(VALID_ONE);
    let path = copy.path();
    edit(
        path,
        "crates/alpha/src/hidden.rs",
        "pub struct Original;",
        "pub struct Original;\n\npub fn helper() { beta::plain(); }",
    );
    edit(
        path,
        "crates/alpha/Cargo.toml",
        "[lib]",
        "[dependencies]\nheapless = \"0.8\"\n\n[lib]",
    );
    edit(
        path,
        "crates/alpha/src/input.rs",
        "use crate::hidden::Original;",
        "use crate::hidden::Original;\nuse heapless::Vec;",
    );
    assert_eq!(
        findings(path),
        [
            "crates/alpha/src/hidden.rs: is safety-critical, and imports crates/beta/src/lib.rs, which is not",
            "crates/alpha/src/input.rs: is safety-critical, and imports heapless@unlocked, which its manifest does not pin to one version",
        ]
    );
    edit(path, "crates/alpha/Cargo.toml", "\"0.8\"", "\"=0.8.0\"");
    assert_eq!(
        findings(path).len(),
        1,
        "a pinned external crate is allowed"
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
fn a_requirement_without_a_method_is_valid_and_shows_as_a_gap() {
    let copy = with_requirements(VALID_ONE);
    let (project, problems) = Project::load(copy.path()).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    let detail = report::check(&project, "INPUT-002", &NoEvidence).unwrap();
    assert!(detail.contains("no verification method yet"), "{detail}");
    let summary = report::summary(&project, &NoEvidence);
    assert!(
        summary.contains("2 with no verification method yet"),
        "{summary}"
    );
}

#[test]
fn derivation_cycles_are_refused() {
    let cycle = "schema_version: 2
requirements:
  - id: A-001
    title: a
    statement: a
    rationale: a
    derived_from: [B-001]
    verification:
      entries: []
  - id: B-001
    title: b
    statement: b
    rationale: b
    derived_from: [A-001]
    verification:
      entries: []
";
    assert_problem(cycle, "derived_from forms a cycle");
}

#[test]
fn a_procedure_never_points_into_an_archive() {
    let archived = VALID_ONE
        .replace("level: unit", "level: bench")
        .replace("target: host", "target: fcu")
        .replace("setup: none", "setup: props-off")
        .replace(
            "test: alpha::input::tests::rejects_stale",
            "procedure: archive/old.md#Gate",
        );
    assert_problem(&archived, "is archived");
}
