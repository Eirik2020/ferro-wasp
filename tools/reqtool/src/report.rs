//! What `reqtool check` prints: compact, one fact per line, for agents first.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::items::Resolver;
use crate::project::Project;
use crate::schema::{Method, Mode, Requirement};
use crate::status::{self, Evidence, Status};

/// The whole project: a count per status, then one line per requirement.
pub fn summary(project: &Project, evidence: &dyn Evidence) -> String {
    let statuses = status::compute(&project.requirements, evidence);
    let mut counts: BTreeMap<Status, usize> = BTreeMap::new();
    for status in statuses.values() {
        *counts.entry(*status).or_default() += 1;
    }
    let mut out = format!("{} requirements", project.requirements.len());
    for (status, count) in &counts {
        let _ = write!(out, ", {count} {}", status.name());
    }
    let unmethoded = project.requirements.values().filter(|r| has_no_method(r)).count();
    if unmethoded > 0 {
        let _ = write!(out, "; {unmethoded} with no verification method yet");
    }
    out.push('\n');
    for requirement in project.requirements.values() {
        let gap = if has_no_method(requirement) { "  (no method)" } else { "" };
        let _ = writeln!(
            out,
            "{:<16} {:<10} {}{gap}",
            requirement.id,
            statuses[&requirement.id].name(),
            requirement.title
        );
    }
    out
}

/// A `direct` requirement with no entries: nothing verifies it yet.
fn has_no_method(requirement: &Requirement) -> bool {
    requirement.verification.mode == Mode::Direct && requirement.verification.entries.is_empty()
}

/// One requirement by ID, or every requirement allocated to, inside, or
/// around an item path.
pub fn check(project: &Project, target: &str, evidence: &dyn Evidence) -> Result<String, String> {
    let statuses = status::compute(&project.requirements, evidence);
    if let Some(requirement) = project.requirements.get(target) {
        return Ok(detail(project, requirement, &statuses, evidence));
    }
    if project.root(target).is_some() {
        let children = project.children(target);
        let mut out = format!(
            "{target} is a governing requirement; {} derive from it\n",
            children.len()
        );
        for child in children {
            let _ = writeln!(
                out,
                "{:<16} {:<10} {}",
                child.id,
                statuses[&child.id].name(),
                child.title
            );
        }
        return Ok(out);
    }
    Resolver::new(&project.workspace.crates).resolve(target)?;
    let related: Vec<&Requirement> = project
        .requirements
        .values()
        .filter(|requirement| {
            requirement
                .allocated_to
                .iter()
                .any(|path| overlaps(path, target))
        })
        .collect();
    let crate_name = target.split("::").next().unwrap_or(target);
    let critical = project
        .workspace
        .crates
        .get(crate_name)
        .is_some_and(|found| found.critical);
    if related.is_empty() {
        let mut out = format!("no requirement is allocated to {target} or around it\n");
        if critical {
            let _ = writeln!(
                out,
                "{crate_name} is safety-critical: its items must trace to a requirement"
            );
        }
        return Ok(out);
    }
    let mut out = String::new();
    for requirement in related {
        out.push_str(&detail(project, requirement, &statuses, evidence));
    }
    Ok(out)
}

fn detail(
    project: &Project,
    requirement: &Requirement,
    statuses: &BTreeMap<String, Status>,
    evidence: &dyn Evidence,
) -> String {
    let mut out = format!(
        "{}  {}  {}\n  statement: {}\n",
        requirement.id,
        statuses[&requirement.id].name(),
        requirement.title,
        requirement
            .statement
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    );
    let parents: Vec<String> = requirement
        .derived_from
        .iter()
        .map(|parent| match project.root(parent) {
            Some(root) => format!("{parent} ({})", root.title),
            None => parent.clone(),
        })
        .collect();
    let _ = writeln!(out, "  derived from: {}", parents.join(", "));
    if !requirement.allocated_to.is_empty() {
        let _ = writeln!(
            out,
            "  allocated to: {}",
            requirement.allocated_to.join(", ")
        );
    }
    let children: Vec<&str> = project
        .children(&requirement.id)
        .iter()
        .map(|child| child.id.as_str())
        .collect();
    if !children.is_empty() {
        let _ = writeln!(out, "  children: {}", children.join(", "));
    }
    let verification = &requirement.verification;
    let _ = write!(out, "  verification: {}", verification.mode.name());
    if let Some(observed) = &verification.observed_by {
        let _ = write!(out, ", observed by {observed}");
    }
    out.push('\n');
    if has_no_method(requirement) {
        out.push_str("    no verification method yet\n");
    }
    for (index, entry) in verification.entries.iter().enumerate() {
        let method = match &entry.method {
            Method::Test(path) => format!("test {path}"),
            Method::Procedure(reference) => format!("procedure {reference}"),
        };
        let _ = writeln!(
            out,
            "    {:<8} {:<8} {:<9} {:<10} {method}",
            entry.level.name(),
            entry.target.name(),
            entry.setup.name(),
            evidence.entry(requirement, index).name()
        );
        for (board, status) in evidence.boards(requirement, index) {
            let _ = writeln!(out, "      {board}: {}", status.name());
        }
    }
    out
}

/// One path names the other or something inside it.
fn overlaps(a: &str, b: &str) -> bool {
    let inside =
        |outer: &str, inner: &str| inner == outer || inner.starts_with(&format!("{outer}::"));
    inside(a, b) || inside(b, a)
}

#[cfg(test)]
mod tests {
    use super::overlaps;

    #[test]
    fn overlap_is_by_whole_path_segments() {
        assert!(overlaps("a::b", "a::b"));
        assert!(overlaps("a::b", "a::b::c"));
        assert!(overlaps("a::b::c", "a"));
        assert!(!overlaps("a::bc", "a::b"));
        assert!(!overlaps("a::b", "x::b"));
    }
}
