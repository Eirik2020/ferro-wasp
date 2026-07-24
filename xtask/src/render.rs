use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{backend::ResolvedFeature, manifest::Manifest, mcu};

const TEMPLATE_DIR: &str = "templates/stm32f4-rtic";

#[derive(Debug, Clone)]
pub struct TemplateSet {
    pub cargo_toml: String,
    pub cargo_lock: String,
    pub cargo_lock_osd: String,
    pub build_rs: String,
    pub memory_x: String,
    pub main_rs: String,
}

impl TemplateSet {
    pub fn load(repository_root: &Path) -> Result<Self> {
        let root = repository_root.join(TEMPLATE_DIR);
        Ok(Self {
            cargo_toml: read_template(&root.join("Cargo.toml.tpl"))?,
            cargo_lock: read_template(&root.join("Cargo.lock.tpl"))?,
            cargo_lock_osd: read_template(&root.join("Cargo.lock.osd.tpl"))?,
            build_rs: read_template(&root.join("build.rs.tpl"))?,
            memory_x: read_template(&root.join("memory.x.tpl"))?,
            main_rs: read_template(&root.join("src/main.rs.tpl"))?,
        })
    }

    pub fn fingerprint_inputs(&self) -> [(&'static str, &[u8]); 6] {
        [
            ("template/Cargo.toml", self.cargo_toml.as_bytes()),
            ("template/Cargo.lock", self.cargo_lock.as_bytes()),
            ("template/Cargo.lock.osd", self.cargo_lock_osd.as_bytes()),
            ("template/build.rs", self.build_rs.as_bytes()),
            ("template/memory.x", self.memory_x.as_bytes()),
            ("template/src/main.rs", self.main_rs.as_bytes()),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedCrate {
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

impl RenderedCrate {
    pub fn insert_text(&mut self, path: impl Into<PathBuf>, text: String) {
        self.files
            .insert(path.into(), normalize_newlines(text).into_bytes());
    }

    pub fn write_to(&self, root: &Path) -> Result<()> {
        if root.exists() {
            bail!("render destination already exists: {}", root.display());
        }
        fs::create_dir_all(root)
            .with_context(|| format!("create render destination {}", root.display()))?;

        for (relative, contents) in &self.files {
            validate_relative_path(relative)?;
            let destination = root.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create directory {}", parent.display()))?;
            }
            fs::write(&destination, contents)
                .with_context(|| format!("write rendered file {}", destination.display()))?;
        }
        Ok(())
    }
}

pub fn render_crate(
    manifest: &Manifest,
    templates: &TemplateSet,
    features: &[ResolvedFeature],
) -> Result<RenderedCrate> {
    let profile = mcu::profile(&manifest.bsp.mcu)?;
    let has_button = features
        .iter()
        .any(|feature| matches!(feature.id.as_str(), "button_toggle" | "button_arm_toggle"));
    let mut imports = Vec::new();
    let mut shared_resources = Vec::new();
    let mut local_resources = Vec::new();
    let mut init = Vec::new();
    let mut shared_values = Vec::new();
    let mut local_values = Vec::new();
    let mut tasks = Vec::new();

    if !features.is_empty() {
        // The BSP owns clock setup and peripheral decomposition. Feature
        // fragments consume the resulting board resources.
        imports.push("use stm32f4xx_hal::{prelude::*, rcc::Config};".to_owned());
    }

    for feature in features {
        imports.push(render_fragment(
            &feature.fragments.imports,
            &feature.replacements,
            &feature.id,
            "imports",
        )?);
        shared_resources.push(render_fragment(
            &feature.fragments.shared_resources,
            &feature.replacements,
            &feature.id,
            "shared resources",
        )?);
        local_resources.push(render_fragment(
            &feature.fragments.local_resources,
            &feature.replacements,
            &feature.id,
            "local resources",
        )?);
        init.push(render_fragment(
            &feature.fragments.init,
            &feature.replacements,
            &feature.id,
            "init",
        )?);
        shared_values.push(render_fragment(
            &feature.fragments.shared_constructors,
            &feature.replacements,
            &feature.id,
            "shared constructors",
        )?);
        local_values.push(render_fragment(
            &feature.fragments.local_constructors,
            &feature.replacements,
            &feature.id,
            "local constructors",
        )?);
        tasks.push(render_fragment(
            &feature.fragments.tasks,
            &feature.replacements,
            &feature.id,
            "tasks",
        )?);
    }

    let mut application = BTreeMap::new();
    // This is a translation of an already validated allowlisted value, not a
    // manifest-supplied Rust expression.
    application.insert("PAC_PATH".to_owned(), profile.pac_path.to_owned());
    application.insert(
        "RTIC_DISPATCHERS".to_owned(),
        if features
            .iter()
            .any(|feature| feature.id == "osd_displayport")
        {
            ", dispatchers = [EXTI0, EXTI1, EXTI2]".to_owned()
        } else {
            ", dispatchers = [EXTI0, EXTI1]".to_owned()
        },
    );
    application.insert(
        "RTIC_INIT_LOCALS".to_owned(),
        if features.iter().any(|feature| feature.id == "osd_displayport") {
            "(local = [rx_active: [u8; 70] = [0; 70], rx_spare: [u8; 70] = [0; 70], tx_buffer: [u8; 70] = [0; 70]])".to_owned()
        } else {
            String::new()
        },
    );
    application.insert(
        "INIT_CONTEXT".to_owned(),
        if has_button { "mut cx" } else { "cx" }.to_owned(),
    );
    application.insert("FEATURE_IMPORTS".to_owned(), merge_imports(&imports)?);
    application.insert(
        "SHARED_RESOURCES".to_owned(),
        join_sections(&shared_resources),
    );
    application.insert(
        "LOCAL_RESOURCES".to_owned(),
        join_sections(&local_resources),
    );
    application.insert(
        "BASE_INIT".to_owned(),
        if features.is_empty() {
            "let _ = cx;".to_owned()
        } else if has_button {
            "let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));\nMono::start(cx.core.SYST, 84_000_000);\nlet mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);\nlet gpioa = cx.device.GPIOA.split(&mut rcc);\nlet gpioc = cx.device.GPIOC.split(&mut rcc);".to_owned()
        } else {
            "let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));\nMono::start(cx.core.SYST, 84_000_000);\nlet gpioa = cx.device.GPIOA.split(&mut rcc);".to_owned()
        },
    );
    application.insert("FEATURE_INIT".to_owned(), join_sections(&init));
    application.insert(
        "SHARED_RESOURCE_VALUES".to_owned(),
        join_sections(&shared_values),
    );
    application.insert(
        "LOCAL_RESOURCE_VALUES".to_owned(),
        join_sections(&local_values),
    );
    application.insert("FEATURE_TASKS".to_owned(), join_sections(&tasks));

    let main_rs = substitute(&templates.main_rs, &application)
        .context("render base RTIC application template")?;

    let mut memory = BTreeMap::new();
    memory.insert(
        "FLASH_ORIGIN".to_owned(),
        format!("0x{:08X}", profile.flash_origin),
    );
    memory.insert(
        "FLASH_SIZE_BYTES".to_owned(),
        profile.flash_size_bytes.to_string(),
    );
    memory.insert(
        "RAM_ORIGIN".to_owned(),
        format!("0x{:08X}", profile.ram_origin),
    );
    memory.insert(
        "RAM_SIZE_BYTES".to_owned(),
        profile.ram_size_bytes.to_string(),
    );
    let memory_x = substitute(&templates.memory_x, &memory).context("render memory.x")?;

    let cargo = BTreeMap::from([
        ("HAL_CRATE".to_owned(), profile.hal_crate.to_owned()),
        ("HAL_FEATURE".to_owned(), profile.hal_feature.to_owned()),
        (
            "FEATURE_DEPENDENCIES".to_owned(),
            if features
                .iter()
                .any(|feature| feature.id == "osd_displayport")
            {
                "cortex-m = \"=0.7.7\"\nrtic-sync = \"=1.5.0\"\nferrowasp-serial-osd-compat = { path = \"../../../compat/ferrowasp-serial-osd\" }".to_owned()
            } else {
                String::new()
            },
        ),
    ]);
    let cargo_toml = substitute(&templates.cargo_toml, &cargo).context("render Cargo.toml")?;

    for (label, text) in [
        ("Cargo.toml", &cargo_toml),
        (
            "Cargo.lock",
            if features
                .iter()
                .any(|feature| feature.id == "osd_displayport")
            {
                &templates.cargo_lock_osd
            } else {
                &templates.cargo_lock
            },
        ),
        ("build.rs", &templates.build_rs),
        ("memory.x", &memory_x),
        ("src/main.rs", &main_rs),
    ] {
        ensure_no_markers(text).with_context(|| format!("validate rendered {label}"))?;
    }

    let mut rendered = RenderedCrate::default();
    rendered.insert_text("Cargo.toml", cargo_toml);
    rendered.insert_text(
        "Cargo.lock",
        if features
            .iter()
            .any(|feature| feature.id == "osd_displayport")
        {
            templates.cargo_lock_osd.clone()
        } else {
            templates.cargo_lock.clone()
        },
    );
    rendered.insert_text("build.rs", templates.build_rs.clone());
    rendered.insert_text("memory.x", memory_x);
    rendered.insert_text("src/main.rs", main_rs);
    Ok(rendered)
}

fn read_template(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("read template {}", path.display()))
}

fn render_fragment(
    fragment: &str,
    replacements: &BTreeMap<String, String>,
    feature: &str,
    role: &str,
) -> Result<String> {
    substitute(fragment, replacements)
        .with_context(|| format!("render {role} fragment for feature `{feature}`"))
}

pub fn substitute(template: &str, replacements: &BTreeMap<String, String>) -> Result<String> {
    let mut output = String::with_capacity(template.len());
    let mut cursor = 0;

    while let Some(relative_start) = template[cursor..].find("{{") {
        let start = cursor + relative_start;
        output.push_str(&template[cursor..start]);
        let name_start = start + 2;
        let Some(relative_end) = template[name_start..].find("}}") else {
            bail!("unterminated placeholder beginning at byte {start}");
        };
        let end = name_start + relative_end;
        let name = &template[name_start..end];
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            bail!("invalid placeholder `{{{{{name}}}}}`");
        }
        let Some(value) = replacements.get(name) else {
            bail!("unresolved placeholder `{{{{{name}}}}}`");
        };
        output.push_str(value);
        cursor = end + 2;
    }
    output.push_str(&template[cursor..]);
    ensure_no_markers(&output)?;
    Ok(output)
}

pub fn ensure_no_markers(text: &str) -> Result<()> {
    if let Some(position) = text.find("{{") {
        bail!("unresolved or malformed placeholder at byte {position}");
    }
    if let Some(position) = text.find("}}") {
        bail!("unmatched placeholder terminator at byte {position}");
    }
    Ok(())
}

fn join_sections(sections: &[String]) -> String {
    sections
        .iter()
        .filter(|section| !section.trim().is_empty())
        .map(|section| section.trim().to_owned())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn merge_imports(sections: &[String]) -> Result<String> {
    let mut bindings = BTreeMap::<String, (String, String)>::new();
    for section in sections.iter().filter(|section| !section.trim().is_empty()) {
        let file = syn::parse_file(section).context("parse feature import fragment")?;
        for item in file.items {
            let syn::Item::Use(item) = item else {
                bail!("feature import fragments may contain only `use` items");
            };
            if !item.attrs.is_empty()
                || !matches!(item.vis, syn::Visibility::Inherited)
                || item.leading_colon.is_some()
            {
                bail!("feature imports must be unqualified private `use` items without attributes");
            }
            collect_use_tree(&item.tree, &mut Vec::new(), &mut bindings)?;
        }
    }
    Ok(bindings
        .into_values()
        .map(|(_, rendered)| rendered)
        .collect::<Vec<_>>()
        .join("\n"))
}

fn collect_use_tree(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    bindings: &mut BTreeMap<String, (String, String)>,
) -> Result<()> {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_use_tree(&path.tree, prefix, bindings)?;
            prefix.pop();
        }
        syn::UseTree::Name(name) if name.ident == "self" => {
            let source = prefix.join("::");
            let binding = prefix
                .last()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("bare `self` import is unsupported"))?;
            record_import(binding, source.clone(), format!("use {source};"), bindings)?;
        }
        syn::UseTree::Name(name) => {
            let binding = name.ident.to_string();
            let source = path_with_leaf(prefix, &binding);
            record_import(binding, source.clone(), format!("use {source};"), bindings)?;
        }
        syn::UseTree::Rename(rename) => {
            let source = path_with_leaf(prefix, &rename.ident.to_string());
            let binding = rename.rename.to_string();
            if binding == "_" {
                bail!("underscore import aliases are unsupported in feature fragments");
            }
            record_import(
                binding.clone(),
                source.clone(),
                format!("use {source} as {binding};"),
                bindings,
            )?;
        }
        syn::UseTree::Glob(_) => {
            let source = prefix.join("::");
            if source.is_empty() {
                bail!("bare glob imports are unsupported in feature fragments");
            }
            let key = format!("*:{source}");
            bindings
                .entry(key)
                .or_insert_with(|| (source.clone(), format!("use {source}::*;")));
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree(item, prefix, bindings)?;
            }
        }
    }
    Ok(())
}

fn path_with_leaf(prefix: &[String], leaf: &str) -> String {
    if prefix.is_empty() {
        leaf.to_owned()
    } else {
        format!("{}::{leaf}", prefix.join("::"))
    }
}

fn record_import(
    binding: String,
    source: String,
    rendered: String,
    bindings: &mut BTreeMap<String, (String, String)>,
) -> Result<()> {
    if let Some((existing_source, _)) = bindings.get(&binding) {
        if existing_source != &source {
            bail!(
                "conflicting imports bind `{binding}` from both `{existing_source}` and `{source}`"
            );
        }
        return Ok(());
    }
    bindings.insert(binding, (source, rendered));
    Ok(())
}

fn normalize_newlines(text: String) -> String {
    text.replace("\r\n", "\n")
}

fn validate_relative_path(path: &Path) -> Result<()> {
    use std::path::Component;
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!(
            "rendered path must be a non-empty relative path: {}",
            path.display()
        );
    }
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("rendered path escapes its destination: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_only_declared_exact_markers() {
        let replacements = BTreeMap::from([("NAME".to_owned(), "led2".to_owned())]);
        assert_eq!(
            substitute("let {{NAME}} = 1;", &replacements).unwrap(),
            "let led2 = 1;"
        );
        assert!(substitute("{{UNKNOWN}}", &replacements).is_err());
        assert!(substitute("{{bad}}", &replacements).is_err());
        assert!(substitute("{{NAME", &replacements).is_err());
    }

    #[test]
    fn refuses_escaping_output_paths() {
        assert!(validate_relative_path(Path::new("../outside")).is_err());
        assert!(validate_relative_path(Path::new("src/main.rs")).is_ok());
    }

    #[test]
    fn structurally_deduplicates_and_rejects_conflicting_imports() {
        let identical = vec![
            "use core::{fmt::Debug, marker::Copy};".to_owned(),
            "use core::fmt::Debug;".to_owned(),
        ];
        let merged = merge_imports(&identical).unwrap();
        assert_eq!(merged.matches("Debug").count(), 1);
        assert!(merged.contains("use core::marker::Copy;"));

        let conflicting = vec![
            "use core::fmt::Result;".to_owned(),
            "use std::io::Result;".to_owned(),
        ];
        assert!(merge_imports(&conflicting).is_err());
    }
}
