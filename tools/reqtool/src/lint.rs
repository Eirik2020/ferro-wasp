//! What requirement text and the FAQ may hold. A requirement's statement is
//! one testable claim of what must hold, its rationale why it matters; the
//! FAQ answers "why not" for tempting alternatives, one short entry each.
//! Neither carries history, provenance, plans, evidence or paths into the
//! code: git, the PR, remaining work, the evidence records and the modules'
//! own declarations hold those.

use std::collections::BTreeSet;
use std::path::Path;

use crate::project::Project;
use crate::schema::Requirement;

pub const STATEMENT_MAX_WORDS: usize = 60;
pub const RATIONALE_MAX_WORDS: usize = 50;
pub const FAQ_ANSWER_MAX_WORDS: usize = 60;
/// The FAQ, relative to the repository root.
pub const FAQ: &str = "project_meta/faq.md";

/// Words and phrases that narrate history or name who decided.
const NARRATIVE: [&str; 12] = [
    "the user",
    "previously",
    "originally",
    "formerly",
    "at first",
    "was changed",
    "used to",
    "no longer",
    "earlier proposal",
    "was proposed",
    "was rejected",
    "was decided",
];

/// Every finding in the project's requirement text and its FAQ, each
/// prefixed with where it is.
pub fn project(project: &Project) -> Vec<String> {
    let mut findings = Vec::new();
    for requirement in project.requirements.values() {
        for finding in self::requirement(requirement) {
            findings.push(format!("{}: {finding}", requirement.id));
        }
    }
    match std::fs::read_to_string(project.repository.join(FAQ)) {
        Ok(text) => {
            let ids = project.requirements.keys().cloned().collect();
            for finding in faq(&text, &project.repository, &ids) {
                findings.push(format!("{FAQ}: {finding}"));
            }
        }
        Err(_) => findings.push(format!("{FAQ} is missing")),
    }
    findings
}

/// The findings for one requirement's text.
pub fn requirement(requirement: &Requirement) -> Vec<String> {
    let mut findings = Vec::new();
    let statement = &requirement.statement;
    let words = word_count(statement);
    if words > STATEMENT_MAX_WORDS {
        findings.push(format!(
            "statement is {words} words; at most {STATEMENT_MAX_WORDS}"
        ));
    }
    let claims = shall_count(statement);
    if claims != 1 {
        findings.push(format!(
            "statement has {claims} \"shall\"; one claim each, so split it"
        ));
    }
    let words = word_count(&requirement.rationale);
    if words > RATIONALE_MAX_WORDS {
        findings.push(format!(
            "rationale is {words} words; at most {RATIONALE_MAX_WORDS}"
        ));
    }
    for (field, text) in [
        ("title", &requirement.title),
        ("statement", statement),
        ("rationale", &requirement.rationale),
    ] {
        for finding in forbidden(text) {
            findings.push(format!("{field}: {finding}"));
        }
    }
    findings
}

/// What text in a requirement or a FAQ answer may not hold.
fn forbidden(text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let lower = text.to_lowercase();
    for phrase in NARRATIVE {
        if contains_phrase(&lower, phrase) {
            findings.push(format!("narrates history or provenance (\"{phrase}\")"));
        }
    }
    if has_date(text) {
        findings.push("holds a date".into());
    }
    if has_numbered(text, "ADR-") || has_numbered(text, "#") {
        findings.push("names an ADR or a PR".into());
    }
    if text.contains("::") || text.contains(".rs") || text.contains("src/") {
        findings.push("names a path into the code; modules declare their requirements".into());
    }
    findings
}

/// The findings for the FAQ's text, given the requirement IDs that exist.
/// The format: a `#` title and a preamble, then `##` topic headings, each
/// holding `### Why not …?` entries of one paragraph and a closing
/// `See: …` line naming a requirement or a document that exists.
pub fn faq(text: &str, repository: &Path, requirements: &BTreeSet<String>) -> Vec<String> {
    let mut findings = Vec::new();
    let mut entry: Option<Entry> = None;
    let mut in_topic = false;
    for (number, line) in text.lines().enumerate() {
        let number = number + 1;
        if let Some(heading) = line.strip_prefix("### ") {
            if let Some(entry) = entry.take() {
                findings.extend(entry.finish(repository, requirements));
            }
            if !in_topic {
                findings.push(format!("line {number}: an entry outside a `##` topic"));
            }
            if !(heading.starts_with("Why not ") && heading.ends_with('?')) {
                findings.push(format!(
                    "line {number}: an entry's heading is \"Why not …?\", got \"{heading}\""
                ));
            }
            entry = Some(Entry::new(number));
        } else if line.starts_with("## ") {
            if let Some(entry) = entry.take() {
                findings.extend(entry.finish(repository, requirements));
            }
            in_topic = true;
        } else if line.starts_with('#') && number > 1 {
            findings.push(format!(
                "line {number}: only `##` topics and `###` entries follow the title"
            ));
        } else if let Some(entry) = entry.as_mut() {
            entry.push(number, line, &mut findings);
        }
    }
    if let Some(entry) = entry {
        findings.extend(entry.finish(repository, requirements));
    }
    findings
}

struct Entry {
    heading_line: usize,
    answer: Vec<String>,
    paragraphs: usize,
    in_paragraph: bool,
    see: Option<(usize, String)>,
}

impl Entry {
    fn new(heading_line: usize) -> Self {
        Self {
            heading_line,
            answer: Vec::new(),
            paragraphs: 0,
            in_paragraph: false,
            see: None,
        }
    }

    fn push(&mut self, number: usize, line: &str, findings: &mut Vec<String>) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            self.in_paragraph = false;
            return;
        }
        if self.see.is_some() {
            findings.push(format!(
                "line {number}: nothing follows an entry's `See:` line"
            ));
            return;
        }
        if let Some(see) = trimmed.strip_prefix("See: ") {
            self.see = Some((number, see.to_string()));
            return;
        }
        if trimmed.starts_with(['-', '*', '|', '>', '`']) || trimmed.starts_with("1.") {
            findings.push(format!(
                "line {number}: an answer is one plain paragraph: no lists, tables, quotes or code"
            ));
        }
        if !self.in_paragraph {
            self.paragraphs += 1;
            self.in_paragraph = true;
        }
        self.answer.push(trimmed.to_string());
    }

    fn finish(self, repository: &Path, requirements: &BTreeSet<String>) -> Vec<String> {
        let mut findings = Vec::new();
        let at = self.heading_line;
        let answer = self.answer.join(" ");
        if self.paragraphs != 1 {
            findings.push(format!(
                "entry at line {at}: the answer is one paragraph, got {}",
                self.paragraphs
            ));
        }
        let words = word_count(&answer);
        if words > FAQ_ANSWER_MAX_WORDS {
            findings.push(format!(
                "entry at line {at}: the answer is {words} words; at most {FAQ_ANSWER_MAX_WORDS}"
            ));
        }
        for finding in forbidden(&answer) {
            findings.push(format!("entry at line {at}: {finding}"));
        }
        match self.see {
            None => findings.push(format!(
                "entry at line {at}: ends with a `See:` line naming where the decision is kept"
            )),
            Some((number, see)) => {
                let references: Vec<&str> = see
                    .split(',')
                    .map(str::trim)
                    .filter(|reference| !reference.is_empty())
                    .collect();
                if references.is_empty() {
                    findings.push(format!("line {number}: `See:` names nothing"));
                }
                for reference in references {
                    if let Some(problem) = unresolved(reference, repository, requirements) {
                        findings.push(format!("line {number}: {problem}"));
                    }
                }
            }
        }
        findings
    }
}

/// Why a `See:` reference does not resolve: a requirement ID that does not
/// exist, or a Markdown link to a file that does not.
fn unresolved(
    reference: &str,
    repository: &Path,
    requirements: &BTreeSet<String>,
) -> Option<String> {
    if let Some(link) = reference
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("]("))
        .and_then(|(_, target)| target.strip_suffix(')'))
    {
        let file = link.split('#').next().unwrap_or(link);
        let path = repository.join("project_meta").join(file);
        return (!path.is_file()).then(|| format!("links to {file}, which does not exist"));
    }
    (!requirements.contains(reference))
        .then(|| format!("{reference} is neither a requirement nor a Markdown link"))
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn shall_count(text: &str) -> usize {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.eq_ignore_ascii_case("shall"))
        .count()
}

fn contains_phrase(lower: &str, phrase: &str) -> bool {
    lower.match_indices(phrase).any(|(start, _)| {
        let before = lower[..start].chars().next_back();
        let after = lower[start + phrase.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// A `YYYY-MM-DD` date.
fn has_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(10).any(|w| {
        w[..4].iter().all(u8::is_ascii_digit)
            && w[4] == b'-'
            && w[5..7].iter().all(u8::is_ascii_digit)
            && w[7] == b'-'
            && w[8..].iter().all(u8::is_ascii_digit)
    })
}

/// `prefix` followed directly by a digit, as in `ADR-0042` or `#58`.
fn has_numbered(text: &str, prefix: &str) -> bool {
    text.match_indices(prefix).any(|(start, _)| {
        text[start + prefix.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(statement: &str, rationale: &str) -> Requirement {
        serde_saphyr::from_str(&format!(
            "id: X-001\ntitle: A title\nstatement: >-\n  {statement}\nrationale: >-\n  {rationale}\n\
             derived_from: [W2]\nverification:\n  entries: []\n"
        ))
        .unwrap()
    }

    #[test]
    fn a_clean_requirement_has_no_findings() {
        let clean = with(
            "The parser shall reject a frame whose CRC does not match.",
            "A corrupt frame read as valid would command the motors.",
        );
        assert!(requirement(&clean).is_empty());
    }

    #[test]
    fn length_and_claim_count_are_limited() {
        let long = "word ".repeat(STATEMENT_MAX_WORDS);
        let findings = requirement(&with(
            &format!("It shall hold and it shall also hold {long}"),
            &"why ".repeat(RATIONALE_MAX_WORDS + 1),
        ));
        assert!(findings.iter().any(|f| f.starts_with("statement is")));
        assert!(findings.iter().any(|f| f.contains("2 \"shall\"")));
        assert!(findings.iter().any(|f| f.starts_with("rationale is")));
    }

    #[test]
    fn history_provenance_and_paths_are_refused() {
        let findings = requirement(&with(
            "The link shall never block, as the user chose on 2026-10-17 in ADR-0055.",
            "It previously lived in snapshots.rs and #58 moved it to ferrowasp_border::observer.",
        ));
        let has = |needle: &str| findings.iter().any(|f| f.contains(needle));
        assert!(has("\"the user\""));
        assert!(has("a date"));
        assert!(has("an ADR or a PR"));
        assert!(has("\"previously\""));
        assert!(has("a path into the code"));
    }

    #[test]
    fn a_word_inside_another_is_not_narration() {
        assert!(!contains_phrase("refused to used tool", "used to"));
        assert!(contains_phrase("it used to hold", "used to"));
        assert!(!contains_phrase("theuser", "the user"));
    }

    const GOOD_FAQ: &str = "# FAQ\n\nWhy the obvious alternatives lost.\n\n## Links\n\n\
        ### Why not one shared channel?\n\nOne requester could fill it for every other, and the \
        sender would have to name itself.\n\nSee: X-001\n";

    #[test]
    fn a_well_formed_faq_has_no_findings() {
        let ids = BTreeSet::from(["X-001".to_string()]);
        assert!(faq(GOOD_FAQ, Path::new("/nonexistent"), &ids).is_empty());
    }

    #[test]
    fn a_faq_entry_keeps_its_shape() {
        let ids = BTreeSet::from(["X-001".to_string()]);
        let bad = "# FAQ\n\n### Why not early?\n\nOrphan.\n\nSee: X-001\n\n## Links\n\n\
            ### Should we use a channel\n\n- a list\n\nAnd a second paragraph.\n\n\
            ### Why not later?\n\nNo reference here.\n";
        let findings = faq(bad, Path::new("/nonexistent"), &ids);
        let has = |needle: &str| findings.iter().any(|f| f.contains(needle));
        assert!(has("outside a `##` topic"));
        assert!(has("\"Why not …?\""));
        assert!(has("no lists"));
        assert!(has("one paragraph, got 2"));
        assert!(has("ends with a `See:` line"));
    }

    #[test]
    fn a_faq_reference_must_resolve() {
        let ids = BTreeSet::from(["X-001".to_string()]);
        let text = "# FAQ\n\n## Links\n\n### Why not a mailbox?\n\nIt keeps only the newest.\n\n\
            See: X-002, [missing](nowhere.md)\n";
        let findings = faq(text, Path::new("/nonexistent"), &ids);
        assert!(findings.iter().any(|f| f.contains("X-002 is neither")));
        assert!(findings.iter().any(|f| f.contains("nowhere.md")));
    }
}
