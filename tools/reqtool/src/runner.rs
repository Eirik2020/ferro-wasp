//! `reqtool test`: run the host tests that requirements name, and record what
//! each one did, so unit evidence is never typed in by hand.
//!
//! Tests run per package with cargo, in the workspace that resolves the
//! package and on that workspace's own toolchain, as `cargo test --lib` with
//! exact name filters. A test that did not run (filtered out by features,
//! ignored, or missing from the build) is reported and never recorded.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::project::Project;
use crate::schema::Method;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ran {
    Passed,
    Failed,
    Ignored,
    NotRun,
}

impl Ran {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Ignored => "ignored",
            Self::NotRun => "did not run",
        }
    }
}

/// The host tests named by host-level entries of `requirements`, or of every
/// requirement when the list is empty.
pub fn named_tests(project: &Project, requirements: &[String]) -> Result<BTreeSet<String>, String> {
    for id in requirements {
        if !project.requirements.contains_key(id) {
            return Err(format!("no requirement {id}"));
        }
    }
    let mut tests = BTreeSet::new();
    for requirement in project.requirements.values() {
        if !requirements.is_empty() && !requirements.contains(&requirement.id) {
            continue;
        }
        for entry in &requirement.verification.entries {
            if let Method::Test(path) = &entry.method
                && !entry.level.on_target()
            {
                tests.insert(path.clone());
            }
        }
    }
    Ok(tests)
}

/// Runs the tests and reports what each one did. `Err` means cargo could not
/// build or run a package's tests at all.
pub fn run(project: &Project, tests: &BTreeSet<String>) -> Result<BTreeMap<String, Ran>, String> {
    let mut by_package: BTreeMap<(PathBuf, String), Vec<(String, String)>> = BTreeMap::new();
    for path in tests {
        let (crate_name, inside) = path
            .split_once("::")
            .ok_or_else(|| format!("{path}: not a crate-qualified test path"))?;
        let found = project
            .workspace
            .crates
            .get(crate_name)
            .ok_or_else(|| format!("{path}: no crate {crate_name}"))?;
        let workspace = found
            .lockfile
            .parent()
            .unwrap_or(&project.repository)
            .to_path_buf();
        by_package
            .entry((workspace, found.package.clone()))
            .or_default()
            .push((path.clone(), inside.to_owned()));
    }

    let mut outcomes = BTreeMap::new();
    for ((workspace, package), tests) in by_package {
        let host = host_triple(&workspace)?;
        let mut command = Command::new("cargo");
        command
            .args([
                "test", "--locked", "--target", &host, "-p", &package, "--lib", "--", "--exact",
            ])
            .args(tests.iter().map(|(_, filter)| filter))
            .current_dir(&workspace);
        let output = command
            .output()
            .map_err(|error| format!("{package}: cannot run cargo: {error}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.lines().any(|line| line.starts_with("test result:")) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(20).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            return Err(format!(
                "{package}: the tests did not build or run:\n{}",
                tail.join("\n")
            ));
        }
        let reported = parse_libtest(&stdout);
        for (path, filter) in tests {
            outcomes.insert(
                path,
                reported
                    .get(filter.as_str())
                    .copied()
                    .unwrap_or(Ran::NotRun),
            );
        }
    }
    Ok(outcomes)
}

/// `test <name> ... ok | FAILED | ignored` lines from libtest's output.
pub fn parse_libtest(stdout: &str) -> BTreeMap<&str, Ran> {
    stdout
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("test ")?;
            let (name, verdict) = rest.rsplit_once(" ... ")?;
            let ran = match verdict.trim() {
                "ok" => Ran::Passed,
                "FAILED" => Ran::Failed,
                verdict if verdict.starts_with("ignored") => Ran::Ignored,
                _ => return None,
            };
            Some((name.trim(), ran))
        })
        .collect()
}

fn host_triple(workspace: &Path) -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .current_dir(workspace)
        .output()
        .map_err(|error| format!("cannot run rustc: {error}"))?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or_else(|| "rustc -vV reported no host triple".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libtest_lines_become_outcomes() {
        let output = "\nrunning 3 tests\n\
                      test safety::tests::a ... ok\n\
                      test safety::tests::b ... FAILED\n\
                      test safety::tests::c ... ignored, needs hardware\n\
                      \n\
                      test result: FAILED. 1 passed; 1 failed; 1 ignored\n";
        let parsed = parse_libtest(output);
        assert_eq!(parsed["safety::tests::a"], Ran::Passed);
        assert_eq!(parsed["safety::tests::b"], Ran::Failed);
        assert_eq!(parsed["safety::tests::c"], Ran::Ignored);
        assert_eq!(parsed.len(), 3);
    }
}
