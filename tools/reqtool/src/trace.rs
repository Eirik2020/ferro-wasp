//! Tracing per source file (design section 20): for every module of every
//! package, its identity, the requirements it declares, whether it is
//! safety-critical, and what it imports directly.
//!
//! A module declares in its module doc comment:
//!
//! ```text
//! //! Requirements: SAFE-001, SAFE-002
//! //! Safety-critical.
//! ```
//!
//! An import is a `use` declaration or a path in the file, resolved to the
//! file that defines what it names, or to an external crate at its locked
//! version. Where a path does not resolve to one file, the import widens and
//! never narrows:
//! - a glob import covers the module's file and every file it re-exports;
//! - a macro covers the files that define a `macro_rules!` of its name in the
//!   importing crate, or in the crate its path names;
//! - a type or trait covers the files holding `impl` blocks for it, in its
//!   own crate and in the importing one;
//! - paths inside macro invocations and attribute arguments are read from
//!   their tokens.
//!
//! `#[cfg]` is ignored, so an import counts if any feature set compiles it;
//! a configuration then takes only the files it compiles in.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use serde::Deserialize;
use syn::visit::{self, Visit};
use syn::{Item, UseTree};

use crate::identity;
use crate::items::{self, Kind, Module, Resolver};
use crate::workspace::{Crate, Workspace};

pub const REQUIREMENTS_LINE: &str = "Requirements:";
pub const CRITICAL_LINE: &str = "Safety-critical.";

/// Crates every Rust program can name without a dependency; they belong to
/// the toolchain, which a record covers on its own.
const TOOLCHAIN_CRATES: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];

/// How deep glob re-exports are followed.
const MAX_GLOB_DEPTH: usize = 4;

/// What one direct import is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Import {
    /// Another source file, by repository-relative path.
    File,
    /// An external crate; `pinned` when its manifest entry names one version.
    External { pinned: bool },
}

/// One source file that some package's module tree reaches.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub package: String,
    pub identity: String,
    pub requirements: Vec<String>,
    pub critical: bool,
    /// Holds something beyond module declarations and imports.
    pub defines_code: bool,
    /// By key: a repository-relative path, or `extern:<package>@<version>`.
    pub imports: BTreeMap<String, Import>,
}

/// Every source file, keyed by repository-relative path.
#[derive(Debug, Default)]
pub struct Trace {
    pub files: BTreeMap<String, SourceFile>,
    /// Files that could not be read, parsed or reached.
    pub problems: Vec<String>,
}

impl Trace {
    pub fn build(repository: &Path, workspace: &Workspace) -> Result<Self, String> {
        let resolver = Resolver::new(&workspace.crates);
        let mut builder = Builder {
            repository,
            workspace,
            resolver: &resolver,
            trace: Self::default(),
            locks: BTreeMap::new(),
            impls: BTreeMap::new(),
            macros: BTreeMap::new(),
        };
        for package in workspace.packages.values() {
            let mut roots = Vec::new();
            if let Some(lib) = &package.lib {
                roots.push((package.name.clone(), lib.clone()));
            }
            if let Some(main) = &package.main {
                // A binary's `crate::` is not the library's, so it gets a
                // name no path can reach.
                roots.push((format!("{}#bin", package.name), main.clone()));
            }
            for (crate_name, file) in roots {
                match resolver.root_at(&crate_name, &file) {
                    Ok(root) => builder.module(package, &root),
                    Err(message) => builder.trace.problems.push(message),
                }
            }
        }
        Ok(builder.trace)
    }

    /// The current identity behind an import key: a file's identity, or an
    /// external crate's version, which its key already names.
    pub fn identity(&self, key: &str) -> Option<String> {
        match key.strip_prefix("extern:") {
            Some(external) => external
                .rsplit_once('@')
                .map(|(_, version)| version.to_owned()),
            None => self.files.get(key).map(|file| file.identity.clone()),
        }
    }

    /// The direct imports of a file, as sorted keys.
    pub fn imports(&self, path: &str) -> Vec<String> {
        self.files
            .get(path)
            .map(|file| file.imports.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// The files that declare a requirement.
    pub fn declaring(&self, requirement: &str) -> Vec<&str> {
        self.files
            .iter()
            .filter(|(_, file)| file.requirements.iter().any(|id| id == requirement))
            .map(|(path, _)| path.as_str())
            .collect()
    }

    /// The file a resolved item lives in, by repository-relative path.
    pub fn locate(&self, repository: &Path, file: &Path) -> Option<String> {
        let path = relative(repository, file);
        self.files.contains_key(&path).then_some(path)
    }
}

struct Builder<'a> {
    repository: &'a Path,
    workspace: &'a Workspace,
    resolver: &'a Resolver<'a>,
    trace: Trace,
    locks: BTreeMap<PathBuf, LockFile>,
    /// Per crate: type and trait names, to the files implementing them.
    impls: BTreeMap<String, BTreeMap<String, BTreeSet<PathBuf>>>,
    /// Per crate: macro names, to the files defining them.
    macros: BTreeMap<String, BTreeMap<String, BTreeSet<PathBuf>>>,
}

impl Builder<'_> {
    /// Records a module and everything below it.
    fn module(&mut self, package: &Crate, module: &Module) {
        let path = relative(self.repository, &module.file);
        if !self.trace.files.contains_key(&path) {
            match read(&module.file) {
                Ok((identity, requirements, critical)) => {
                    let defines_code = defines_code(&module.items);
                    self.trace.files.insert(
                        path.clone(),
                        SourceFile {
                            package: package.package.clone(),
                            identity,
                            requirements,
                            critical,
                            defines_code,
                            imports: BTreeMap::new(),
                        },
                    );
                }
                Err(message) => {
                    self.trace.problems.push(message);
                    return;
                }
            }
        }
        let imports = self.imports(package, module);
        if let Some(file) = self.trace.files.get_mut(&path) {
            file.imports.extend(imports);
            file.imports.remove(&path);
        }
        for child in self.resolver.submodules(module) {
            match child {
                Ok(child) => self.module(package, &child),
                Err(message) => self.trace.problems.push(format!(
                    "{}: {message}",
                    relative(self.repository, &module.file)
                )),
            }
        }
    }

    /// The direct imports of one module's own items, inline modules left to
    /// their own call.
    fn imports(&mut self, package: &Crate, module: &Module) -> BTreeMap<String, Import> {
        let mut references = References::default();
        for item in module.items.iter() {
            match item {
                Item::Mod(_) => {}
                Item::Use(declaration) => references.use_tree(&declaration.tree),
                other => references.visit_item(other),
            }
        }
        let mut out = BTreeMap::new();
        for path in &references.paths {
            self.path(package, module, path, &mut out);
        }
        for path in &references.globs {
            self.glob(package, module, path, 0, &mut out);
        }
        for path in &references.macros {
            self.macro_use(package, module, path, &mut out);
        }
        out
    }

    fn path(
        &mut self,
        package: &Crate,
        module: &Module,
        path: &[String],
        out: &mut BTreeMap<String, Import>,
    ) {
        let Some(first) = path.first() else {
            return;
        };
        if first == "Self" || TOOLCHAIN_CRATES.contains(&first.as_str()) {
            return;
        }
        if let Some(external) = self.external(package, module, first) {
            out.insert(external.0, Import::External { pinned: external.1 });
            return;
        }
        let Some(reached) = self.resolver.reach(module, path) else {
            return;
        };
        self.add_file(&reached.file, out);
        if matches!(reached.kind, Some(Kind::Type | Kind::Trait)) {
            for crate_name in [reached.crate_name.as_str(), module.crate_name.as_str()] {
                let files = self
                    .implementations(crate_name)
                    .get(&reached.name)
                    .cloned()
                    .unwrap_or_default();
                for file in files {
                    self.add_file(&file, out);
                }
            }
        }
    }

    /// A glob import: the module's file, and every file its `pub use`
    /// declarations re-export.
    fn glob(
        &mut self,
        package: &Crate,
        module: &Module,
        path: &[String],
        depth: usize,
        out: &mut BTreeMap<String, Import>,
    ) {
        if let Some(first) = path.first()
            && let Some(external) = self.external(package, module, first)
        {
            out.insert(external.0, Import::External { pinned: external.1 });
            return;
        }
        let Some(reached) = self.resolver.reach(module, path) else {
            return;
        };
        self.add_file(&reached.file, out);
        let Some(target) = reached.module else {
            return;
        };
        if depth >= MAX_GLOB_DEPTH {
            return;
        }
        for item in target.items.iter() {
            let Item::Use(declaration) = item else {
                continue;
            };
            if !matches!(declaration.vis, syn::Visibility::Public(_)) {
                continue;
            }
            let mut imports = Vec::new();
            items::flatten(&declaration.tree, &mut Vec::new(), &mut imports);
            for import in imports {
                match import {
                    items::Import::Named { path, .. } => self.path(package, &target, &path, out),
                    items::Import::Glob { path } => {
                        self.glob(package, &target, &path, depth + 1, out);
                    }
                }
            }
        }
    }

    fn macro_use(
        &mut self,
        package: &Crate,
        module: &Module,
        path: &[String],
        out: &mut BTreeMap<String, Import>,
    ) {
        let Some(name) = path.last() else {
            return;
        };
        if path.len() > 1 {
            self.path(package, module, path, out);
        }
        let crate_name = match path.first() {
            Some(first) if path.len() > 1 && self.workspace.crates.contains_key(first) => {
                first.clone()
            }
            _ => module.crate_name.clone(),
        };
        let files = self
            .macro_definitions(&crate_name)
            .get(name)
            .cloned()
            .unwrap_or_default();
        for file in files {
            self.add_file(&file, out);
        }
    }

    fn add_file(&self, file: &Path, out: &mut BTreeMap<String, Import>) {
        out.insert(relative(self.repository, file), Import::File);
    }

    /// `name` as an external crate the package depends on, unless the module
    /// declares or imports something of that name: its key, and whether it
    /// is pinned.
    fn external(&mut self, package: &Crate, module: &Module, name: &str) -> Option<(String, bool)> {
        if matches!(name, "crate" | "self" | "super") {
            return None;
        }
        let dependency = package.dependencies.get(name)?;
        if dependency.local || self.workspace.crates.contains_key(name) {
            return None;
        }
        if module.items.iter().any(|item| item_declares(item, name)) {
            return None;
        }
        let version = self.locked_version(package, &dependency.package);
        Some((
            format!("extern:{}@{version}", dependency.package),
            dependency.pinned,
        ))
    }

    fn locked_version(&mut self, package: &Crate, dependency: &str) -> String {
        let lock = self
            .locks
            .entry(package.lockfile.clone())
            .or_insert_with(|| {
                std::fs::read_to_string(&package.lockfile)
                    .ok()
                    .and_then(|text| toml::from_str(&text).ok())
                    .unwrap_or_default()
            });
        lock.version(&package.package, dependency)
            .unwrap_or_else(|| "unlocked".to_owned())
    }

    fn implementations(&mut self, crate_name: &str) -> &BTreeMap<String, BTreeSet<PathBuf>> {
        if !self.impls.contains_key(crate_name) {
            let mut index: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
            self.each_module(crate_name, &mut |module| {
                for item in module.items.iter() {
                    let Item::Impl(block) = item else {
                        continue;
                    };
                    if let syn::Type::Path(path) = block.self_ty.as_ref()
                        && let Some(last) = path.path.segments.last()
                    {
                        index
                            .entry(last.ident.to_string())
                            .or_default()
                            .insert(module.file.clone());
                    }
                    if let Some((_, path, _)) = &block.trait_
                        && let Some(last) = path.segments.last()
                    {
                        index
                            .entry(last.ident.to_string())
                            .or_default()
                            .insert(module.file.clone());
                    }
                }
            });
            self.impls.insert(crate_name.to_owned(), index);
        }
        &self.impls[crate_name]
    }

    fn macro_definitions(&mut self, crate_name: &str) -> &BTreeMap<String, BTreeSet<PathBuf>> {
        if !self.macros.contains_key(crate_name) {
            let mut index: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
            self.each_module(crate_name, &mut |module| {
                for item in module.items.iter() {
                    if let Item::Macro(definition) = item
                        && let Some(ident) = &definition.ident
                    {
                        index
                            .entry(ident.to_string())
                            .or_default()
                            .insert(module.file.clone());
                    }
                }
            });
            self.macros.insert(crate_name.to_owned(), index);
        }
        &self.macros[crate_name]
    }

    /// Every module of a library crate; a binary's is reached through its
    /// own root, which no other crate names.
    fn each_module(&self, crate_name: &str, visit: &mut dyn FnMut(&Module)) {
        let Some(root) = self.resolver.crate_root(crate_name) else {
            return;
        };
        let mut queue = vec![root];
        while let Some(module) = queue.pop() {
            visit(&module);
            queue.extend(self.resolver.submodules(&module).into_iter().flatten());
        }
    }
}

/// Paths, glob imports and macro invocations in a module's items.
#[derive(Default)]
struct References {
    paths: BTreeSet<Vec<String>>,
    globs: BTreeSet<Vec<String>>,
    macros: BTreeSet<Vec<String>>,
}

impl References {
    fn use_tree(&mut self, tree: &UseTree) {
        let mut imports = Vec::new();
        items::flatten(tree, &mut Vec::new(), &mut imports);
        for import in imports {
            match import {
                items::Import::Named { path, .. } => {
                    self.paths.insert(path);
                }
                items::Import::Glob { path } => {
                    self.globs.insert(path);
                }
            }
        }
    }

    /// Every `a::b` path in a token stream, for macro bodies and attribute
    /// arguments that `syn` leaves unparsed.
    fn tokens(&mut self, tokens: TokenStream) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let mut index = 0;
        while index < trees.len() {
            if let TokenTree::Group(group) = &trees[index] {
                self.tokens(group.stream());
                index += 1;
                continue;
            }
            let mut path = Vec::new();
            let mut at = index;
            while let Some(TokenTree::Ident(ident)) = trees.get(at) {
                path.push(ident.to_string());
                let separated = matches!(
                    (trees.get(at + 1), trees.get(at + 2)),
                    (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                        if a.as_char() == ':' && b.as_char() == ':'
                );
                if !separated {
                    break;
                }
                at += 3;
            }
            if path.len() > 1 {
                self.paths.insert(path);
            }
            index = at.max(index) + 1;
        }
    }
}

impl<'ast> Visit<'ast> for References {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.segments.len() > 1 {
            self.paths.insert(
                path.segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect(),
            );
        }
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.macros.insert(
            mac.path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect(),
        );
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }

    fn visit_meta_list(&mut self, list: &'ast syn::MetaList) {
        self.tokens(list.tokens.clone());
        visit::visit_meta_list(self, list);
    }

    fn visit_item_use(&mut self, declaration: &'ast syn::ItemUse) {
        self.use_tree(&declaration.tree);
    }

    fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {}
}

/// The file's identity and what its module doc declares.
fn read(file: &Path) -> Result<(String, Vec<String>, bool), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let identity = identity::source(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    let (requirements, critical) = declarations(&text);
    Ok((identity, requirements, critical))
}

/// `//! Requirements: A, B` lines and a `//! Safety-critical.` line.
pub fn declarations(text: &str) -> (Vec<String>, bool) {
    let mut requirements = Vec::new();
    let mut critical = false;
    for line in text.lines() {
        let Some(doc) = line.trim_start().strip_prefix("//!") else {
            continue;
        };
        let doc = doc.trim();
        if let Some(list) = doc.strip_prefix(REQUIREMENTS_LINE) {
            requirements.extend(
                list.split(',')
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned),
            );
        } else if doc == CRITICAL_LINE {
            critical = true;
        }
    }
    (requirements, critical)
}

/// Anything beyond module declarations and imports, inline modules included.
fn defines_code(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Use(_) | Item::ExternCrate(_) => false,
        Item::Mod(module) => module
            .content
            .as_ref()
            .is_some_and(|(_, inner)| defines_code(inner)),
        _ => true,
    })
}

fn item_declares(item: &Item, name: &str) -> bool {
    match item {
        Item::Mod(module) => module.ident == name,
        Item::Use(declaration) => {
            let mut imports = Vec::new();
            items::flatten(&declaration.tree, &mut Vec::new(), &mut imports);
            imports.iter().any(|import| {
                matches!(import, items::Import::Named { visible, path } if visible == name && path.len() > 1)
            })
        }
        _ => false,
    }
}

pub fn relative(repository: &Path, path: &Path) -> String {
    path.strip_prefix(repository)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Default, Deserialize)]
struct LockFile {
    #[serde(default)]
    package: Vec<LockPackage>,
}

#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

impl LockFile {
    /// The version of `dependency` that `package` resolves to.
    fn version(&self, package: &str, dependency: &str) -> Option<String> {
        let entry = self.package.iter().find(|found| found.name == package)?;
        let spec = entry
            .dependencies
            .iter()
            .find(|spec| spec.split_whitespace().next() == Some(dependency))?;
        match spec.split_whitespace().nth(1) {
            Some(version) => Some(version.to_owned()),
            None => self
                .package
                .iter()
                .find(|found| found.name == dependency)
                .map(|found| found.version.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_come_from_module_doc_lines() {
        let text = "//! Validates frames.\n//!\n//! Requirements: INPUT-001, INPUT-002\n//! Safety-critical.\n\n// Requirements: NOT-001\nfn f() {}\n";
        assert_eq!(
            declarations(text),
            (vec!["INPUT-001".to_owned(), "INPUT-002".to_owned()], true)
        );
        assert_eq!(declarations("fn f() {}"), (Vec::new(), false));
    }

    #[test]
    fn paths_are_read_from_macro_tokens() {
        let mut references = References::default();
        references.tokens("x == crate::a::B && foo(super::c) && d".parse().unwrap());
        let paths: Vec<Vec<String>> = references.paths.into_iter().collect();
        assert_eq!(
            paths,
            [
                vec!["crate".to_owned(), "a".into(), "B".into()],
                vec!["super".to_owned(), "c".into()]
            ]
        );
    }
}
