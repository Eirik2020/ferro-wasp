use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const FEATURE_METADATA_FILE: &str = "feature.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureMetadata {
    pub schema_version: u64,
    pub id: String,
    pub imports: String,
    pub shared_resources: String,
    pub local_resources: String,
    pub init: String,
    pub shared_constructors: String,
    pub local_constructors: String,
    pub tasks: String,
    pub required_placeholders: Vec<String>,
    pub claimed_symbols: Vec<String>,
    pub required_symbols: Vec<String>,
    pub claimed_resources: Vec<String>,
    pub claimed_interrupts: Vec<String>,
    pub insertion_after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureFragments {
    pub imports: String,
    pub shared_resources: String,
    pub local_resources: String,
    pub init: String,
    pub shared_constructors: String,
    pub local_constructors: String,
    pub tasks: String,
}

impl FeatureFragments {
    pub fn iter(&self) -> [(&'static str, &str); 7] {
        [
            ("imports", &self.imports),
            ("shared_resources", &self.shared_resources),
            ("local_resources", &self.local_resources),
            ("init", &self.init),
            ("shared_constructors", &self.shared_constructors),
            ("local_constructors", &self.local_constructors),
            ("tasks", &self.tasks),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureBundle {
    pub implementation: String,
    pub directory: PathBuf,
    pub metadata: FeatureMetadata,
    pub fragments: FeatureFragments,
    pub fingerprint: String,
}

/// Loads one feature implementation below `feature_library_root`.
///
/// Both the implementation path and every fragment path are confined to their
/// respective roots after canonicalization. This also prevents a symlink from
/// turning an otherwise relative path into an escape from the feature library.
pub fn load_feature_bundle(
    feature_library_root: &Path,
    implementation: &str,
) -> Result<FeatureBundle> {
    let implementation_path = validate_relative_path(implementation, "feature implementation")?;
    let canonical_root = fs::canonicalize(feature_library_root).with_context(|| {
        format!(
            "failed to resolve feature-library root `{}`",
            feature_library_root.display()
        )
    })?;
    let directory =
        fs::canonicalize(canonical_root.join(implementation_path)).with_context(|| {
            format!(
                "feature implementation `{implementation}` does not exist below `{}`",
                feature_library_root.display()
            )
        })?;

    ensure_confined(&directory, &canonical_root, "feature implementation")?;
    if !directory.is_dir() {
        bail!("feature implementation `{implementation}` is not a directory");
    }

    let metadata_path = directory.join(FEATURE_METADATA_FILE);
    let metadata_bytes = fs::read(&metadata_path).with_context(|| {
        format!(
            "failed to read feature metadata `{}`",
            metadata_path.display()
        )
    })?;
    let metadata_source = std::str::from_utf8(&metadata_bytes).with_context(|| {
        format!(
            "feature metadata `{}` is not valid UTF-8",
            metadata_path.display()
        )
    })?;
    let metadata: FeatureMetadata = toml::from_str(metadata_source).with_context(|| {
        format!(
            "failed to parse strict feature metadata `{}`",
            metadata_path.display()
        )
    })?;
    validate_metadata(&metadata, &metadata_path)?;

    let fragment_specs = [
        ("imports", metadata.imports.as_str()),
        ("shared_resources", metadata.shared_resources.as_str()),
        ("local_resources", metadata.local_resources.as_str()),
        ("init", metadata.init.as_str()),
        ("shared_constructors", metadata.shared_constructors.as_str()),
        ("local_constructors", metadata.local_constructors.as_str()),
        ("tasks", metadata.tasks.as_str()),
    ];

    let mut fragment_names = BTreeSet::new();
    let mut loaded = Vec::with_capacity(fragment_specs.len());
    for (kind, relative_name) in fragment_specs {
        if !fragment_names.insert(relative_name) {
            bail!(
                "feature `{}` uses fragment `{relative_name}` for more than one insertion section",
                metadata.id
            );
        }
        let source = load_fragment(&directory, relative_name, kind)?;
        loaded.push((kind, relative_name.to_owned(), source));
    }

    let fragments = FeatureFragments {
        imports: loaded[0].2.clone(),
        shared_resources: loaded[1].2.clone(),
        local_resources: loaded[2].2.clone(),
        init: loaded[3].2.clone(),
        shared_constructors: loaded[4].2.clone(),
        local_constructors: loaded[5].2.clone(),
        tasks: loaded[6].2.clone(),
    };
    validate_fragment_contract(&metadata, &fragments)?;

    let fingerprint = feature_fingerprint(&metadata_bytes, &loaded);

    Ok(FeatureBundle {
        implementation: implementation.to_owned(),
        directory,
        metadata,
        fragments,
        fingerprint,
    })
}

pub fn load(feature_library_root: &Path, implementation: &str) -> Result<FeatureBundle> {
    load_feature_bundle(feature_library_root, implementation)
}

fn validate_metadata(metadata: &FeatureMetadata, metadata_path: &Path) -> Result<()> {
    if metadata.schema_version != 2 {
        bail!(
            "unsupported feature schema_version {} in `{}`; expected 2",
            metadata.schema_version,
            metadata_path.display()
        );
    }
    validate_rust_identifier(&metadata.id, "feature id")?;

    unique_valid_placeholders(&metadata.required_placeholders, "required_placeholders")?;
    unique_valid_placeholders(&metadata.claimed_resources, "claimed_resources")?;
    unique_valid_placeholders(&metadata.claimed_interrupts, "claimed_interrupts")?;
    unique_valid_identifiers(&metadata.claimed_symbols, "claimed_symbols")?;
    unique_valid_identifiers(&metadata.required_symbols, "required_symbols")?;
    unique_valid_identifiers(&metadata.insertion_after, "insertion_after")?;

    let required: BTreeSet<_> = metadata.required_placeholders.iter().collect();
    for (claim_kind, claims) in [
        ("claimed_resources", &metadata.claimed_resources),
        ("claimed_interrupts", &metadata.claimed_interrupts),
    ] {
        for claim in claims {
            if !required.contains(claim) {
                bail!(
                    "feature `{}` {claim_kind} entry `{claim}` is not a required placeholder",
                    metadata.id
                );
            }
        }
    }

    Ok(())
}

fn validate_fragment_contract(
    metadata: &FeatureMetadata,
    fragments: &FeatureFragments,
) -> Result<()> {
    let required: BTreeSet<String> = metadata.required_placeholders.iter().cloned().collect();
    let mut actual = BTreeSet::new();
    for (kind, source) in fragments.iter() {
        actual.extend(scan_placeholders(source, kind)?);
    }

    let missing: Vec<_> = required.difference(&actual).cloned().collect();
    let unknown: Vec<_> = actual.difference(&required).cloned().collect();
    if !missing.is_empty() || !unknown.is_empty() {
        let mut details = Vec::new();
        if !missing.is_empty() {
            details.push(format!("missing markers: {}", missing.join(", ")));
        }
        if !unknown.is_empty() {
            details.push(format!("undeclared markers: {}", unknown.join(", ")));
        }
        bail!(
            "feature `{}` marker contract does not match its fragments ({})",
            metadata.id,
            details.join("; ")
        );
    }

    let resource_markers = scan_resource_markers(fragments)?;
    let claimed_resources: BTreeSet<_> = metadata.claimed_resources.iter().cloned().collect();
    if resource_markers != claimed_resources {
        bail!(
            "feature `{}` resource claims do not match resource declarations (metadata: {}; fragments: {})",
            metadata.id,
            display_set(&claimed_resources),
            display_set(&resource_markers)
        );
    }

    let interrupt_markers = scan_interrupt_markers(&fragments.tasks)?;
    let claimed_interrupts: BTreeSet<_> = metadata.claimed_interrupts.iter().cloned().collect();
    if interrupt_markers != claimed_interrupts {
        bail!(
            "feature `{}` interrupt claims do not match task bindings (metadata: {}; tasks: {})",
            metadata.id,
            display_set(&claimed_interrupts),
            display_set(&interrupt_markers)
        );
    }

    let declared_symbols = scan_declared_symbols(&fragments.tasks);
    let claimed_symbols: BTreeSet<_> = metadata.claimed_symbols.iter().cloned().collect();
    if declared_symbols != claimed_symbols {
        bail!(
            "feature `{}` symbol claims do not match task declarations (metadata: {}; tasks: {})",
            metadata.id,
            display_set(&claimed_symbols),
            display_set(&declared_symbols)
        );
    }

    Ok(())
}

/// Scans exact `{{PLACEHOLDER}}` markers without interpreting arbitrary Rust.
pub fn scan_placeholders(source: &str, fragment_name: &str) -> Result<BTreeSet<String>> {
    let bytes = source.as_bytes();
    let mut markers = BTreeSet::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"{{") {
            let marker_start = cursor + 2;
            let mut marker_end = marker_start;
            while marker_end < bytes.len() && !bytes[marker_end..].starts_with(b"}}") {
                if bytes[marker_end..].starts_with(b"{{") {
                    bail!("nested placeholder opener in `{fragment_name}` at byte {marker_end}");
                }
                marker_end += 1;
            }
            if marker_end == bytes.len() {
                bail!("unclosed placeholder in `{fragment_name}` at byte {cursor}");
            }
            let marker = &source[marker_start..marker_end];
            validate_placeholder(marker).with_context(|| {
                format!("invalid placeholder in `{fragment_name}` at byte {cursor}")
            })?;
            markers.insert(marker.to_owned());
            cursor = marker_end + 2;
        } else if bytes[cursor..].starts_with(b"}}") {
            bail!("unmatched placeholder closer in `{fragment_name}` at byte {cursor}");
        } else {
            cursor += 1;
        }
    }

    Ok(markers)
}

fn scan_resource_markers(fragments: &FeatureFragments) -> Result<BTreeSet<String>> {
    let mut markers = BTreeSet::new();
    for (kind, source) in [
        ("shared_resources", fragments.shared_resources.as_str()),
        ("local_resources", fragments.local_resources.as_str()),
    ] {
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            if !trimmed.starts_with("{{") {
                bail!(
                    "{kind} line {} must start with a claimed resource placeholder",
                    line_index + 1
                );
            }
            let Some(close) = trimmed.find("}}") else {
                bail!(
                    "unclosed resource placeholder in {kind} line {}",
                    line_index + 1
                );
            };
            let marker = &trimmed[2..close];
            validate_placeholder(marker)?;
            if !trimmed[close + 2..].trim_start().starts_with(':') {
                bail!(
                    "resource placeholder `{marker}` in {kind} line {} must be followed by `:`",
                    line_index + 1
                );
            }
            if !markers.insert(marker.to_owned()) {
                bail!("resource marker `{marker}` is declared more than once");
            }
        }
    }
    Ok(markers)
}

fn scan_interrupt_markers(tasks: &str) -> Result<BTreeSet<String>> {
    let bytes = tasks.as_bytes();
    let mut markers = BTreeSet::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if is_identifier_start(bytes[cursor]) {
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
                cursor += 1;
            }
            if &tasks[start..cursor] != "binds" {
                continue;
            }
            cursor = skip_ascii_whitespace(bytes, cursor);
            if bytes.get(cursor) != Some(&b'=') {
                continue;
            }
            cursor = skip_ascii_whitespace(bytes, cursor + 1);
            if !bytes[cursor..].starts_with(b"{{") {
                bail!("RTIC `binds` value must be an exact feature placeholder");
            }
            let remaining = &tasks[cursor..];
            let Some(close) = remaining.find("}}") else {
                bail!("unclosed RTIC `binds` placeholder");
            };
            let marker = &remaining[2..close];
            validate_placeholder(marker)?;
            markers.insert(marker.to_owned());
            cursor += close + 2;
        } else {
            cursor += 1;
        }
    }
    Ok(markers)
}

fn scan_declared_symbols(source: &str) -> BTreeSet<String> {
    let tokens = rust_identifier_tokens(source);
    let mut symbols = BTreeSet::new();
    for pair in tokens.windows(2) {
        if matches!(
            pair[0].as_str(),
            "fn" | "struct" | "enum" | "type" | "const" | "static" | "mod" | "trait"
        ) {
            symbols.insert(pair[1].clone());
        }
    }
    symbols
}

fn rust_identifier_tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"//") {
            cursor += 2;
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
        } else if bytes[cursor..].starts_with(b"/*") {
            cursor += 2;
            let mut depth = 1_u32;
            while cursor < bytes.len() && depth != 0 {
                if bytes[cursor..].starts_with(b"/*") {
                    depth += 1;
                    cursor += 2;
                } else if bytes[cursor..].starts_with(b"*/") {
                    depth -= 1;
                    cursor += 2;
                } else {
                    cursor += 1;
                }
            }
        } else if bytes[cursor] == b'"' {
            cursor += 1;
            while cursor < bytes.len() {
                if bytes[cursor] == b'\\' {
                    cursor = (cursor + 2).min(bytes.len());
                } else if bytes[cursor] == b'"' {
                    cursor += 1;
                    break;
                } else {
                    cursor += 1;
                }
            }
        } else if bytes[cursor] == b'\'' {
            let identifier_start = cursor + 1;
            if identifier_start < bytes.len() && is_identifier_start(bytes[identifier_start]) {
                let mut identifier_end = identifier_start + 1;
                while identifier_end < bytes.len() && is_identifier_continue(bytes[identifier_end])
                {
                    identifier_end += 1;
                }
                if bytes.get(identifier_end) == Some(&b'\'') {
                    // One-character Rust character literal such as `'x'`.
                    cursor = identifier_end + 1;
                } else {
                    // Rust lifetime such as `'static`; declaration scanning
                    // must not mistake it for a `static` item keyword.
                    cursor = identifier_end;
                }
            } else {
                // Escaped character literal such as `'\n'`.
                cursor += 1;
                while cursor < bytes.len() {
                    if bytes[cursor] == b'\\' {
                        cursor = (cursor + 2).min(bytes.len());
                    } else if bytes[cursor] == b'\'' {
                        cursor += 1;
                        break;
                    } else {
                        cursor += 1;
                    }
                }
            }
        } else if is_identifier_start(bytes[cursor]) {
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
                cursor += 1;
            }
            tokens.push(source[start..cursor].to_owned());
        } else {
            cursor += 1;
        }
    }
    tokens
}

fn load_fragment(directory: &Path, relative_name: &str, kind: &str) -> Result<String> {
    let relative_path = validate_relative_path(relative_name, &format!("{kind} fragment"))?;
    let requested = directory.join(relative_path);
    let canonical = fs::canonicalize(&requested).with_context(|| {
        format!(
            "missing {kind} fragment `{}` for feature `{}`",
            relative_name,
            directory.display()
        )
    })?;
    ensure_confined(&canonical, directory, &format!("{kind} fragment"))?;
    if !canonical.is_file() {
        bail!("{kind} fragment `{relative_name}` is not a regular file");
    }
    fs::read_to_string(&canonical).with_context(|| {
        format!(
            "failed to read {kind} fragment `{}` as UTF-8",
            canonical.display()
        )
    })
}

fn validate_relative_path<'a>(value: &'a str, label: &str) -> Result<&'a Path> {
    if value.is_empty() {
        bail!("{label} path must not be empty");
    }
    let path = Path::new(value);
    if path.is_absolute() {
        bail!("{label} path `{value}` must be relative");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::ParentDir => {
                bail!("{label} path `{value}` must not contain `..`");
            }
            Component::CurDir => {
                bail!("{label} path `{value}` must not contain `.` components");
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("{label} path `{value}` must be relative");
            }
        }
    }
    Ok(path)
}

fn ensure_confined(path: &Path, root: &Path, label: &str) -> Result<()> {
    if !path.starts_with(root) {
        bail!(
            "{label} `{}` resolves outside `{}`",
            path.display(),
            root.display()
        );
    }
    Ok(())
}

fn unique_valid_placeholders(values: &[String], field: &str) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_placeholder(value)
            .with_context(|| format!("invalid `{field}` entry `{value}`"))?;
        if !unique.insert(value) {
            bail!("duplicate `{field}` entry `{value}`");
        }
    }
    Ok(())
}

fn unique_valid_identifiers(values: &[String], field: &str) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_rust_identifier(value, field)?;
        if !unique.insert(value) {
            bail!("duplicate `{field}` entry `{value}`");
        }
    }
    Ok(())
}

fn validate_placeholder(value: &str) -> Result<()> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        bail!("placeholder name must not be empty");
    };
    if !first.is_ascii_uppercase() {
        bail!("placeholder `{value}` must begin with an ASCII uppercase letter");
    }
    if !bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_') {
        bail!(
            "placeholder `{value}` may contain only ASCII uppercase letters, digits, and underscores"
        );
    }
    Ok(())
}

fn validate_rust_identifier(value: &str, field: &str) -> Result<()> {
    let identifier = syn::parse_str::<syn::Ident>(value)
        .with_context(|| format!("`{field}` value `{value}` is not a Rust identifier"))?;
    if identifier != value {
        bail!("`{field}` value `{value}` is not a plain Rust identifier");
    }
    Ok(())
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn skip_ascii_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    cursor
}

fn display_set(values: &BTreeSet<String>) -> String {
    if values.is_empty() {
        "<none>".to_owned()
    } else {
        values.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

fn feature_fingerprint(metadata_bytes: &[u8], loaded: &[(&str, String, String)]) -> String {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, FEATURE_METADATA_FILE.as_bytes());
    hash_part(&mut hasher, metadata_bytes);
    for (kind, name, source) in loaded {
        hash_part(&mut hasher, kind.as_bytes());
        hash_part(&mut hasher, name.as_bytes());
        hash_part(&mut hasher, source.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_valid_bundle(root: &Path) -> PathBuf {
        let bundle = root.join("stm32f4").join("blink-led");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(
            bundle.join("feature.toml"),
            r#"
schema_version = 2
id = "blink_led"
imports = "imports.rs.tpl"
shared_resources = "shared-resources.rs.tpl"
local_resources = "local-resources.rs.tpl"
init = "init.rs.tpl"
shared_constructors = "shared-constructors.rs.tpl"
local_constructors = "local-constructors.rs.tpl"
tasks = "tasks.rs.tpl"
required_placeholders = ["LED_RESOURCE_NAME", "LED_RESOURCE_TYPE", "LED_INIT_EXPRESSION", "TIMER_RESOURCE_NAME", "TIMER_RESOURCE_TYPE", "TIMER_INIT_EXPRESSION", "TIMER_INTERRUPT", "TASK_PRIORITY"]
claimed_symbols = ["blink_led"]
required_symbols = []
claimed_resources = ["LED_RESOURCE_NAME", "TIMER_RESOURCE_NAME"]
claimed_interrupts = ["TIMER_INTERRUPT"]
insertion_after = []
"#,
        )
        .unwrap();
        fs::write(bundle.join("imports.rs.tpl"), "").unwrap();
        fs::write(bundle.join("shared-resources.rs.tpl"), "").unwrap();
        fs::write(
            bundle.join("local-resources.rs.tpl"),
            "{{LED_RESOURCE_NAME}}: {{LED_RESOURCE_TYPE}},\n{{TIMER_RESOURCE_NAME}}: {{TIMER_RESOURCE_TYPE}},\n",
        )
        .unwrap();
        fs::write(
            bundle.join("init.rs.tpl"),
            "{{LED_INIT_EXPRESSION}}\n{{TIMER_INIT_EXPRESSION}}\n",
        )
        .unwrap();
        fs::write(bundle.join("shared-constructors.rs.tpl"), "").unwrap();
        fs::write(
            bundle.join("local-constructors.rs.tpl"),
            "{{LED_RESOURCE_NAME}},\n{{TIMER_RESOURCE_NAME}},\n",
        )
        .unwrap();
        fs::write(
            bundle.join("tasks.rs.tpl"),
            "#[task(binds = {{TIMER_INTERRUPT}}, priority = {{TASK_PRIORITY}}, local = [{{LED_RESOURCE_NAME}}, {{TIMER_RESOURCE_NAME}}])]\nfn blink_led(_: blink_led::Context) {}\n",
        )
        .unwrap();
        bundle
    }

    #[test]
    fn loads_all_fragments_and_has_stable_fingerprint() {
        let temp = tempfile::tempdir().unwrap();
        write_valid_bundle(temp.path());

        let first = load_feature_bundle(temp.path(), "stm32f4/blink-led").unwrap();
        let second = load_feature_bundle(temp.path(), "stm32f4/blink-led").unwrap();

        assert_eq!(first.metadata.id, "blink_led");
        assert!(first.fragments.tasks.contains("fn blink_led"));
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.fingerprint.len(), 64);
    }

    #[test]
    fn rejects_parent_path_implementation() {
        let temp = tempfile::tempdir().unwrap();
        let error = load_feature_bundle(temp.path(), "../outside").unwrap_err();
        assert!(error.to_string().contains("must not contain `..`"));
    }

    #[test]
    fn rejects_missing_fragment() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = write_valid_bundle(temp.path());
        fs::remove_file(bundle.join("tasks.rs.tpl")).unwrap();

        let error = load_feature_bundle(temp.path(), "stm32f4/blink-led").unwrap_err();
        assert!(error.to_string().contains("missing tasks fragment"));
    }

    #[test]
    fn rejects_missing_and_undeclared_markers() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = write_valid_bundle(temp.path());
        fs::write(
            bundle.join("init.rs.tpl"),
            "{{LED_INIT_EXPRESSION}}\n{{NOT_DECLARED}}\n",
        )
        .unwrap();

        let error = load_feature_bundle(temp.path(), "stm32f4/blink-led").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("missing markers: TIMER_INIT_EXPRESSION"));
        assert!(message.contains("undeclared markers: NOT_DECLARED"));
    }

    #[test]
    fn rejects_malformed_markers() {
        let error = scan_placeholders("{{ BAD }}", "test").unwrap_err();
        assert!(error.to_string().contains("invalid placeholder"));
        let error = scan_placeholders("{{UNCLOSED", "test").unwrap_err();
        assert!(error.to_string().contains("unclosed placeholder"));
    }

    #[test]
    fn declaration_scanner_ignores_lifetimes_and_literals() {
        let source = r#"
            type WorkSender = Sender<'static, OsdWork<70>, 4>;
            const MARKER: char = 'x';
            const NEWLINE: char = '\n';
            async fn osd_displayport() {}
        "#;

        assert_eq!(
            scan_declared_symbols(source),
            BTreeSet::from([
                "MARKER".to_owned(),
                "NEWLINE".to_owned(),
                "WorkSender".to_owned(),
                "osd_displayport".to_owned(),
            ])
        );
    }

    #[test]
    fn fingerprint_changes_with_fragment_content() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = write_valid_bundle(temp.path());
        let before = load_feature_bundle(temp.path(), "stm32f4/blink-led")
            .unwrap()
            .fingerprint;
        fs::write(bundle.join("imports.rs.tpl"), "use core::fmt;\n").unwrap();
        let after = load_feature_bundle(temp.path(), "stm32f4/blink-led")
            .unwrap()
            .fingerprint;
        assert_ne!(before, after);
    }
}
