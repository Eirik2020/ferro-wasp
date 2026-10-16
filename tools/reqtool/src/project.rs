//! Loading the requirement files and checking them against the roots, each
//! other, and the code.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::items::Resolver;
use crate::roots::{self, Root};
use crate::schema::{
    Entry, Level, Method, Mode, Requirement, RequirementFile, SCHEMA_VERSION, Setup, Target,
};
use crate::workspace::{self, Crate};

pub const REQUIREMENTS: &str = "project_meta/requirements";

/// One thing wrong, at a file or a requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Problem {
    pub location: String,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

pub struct Project {
    pub repository: PathBuf,
    pub roots: Vec<Root>,
    pub requirements: BTreeMap<String, Requirement>,
    /// The file each requirement came from, relative to the repository.
    pub sources: BTreeMap<String, String>,
    pub crates: BTreeMap<String, Crate>,
}

impl Project {
    /// Loads everything; returns the problems found alongside what loaded.
    /// An `Err` means the repository itself could not be read.
    pub fn load(repository: &Path) -> Result<(Self, Vec<Problem>), String> {
        let roots = roots::load(repository)?;
        let crates = workspace::discover(repository)?;
        let mut problems = Vec::new();
        let mut requirements = BTreeMap::new();
        let mut sources = BTreeMap::new();

        for file in requirement_files(repository)? {
            let relative = relative(repository, &file);
            let parsed = std::fs::read_to_string(&file)
                .map_err(|error| error.to_string())
                .and_then(|text| {
                    serde_saphyr::from_str::<RequirementFile>(&text)
                        .map_err(|error| error.to_string())
                });
            let parsed = match parsed {
                Ok(parsed) => parsed,
                Err(message) => {
                    problems.push(Problem {
                        location: relative,
                        message,
                    });
                    continue;
                }
            };
            if parsed.schema_version != SCHEMA_VERSION {
                problems.push(Problem {
                    location: relative,
                    message: format!(
                        "schema_version {} is not {SCHEMA_VERSION}",
                        parsed.schema_version
                    ),
                });
                continue;
            }
            for requirement in parsed.requirements {
                let location = format!("{relative}: {}", requirement.id);
                if let Err(message) = check_id(&requirement.id) {
                    problems.push(Problem { location, message });
                    continue;
                }
                if roots.iter().any(|root| root.id == requirement.id) {
                    problems.push(Problem {
                        location,
                        message: "reuses a governing requirement's ID".into(),
                    });
                    continue;
                }
                if let Some(first) = sources.get(&requirement.id) {
                    problems.push(Problem {
                        location,
                        message: format!("duplicate ID, first defined in {first}"),
                    });
                    continue;
                }
                sources.insert(requirement.id.clone(), relative.clone());
                requirements.insert(requirement.id.clone(), requirement);
            }
        }

        let project = Self {
            repository: repository.to_path_buf(),
            roots,
            requirements,
            sources,
            crates,
        };
        problems.extend(project.validate());
        Ok((project, problems))
    }

    pub fn root(&self, id: &str) -> Option<&Root> {
        self.roots.iter().find(|root| root.id == id)
    }

    /// Requirements that name `parent` in `derived_from`.
    pub fn children(&self, parent: &str) -> Vec<&Requirement> {
        self.requirements
            .values()
            .filter(|r| r.derived_from.iter().any(|p| p == parent))
            .collect()
    }

    fn location(&self, id: &str) -> String {
        format!("{}: {id}", self.sources.get(id).map_or("?", String::as_str))
    }

    fn validate(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        let resolver = Resolver::new(&self.crates);
        for requirement in self.requirements.values() {
            let location = self.location(&requirement.id);
            let mut problem = |message: String| {
                problems.push(Problem {
                    location: location.clone(),
                    message,
                })
            };
            for (field, value) in [
                ("title", &requirement.title),
                ("statement", &requirement.statement),
                ("rationale", &requirement.rationale),
            ] {
                if value.trim().is_empty() {
                    problem(format!("{field} is empty"));
                }
            }
            if requirement.derived_from.is_empty() {
                problem(
                    "derived_from is empty; every requirement traces to W1-W7 or a parent".into(),
                );
            }
            for parent in &requirement.derived_from {
                if parent == &requirement.id {
                    problem("derives from itself".into());
                } else if self.root(parent).is_none() && !self.requirements.contains_key(parent) {
                    problem(format!("derives from unknown {parent}"));
                }
            }
            let mut seen = BTreeSet::new();
            for path in &requirement.allocated_to {
                if !seen.insert(path) {
                    problem(format!("allocated twice to {path}"));
                } else if let Err(message) = resolver.resolve(path) {
                    problem(format!(
                        "allocated to an item that does not resolve: {message}"
                    ));
                }
            }
            for message in self.mode_problems(requirement) {
                problem(message);
            }
            for (index, entry) in requirement.verification.entries.iter().enumerate() {
                for message in entry_problems(entry, &resolver, &self.repository) {
                    problem(format!("entry {}: {message}", index + 1));
                }
            }
        }
        problems.extend(self.cycles());
        problems
    }

    fn mode_problems(&self, requirement: &Requirement) -> Vec<String> {
        let verification = &requirement.verification;
        // A child covered by this requirement takes its status from it, so it
        // cannot help prove it.
        let has_children = self
            .children(&requirement.id)
            .iter()
            .any(|child| child.verification.mode != Mode::CoveredByParent);
        let has_parent = requirement
            .derived_from
            .iter()
            .any(|parent| self.requirements.contains_key(parent));
        let has_reason = verification
            .reason
            .as_deref()
            .is_some_and(|reason| !reason.trim().is_empty());
        let mut problems = Vec::new();
        match verification.mode {
            Mode::Direct => {
                if verification.entries.is_empty() {
                    problems.push("direct verification has no entries".into());
                }
            }
            Mode::ViaChildren => {
                if !has_children {
                    problems.push(
                        "via_children, but no child verifies itself (children covered by this one cannot prove it)".into(),
                    );
                }
                if !has_reason {
                    problems.push(
                        "via_children needs a reason: why the children are jointly sufficient"
                            .into(),
                    );
                }
            }
            Mode::CoveredByParent => {
                if !has_parent {
                    problems.push(
                        "covered_by_parent, but it derives from no parent requirement".into(),
                    );
                }
                if !has_reason {
                    problems.push("covered_by_parent needs a reason".into());
                }
                if verification
                    .observed_by
                    .as_deref()
                    .is_none_or(|observable| observable.trim().is_empty())
                {
                    problems.push("covered_by_parent needs observed_by: what in the parent's run observes this".into());
                }
            }
            Mode::Either => {
                if verification.entries.is_empty() || !has_children {
                    problems.push(
                        "either needs its own entries and at least one child that verifies itself"
                            .into(),
                    );
                }
                if !has_reason {
                    problems.push(
                        "either needs a reason: why the children are jointly sufficient".into(),
                    );
                }
            }
        }
        if verification.observed_by.is_some() && verification.mode != Mode::CoveredByParent {
            problems.push(format!(
                "observed_by only applies to covered_by_parent, not {}",
                verification.mode.name()
            ));
        }
        problems
    }

    /// Cycles through `derived_from`, which would make status undefined.
    fn cycles(&self) -> Vec<Problem> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Open,
            Done,
        }
        fn visit(
            project: &Project,
            id: &str,
            marks: &mut BTreeMap<String, Mark>,
            found: &mut Vec<Problem>,
        ) {
            match marks.get(id) {
                Some(Mark::Done) => return,
                Some(Mark::Open) => {
                    found.push(Problem {
                        location: project.location(id),
                        message: "derived_from forms a cycle".into(),
                    });
                    return;
                }
                None => {}
            }
            marks.insert(id.to_owned(), Mark::Open);
            for parent in &project.requirements[id].derived_from {
                if project.requirements.contains_key(parent) {
                    visit(project, parent, marks, found);
                }
            }
            marks.insert(id.to_owned(), Mark::Done);
        }
        let mut marks = BTreeMap::new();
        let mut found = Vec::new();
        for id in self.requirements.keys() {
            visit(self, id, &mut marks, &mut found);
        }
        found
    }
}

fn entry_problems(entry: &Entry, resolver: &Resolver<'_>, repository: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let (targets, setups): (&[Target], &[Setup]) = match entry.level {
        Level::Analysis | Level::Unit => (&[Target::Host], &[Setup::None]),
        Level::Sitl => (&[Target::Host], &[Setup::Sim]),
        Level::Mcu => (&[Target::Fcu, Target::Devboard], &[Setup::None]),
        Level::Bench => (&[Target::Fcu, Target::Devboard], &[Setup::PropsOff]),
        Level::Flight => (&[Target::Fcu], &[Setup::None]),
    };
    let level = entry.level.name();
    if !targets.contains(&entry.target) {
        problems.push(format!(
            "target {} does not fit level {level}",
            entry.target.name()
        ));
    }
    if !setups.contains(&entry.setup) {
        problems.push(format!(
            "setup {} does not fit level {level}",
            entry.setup.name()
        ));
    }
    match &entry.method {
        Method::Test(path) => {
            if entry.level >= Level::Mcu {
                problems.push(format!(
                    "level {level} is verified by a procedure, not a host test"
                ));
            }
            match resolver.resolve(path) {
                Ok(resolved) if resolved.kind.is_test() => {}
                Ok(_) => problems.push(format!("{path} is not a #[test] function")),
                Err(message) => problems.push(format!("test does not resolve: {message}")),
            }
        }
        Method::Procedure(reference) => {
            if matches!(entry.level, Level::Unit) {
                problems.push("level unit is verified by a host test, not a procedure".into());
            }
            if let Err(message) = check_procedure(repository, reference) {
                problems.push(message);
            }
        }
    }
    problems
}

/// `path#heading`: the file exists and has a Markdown heading with exactly
/// that text.
fn check_procedure(repository: &Path, reference: &str) -> Result<(), String> {
    let (path, heading) = reference
        .split_once('#')
        .ok_or_else(|| format!("procedure {reference:?} is not path#heading"))?;
    let text = std::fs::read_to_string(repository.join(path))
        .map_err(|error| format!("procedure {path}: {error}"))?;
    let found = text.lines().any(|line| {
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        hashes > 0 && line[hashes..].starts_with(' ') && line[hashes..].trim() == heading
    });
    if found {
        Ok(())
    } else {
        Err(format!("procedure {path} has no heading {heading:?}"))
    }
}

/// `<AREA>-<NNN>`: one or more upper-case segments, then three digits.
pub fn check_id(id: &str) -> Result<(), String> {
    let segments: Vec<&str> = id.split('-').collect();
    let Some((number, area)) = segments.split_last() else {
        return Err(format!("ID {id:?} is not <AREA>-<NNN>"));
    };
    let number_ok = number.len() == 3 && number.bytes().all(|byte| byte.is_ascii_digit());
    let area_ok = !area.is_empty()
        && area.iter().all(|segment| {
            segment
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_uppercase())
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        });
    if number_ok && area_ok {
        Ok(())
    } else {
        Err(format!("ID {id:?} is not <AREA>-<NNN>"))
    }
}

fn requirement_files(repository: &Path) -> Result<Vec<PathBuf>, String> {
    let directory = repository.join(REQUIREMENTS);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(Vec::new());
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml" || extension == "yml")
        })
        .collect();
    files.sort();
    Ok(files)
}

fn relative(repository: &Path, path: &Path) -> String {
    path.strip_prefix(repository)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::check_id;

    #[test]
    fn ids_are_area_and_three_digits() {
        for good in ["SAFE-001", "INPUT-012", "CMD-INPUT-001", "IO2-100"] {
            assert!(check_id(good).is_ok(), "{good}");
        }
        for bad in [
            "W2",
            "safe-001",
            "SAFE-1",
            "SAFE-0001",
            "-001",
            "SAFE_001",
            "1AB-001",
        ] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
    }
}
