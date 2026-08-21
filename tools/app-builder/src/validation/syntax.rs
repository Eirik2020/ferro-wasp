use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::validation::render::RenderedCrate;

pub fn validate_rendered_rust(rendered: &RenderedCrate) -> Result<()> {
    for (path, contents) in &rendered.files {
        if path.extension().and_then(|part| part.to_str()) != Some("rs") {
            continue;
        }
        let source = std::str::from_utf8(contents)
            .with_context(|| format!("rendered Rust file {} is not UTF-8", path.display()))?;
        parse_file(path, source)?;
    }
    Ok(())
}

pub fn validate_rust_tree(root: &Path) -> Result<()> {
    visit(root, root)
}

fn visit(root: &Path, path: &Path) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("read directory {}", path.display()))? {
        let entry = entry.with_context(|| format!("read entry below {}", path.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type for {}", entry.path().display()))?;
        if file_type.is_symlink() {
            bail!(
                "symlinks are not accepted in rendered crates: {}",
                entry.path().display()
            );
        }
        if file_type.is_dir() {
            visit(root, &entry.path())?;
        } else if entry.path().extension().and_then(|part| part.to_str()) == Some("rs") {
            let source = fs::read_to_string(entry.path())
                .with_context(|| format!("read generated Rust {}", entry.path().display()))?;
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(&entry.path())
                .to_path_buf();
            parse_file(&relative, &source)?;
        }
    }
    Ok(())
}

fn parse_file(path: &Path, source: &str) -> Result<()> {
    syn::parse_file(source)
        .map(|_| ())
        .with_context(|| format!("parse generated Rust {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_rendered_rust_files() {
        let mut rendered = RenderedCrate::default();
        rendered.insert_text("src/main.rs", "fn main() {}\n".to_owned());
        rendered.insert_text("build.rs", "fn main() {}\n".to_owned());
        assert!(validate_rendered_rust(&rendered).is_ok());
        rendered.insert_text("src/main.rs", "fn {\n".to_owned());
        assert!(validate_rendered_rust(&rendered).is_err());
    }
}
