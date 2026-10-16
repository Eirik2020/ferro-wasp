//! The governing requirements W1-W7, read from their owning document.
//!
//! The roots stay prose in `project_meta/governing-requirements.md`; this
//! reads only their IDs and titles, so the document remains the one owner.

use std::path::Path;

pub const GOVERNING_REQUIREMENTS: &str = "project_meta/governing-requirements.md";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Root {
    pub id: String,
    pub title: String,
}

/// Reads every `N. **Wn - Title.**` line of the governing requirements.
pub fn load(repository: &Path) -> Result<Vec<Root>, String> {
    let path = repository.join(GOVERNING_REQUIREMENTS);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("{GOVERNING_REQUIREMENTS}: {error}"))?;
    let roots = parse(&text);
    if roots.is_empty() {
        return Err(format!(
            "{GOVERNING_REQUIREMENTS}: no governing requirement found"
        ));
    }
    Ok(roots)
}

pub fn parse(text: &str) -> Vec<Root> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Root> {
    let (number, rest) = line.trim_start().split_once(". **W")?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (digits, rest) = rest.split_once(" - ")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (title, _) = rest.split_once(".**")?;
    Some(Root {
        id: format!("W{digits}"),
        title: title.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbered_roots_and_ignores_other_lines() {
        let text = "# Governing Requirements\n\
                    \n\
                    1. **W1 - Motor authority.** Only the safety master.\n\
                    2. **W2 - Fail closed.** Stale data.\n\
                    **Authority.** Not a root.\n\
                    - **W9 - Not numbered.** Ignored.\n";
        assert_eq!(
            parse(text),
            vec![
                Root {
                    id: "W1".into(),
                    title: "Motor authority".into()
                },
                Root {
                    id: "W2".into(),
                    title: "Fail closed".into()
                },
            ]
        );
    }
}
