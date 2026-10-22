//! Computed status (design §2). Nothing here is ever stored.

use std::collections::BTreeMap;

use crate::schema::Requirement;

/// Ordered from best to worst, so the worst of several is their maximum.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Status {
    /// A pass exists whose identities match the current code.
    Verified,
    /// A pass exists, but what it covered has changed since.
    Stale,
    /// No evidence at all.
    Unverified,
    /// A fail stands against the current code.
    Failed,
}

impl Status {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Stale => "stale",
            Self::Unverified => "unverified",
            Self::Failed => "failed",
        }
    }
}

/// Where entry statuses come from: the evidence records
/// ([`crate::evidence::Recorded`]), or nothing at all ([`NoEvidence`]).
pub trait Evidence {
    fn entry(&self, requirement: &Requirement, entry: usize) -> Status;

    /// For a target-level entry, its status on each board with evidence.
    fn boards(&self, _requirement: &Requirement, _entry: usize) -> Vec<(String, Status)> {
        Vec::new()
    }
}

pub struct NoEvidence;

impl Evidence for NoEvidence {
    fn entry(&self, _: &Requirement, _: usize) -> Status {
        Status::Unverified
    }
}

/// Requirement statuses from entry statuses: the worst of a requirement's own
/// entries, and `unverified` without any. A parent is never proved through
/// its children, nor a child through its parent's run (design section 20).
pub fn compute(
    requirements: &BTreeMap<String, Requirement>,
    evidence: &dyn Evidence,
) -> BTreeMap<String, Status> {
    requirements
        .values()
        .map(|requirement| {
            let status = (0..requirement.verification.entries.len())
                .map(|entry| evidence.entry(requirement, entry))
                .max()
                .unwrap_or(Status::Unverified);
            (requirement.id.clone(), status)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Entry, Level, Method, Setup, Target, Verification};

    fn requirement(id: &str, parents: &[&str], entries: usize) -> Requirement {
        Requirement {
            id: id.into(),
            title: id.into(),
            statement: String::new(),
            rationale: String::new(),
            derived_from: parents.iter().map(|parent| (*parent).into()).collect(),
            verification: Verification {
                entries: (0..entries)
                    .map(|_| Entry {
                        level: Level::Unit,
                        target: Target::Host,
                        setup: Setup::None,
                        method: Method::Test("t".into()),
                    })
                    .collect(),
            },
        }
    }

    struct Fixed(BTreeMap<(String, usize), Status>);

    impl Evidence for Fixed {
        fn entry(&self, requirement: &Requirement, entry: usize) -> Status {
            self.0
                .get(&(requirement.id.clone(), entry))
                .copied()
                .unwrap_or(Status::Unverified)
        }
    }

    fn graph(list: Vec<Requirement>) -> BTreeMap<String, Requirement> {
        list.into_iter().map(|r| (r.id.clone(), r)).collect()
    }

    fn evidence(entries: &[(&str, usize, Status)]) -> Fixed {
        Fixed(
            entries
                .iter()
                .map(|(id, entry, status)| (((*id).into(), *entry), *status))
                .collect(),
        )
    }

    #[test]
    fn without_evidence_everything_is_unverified() {
        let requirements = graph(vec![
            requirement("A-001", &["W2"], 2),
            requirement("A-002", &["A-001"], 0),
        ]);
        let statuses = compute(&requirements, &NoEvidence);
        assert!(
            statuses
                .values()
                .all(|status| *status == Status::Unverified)
        );
    }

    #[test]
    fn a_requirement_takes_its_worst_entry() {
        let requirements = graph(vec![requirement("A-001", &["W2"], 2)]);
        let fixed = evidence(&[("A-001", 0, Status::Verified), ("A-001", 1, Status::Stale)]);
        assert_eq!(compute(&requirements, &fixed)["A-001"], Status::Stale);
        let fixed = evidence(&[("A-001", 0, Status::Verified), ("A-001", 1, Status::Failed)]);
        assert_eq!(compute(&requirements, &fixed)["A-001"], Status::Failed);
    }

    #[test]
    fn verified_children_do_not_verify_a_parent() {
        let requirements = graph(vec![
            requirement("P-001", &["W7"], 0),
            requirement("C-001", &["P-001"], 1),
        ]);
        let fixed = evidence(&[("C-001", 0, Status::Verified)]);
        let statuses = compute(&requirements, &fixed);
        assert_eq!(statuses["C-001"], Status::Verified);
        assert_eq!(statuses["P-001"], Status::Unverified);
    }
}
