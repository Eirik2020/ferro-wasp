//! Computed status (ADR-0050). Nothing here is ever stored.

use std::collections::BTreeMap;

use crate::schema::{Mode, Requirement};

/// Ordered from best to worst, so the worst of several is their maximum.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Status {
    /// A pass exists whose fingerprint matches the current code.
    Verified,
    /// A pass exists, but its fingerprint no longer matches.
    Stale,
    /// No evidence at all.
    Unverified,
    /// The newest evidence at the current fingerprint is a fail.
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

/// Where entry statuses come from. Evidence records are not read yet, so the
/// only source today reports every entry unverified.
pub trait Evidence {
    fn entry(&self, requirement: &Requirement, entry: usize) -> Status;
}

pub struct NoEvidence;

impl Evidence for NoEvidence {
    fn entry(&self, _: &Requirement, _: usize) -> Status {
        Status::Unverified
    }
}

/// Requirement statuses from entry statuses, following each mode:
/// - `direct`: the worst of its own entries;
/// - `via_children`: the worst of its children;
/// - `covered_by_parent`: the worst of its parent requirements;
/// - `either`: the better of `direct` and `via_children`, except that a
///   failing own entry is never outvoted by passing children.
///
/// A child covered by its parent takes its status from that parent, so it
/// never counts towards the parent's own status.
///
/// The requirement graph should already be validated. A loop that slips
/// through reads as `unverified` rather than recursing forever.
pub fn compute(
    requirements: &BTreeMap<String, Requirement>,
    evidence: &dyn Evidence,
) -> BTreeMap<String, Status> {
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for requirement in requirements.values() {
        if requirement.verification.mode == Mode::CoveredByParent {
            continue;
        }
        for parent in &requirement.derived_from {
            if requirements.contains_key(parent) {
                children
                    .entry(parent.as_str())
                    .or_default()
                    .push(&requirement.id);
            }
        }
    }
    let mut computed = BTreeMap::new();
    for id in requirements.keys() {
        status_of(id, requirements, &children, evidence, &mut computed);
    }
    computed
}

fn status_of(
    id: &str,
    requirements: &BTreeMap<String, Requirement>,
    children: &BTreeMap<&str, Vec<&str>>,
    evidence: &dyn Evidence,
    computed: &mut BTreeMap<String, Status>,
) -> Status {
    if let Some(status) = computed.get(id) {
        return *status;
    }
    computed.insert(id.to_owned(), Status::Unverified);
    let requirement = &requirements[id];
    let own = || {
        (0..requirement.verification.entries.len())
            .map(|entry| evidence.entry(requirement, entry))
            .max()
            .unwrap_or(Status::Unverified)
    };
    let from = |related: &[&str], computed: &mut BTreeMap<String, Status>| {
        related
            .iter()
            .map(|other| status_of(other, requirements, children, evidence, computed))
            .max()
            .unwrap_or(Status::Unverified)
    };
    let status = match requirement.verification.mode {
        Mode::Direct => own(),
        Mode::ViaChildren => from(children.get(id).map_or(&[][..], Vec::as_slice), computed),
        Mode::CoveredByParent => {
            let parents: Vec<&str> = requirement
                .derived_from
                .iter()
                .map(String::as_str)
                .filter(|parent| requirements.contains_key(*parent))
                .collect();
            from(&parents, computed)
        }
        Mode::Either => {
            let mine = own();
            let below = from(children.get(id).map_or(&[][..], Vec::as_slice), computed);
            if mine == Status::Failed {
                mine
            } else {
                mine.min(below)
            }
        }
    };
    computed.insert(id.to_owned(), status);
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Entry, Level, Method, Setup, Target, Verification};

    fn requirement(id: &str, parents: &[&str], mode: Mode, entries: usize) -> Requirement {
        Requirement {
            id: id.into(),
            title: id.into(),
            statement: String::new(),
            rationale: String::new(),
            derived_from: parents.iter().map(|parent| (*parent).into()).collect(),
            allocated_to: Vec::new(),
            verification: Verification {
                mode,
                reason: None,
                observed_by: None,
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
            requirement("A-001", &["W2"], Mode::Direct, 2),
            requirement("A-002", &["A-001"], Mode::CoveredByParent, 0),
        ]);
        let statuses = compute(&requirements, &NoEvidence);
        assert!(
            statuses
                .values()
                .all(|status| *status == Status::Unverified)
        );
    }

    #[test]
    fn direct_takes_the_worst_entry() {
        let requirements = graph(vec![requirement("A-001", &["W2"], Mode::Direct, 2)]);
        let fixed = evidence(&[("A-001", 0, Status::Verified), ("A-001", 1, Status::Stale)]);
        assert_eq!(compute(&requirements, &fixed)["A-001"], Status::Stale);
        let fixed = evidence(&[("A-001", 0, Status::Verified), ("A-001", 1, Status::Failed)]);
        assert_eq!(compute(&requirements, &fixed)["A-001"], Status::Failed);
    }

    #[test]
    fn via_children_needs_every_child() {
        let requirements = graph(vec![
            requirement("P-001", &["W7"], Mode::ViaChildren, 0),
            requirement("C-001", &["P-001"], Mode::Direct, 1),
            requirement("C-002", &["P-001"], Mode::Direct, 1),
        ]);
        let one = evidence(&[("C-001", 0, Status::Verified)]);
        assert_eq!(compute(&requirements, &one)["P-001"], Status::Unverified);
        let both = evidence(&[
            ("C-001", 0, Status::Verified),
            ("C-002", 0, Status::Verified),
        ]);
        assert_eq!(compute(&requirements, &both)["P-001"], Status::Verified);
    }

    #[test]
    fn covered_by_parent_follows_the_parent() {
        let requirements = graph(vec![
            requirement("P-001", &["W7"], Mode::Direct, 1),
            requirement("C-001", &["P-001"], Mode::CoveredByParent, 0),
        ]);
        let fixed = evidence(&[("P-001", 0, Status::Verified)]);
        assert_eq!(compute(&requirements, &fixed)["C-001"], Status::Verified);
    }

    #[test]
    fn either_takes_the_better_of_own_and_children() {
        let requirements = graph(vec![
            requirement("P-001", &["W7"], Mode::Either, 1),
            requirement("C-001", &["P-001"], Mode::Direct, 1),
        ]);
        let by_children = evidence(&[("C-001", 0, Status::Verified)]);
        assert_eq!(
            compute(&requirements, &by_children)["P-001"],
            Status::Verified
        );
        let by_own = evidence(&[("P-001", 0, Status::Verified)]);
        assert_eq!(compute(&requirements, &by_own)["P-001"], Status::Verified);
    }

    #[test]
    fn a_child_covered_by_its_parent_does_not_verify_the_parent() {
        let requirements = graph(vec![
            requirement("P-001", &["W7"], Mode::ViaChildren, 0),
            requirement("C-001", &["P-001"], Mode::Direct, 1),
            requirement("C-002", &["P-001"], Mode::CoveredByParent, 0),
        ]);
        let fixed = evidence(&[("C-001", 0, Status::Verified)]);
        let statuses = compute(&requirements, &fixed);
        assert_eq!(statuses["P-001"], Status::Verified);
        assert_eq!(statuses["C-002"], Status::Verified);
    }

    #[test]
    fn either_never_lets_children_outvote_an_own_failure() {
        let requirements = graph(vec![
            requirement("P-001", &["W7"], Mode::Either, 1),
            requirement("C-001", &["P-001"], Mode::Direct, 1),
        ]);
        let fixed = evidence(&[("P-001", 0, Status::Failed), ("C-001", 0, Status::Verified)]);
        assert_eq!(compute(&requirements, &fixed)["P-001"], Status::Failed);
    }
}
