//! Identities (design section 20): what evidence covered, derived from the
//! code and the requirements and never listed by hand.
//!
//! - A Rust source file's identity is a SHA-256 over its tokens, so
//!   whitespace and comments, doc comments included, leave it unchanged.
//!   Any other file is hashed as bytes.
//! - A claim is what one verification entry asserts: the requirement's ID and
//!   statement, the entry's level, target, setup and method, and for a
//!   procedure the text of its section.
//! - The toolchain is the nearest `rust-toolchain.toml`.

use std::fmt::Write as _;
use std::path::Path;
use std::str::FromStr;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use sha2::{Digest, Sha256};

use crate::schema::{Entry, Method, Requirement};

/// The identity of a Rust source file's text: its tokens without comments.
pub fn source(text: &str) -> Result<String, String> {
    let tokens = TokenStream::from_str(text).map_err(|error| error.to_string())?;
    let mut normal = String::new();
    write_tokens(tokens, &mut normal);
    Ok(sha256(normal.as_bytes()))
}

/// The identity of any file: its tokens for a `.rs` file, its bytes
/// otherwise.
pub fn file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if path.extension().is_some_and(|extension| extension == "rs") {
        let text = String::from_utf8(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        source(&text).map_err(|e| format!("{}: {e}", path.display()))
    } else {
        Ok(sha256(&bytes))
    }
}

/// One token per word, doc attributes left out. A punctuation mark records
/// whether it joins the next, so `->` and `- >` stay distinct.
fn write_tokens(tokens: TokenStream, out: &mut String) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut index = 0;
    while index < trees.len() {
        if let Some(length) = doc_attribute(&trees[index..]) {
            index += length;
            continue;
        }
        match &trees[index] {
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                out.push_str(open);
                out.push(' ');
                write_tokens(group.stream(), out);
                out.push_str(close);
            }
            TokenTree::Ident(ident) => {
                let _ = write!(out, "{ident}");
            }
            TokenTree::Punct(punct) => {
                out.push(punct.as_char());
                if punct.spacing() == Spacing::Joint {
                    out.push('~');
                }
            }
            TokenTree::Literal(literal) => {
                let _ = write!(out, "{literal}");
            }
        }
        out.push(' ');
        index += 1;
    }
}

/// `# [doc = ...]` or `# ! [doc = ...]`, as the lexer turns doc comments:
/// how many trees it spans, if it starts here.
fn doc_attribute(trees: &[TokenTree]) -> Option<usize> {
    let TokenTree::Punct(hash) = trees.first()? else {
        return None;
    };
    if hash.as_char() != '#' {
        return None;
    }
    let inner = matches!(trees.get(1), Some(TokenTree::Punct(bang)) if bang.as_char() == '!');
    let at = if inner { 2 } else { 1 };
    let TokenTree::Group(group) = trees.get(at)? else {
        return None;
    };
    let is_doc = group.delimiter() == Delimiter::Bracket
        && matches!(group.stream().into_iter().next(), Some(TokenTree::Ident(ident)) if ident == "doc");
    is_doc.then_some(at + 1)
}

/// What one verification entry claims. Whitespace in the statement is
/// collapsed, so re-wrapping the YAML leaves the claim alone.
pub fn claim(
    repository: &Path,
    requirement: &Requirement,
    entry: &Entry,
) -> Result<String, String> {
    let squash = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut text = String::new();
    let _ = writeln!(text, "id {}", requirement.id);
    let _ = writeln!(text, "statement {}", squash(&requirement.statement));
    let method = match &entry.method {
        Method::Test(path) => format!("test {path}"),
        Method::Procedure(reference) => format!(
            "procedure {reference} {}",
            sha256(procedure_section(repository, reference)?.as_bytes())
        ),
    };
    let _ = writeln!(
        text,
        "entry {} {} {} {method}",
        entry.level.name(),
        entry.target.name(),
        entry.setup.name()
    );
    Ok(sha256(text.as_bytes()))
}

/// The procedure's section: its heading line up to the next heading of the
/// same or a higher level.
pub fn procedure_section(repository: &Path, reference: &str) -> Result<String, String> {
    let (path, heading) = reference
        .split_once('#')
        .ok_or_else(|| format!("procedure {reference:?} is not path#heading"))?;
    let text = std::fs::read_to_string(repository.join(path))
        .map_err(|e| format!("procedure {path}: {e}"))?;
    let level = |line: &str| {
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        (hashes > 0 && line[hashes..].starts_with(' ')).then_some(hashes)
    };
    let mut section = String::new();
    let mut depth = None;
    for line in text.lines() {
        match (depth, level(line)) {
            (None, Some(found)) if line[found..].trim() == heading => depth = Some(found),
            (Some(own), Some(found)) if found <= own => break,
            _ => {}
        }
        if depth.is_some() {
            section.push_str(line.trim_end());
            section.push('\n');
        }
    }
    if depth.is_none() {
        return Err(format!("procedure {path} has no heading {heading:?}"));
    }
    Ok(section)
}

/// The nearest `rust-toolchain.toml` at or above `directory`, up to the
/// repository root, or `none`.
pub fn toolchain(repository: &Path, directory: &Path) -> Result<String, String> {
    for candidate in directory.ancestors() {
        let file = candidate.join("rust-toolchain.toml");
        if file.is_file() {
            return file_bytes(&file);
        }
        if candidate == repository {
            break;
        }
    }
    Ok("none".to_owned())
}

fn file_bytes(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| sha256(&bytes))
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::source;

    #[test]
    fn layout_and_comments_do_not_change_a_source_identity() {
        let original = "/// Adds.\nfn add(a: u8) -> u8 { a + 1 }\n";
        let reformatted =
            "//! A module.\n\n// Why.\nfn add( a : u8 )\n    -> u8 {\n    /* inline */ a + 1\n}\n";
        assert_eq!(source(original).unwrap(), source(reformatted).unwrap());
    }

    #[test]
    fn a_token_change_does_change_it() {
        let base = source("fn f() -> u8 { 1 }").unwrap();
        assert_ne!(base, source("fn f() -> u8 { 2 }").unwrap());
        assert_ne!(base, source("fn g() -> u8 { 1 }").unwrap());
        assert_ne!(
            source("fn f() { a - > b }").unwrap(),
            source("fn f() { a -> b }").unwrap()
        );
        assert_ne!(
            source("#[inline] fn f() {}").unwrap(),
            source("fn f() {}").unwrap(),
            "only doc attributes are left out"
        );
    }
}
