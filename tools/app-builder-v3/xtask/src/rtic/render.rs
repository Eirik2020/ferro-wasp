//! RTIC source rendering and reusable-task body extraction.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use proc_macro2::{LineColumn, Span, TokenStream, TokenTree};
use quote::ToTokens;
use syn::{
    Expr, ExprField, ExprPath, FnArg, Item, ItemFn, Member, Pat, Type, braced,
    parse::{Parse, ParseStream},
    spanned::Spanned,
    visit::Visit,
};

use crate::{
    hardware_definitions::stm32f4::lower::{self, RenderedHardware, RenderedInitLocal},
    rtic::{
        resolve::{ResolvedApp, ResolvedTask, ResolvedTaskTrigger},
        task::{ConfigValue, SpawnArgument},
    },
};

/// Renders one resolved application and verifies that the result is Rust syntax.
pub fn render(source_root: &Path, app: &ResolvedApp) -> Result<String> {
    let hardware = lower::lower(app).context("lower resolved application for STM32F4")?;
    let task_sources = app
        .tasks
        .iter()
        .map(|task| {
            read_task_source(source_root, task)
                .with_context(|| format!("read reusable body for task `{}`", task.id))
        })
        .collect::<Result<Vec<_>>>()?;
    validate_spawn_signatures(app, &task_sources)?;
    validate_init_spawn_signatures(app, &task_sources)?;
    let tasks = app
        .tasks
        .iter()
        .zip(&task_sources)
        .map(|(task, source)| render_task(task, source))
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");
    let rendered = render_app(app, &hardware, &tasks);
    syn::parse_file(&rendered).context("parse complete generated RTIC application")?;
    Ok(rendered)
}

#[derive(Clone)]
struct TaskSource {
    source: String,
    item: ItemFn,
}

struct ReusableTaskInput {
    body: ItemFn,
}

impl Parse for ReusableTaskInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let contract: syn::Ident = input.parse()?;
        if contract != "contract" {
            return Err(syn::Error::new(contract.span(), "expected `contract`"));
        }
        let contract_content;
        braced!(contract_content in input);
        let _contract_tokens: TokenStream = contract_content.parse()?;
        let body = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("reusable_task! accepts one contract and one function"));
        }
        Ok(Self { body })
    }
}

fn read_task_source(source_root: &Path, task: &ResolvedTask) -> Result<TaskSource> {
    let path = resolve_source_path(source_root, task.contract.source.file)?;
    let source = fs::read_to_string(&path)
        .with_context(|| format!("read task source {}", path.display()))?;
    let file = syn::parse_file(&source)
        .with_context(|| format!("parse task source {}", path.display()))?;
    let mut matches = Vec::new();
    for item in &file.items {
        let Item::Macro(item_macro) = item else {
            continue;
        };
        if !item_macro
            .mac
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "reusable_task")
        {
            continue;
        }
        let parsed: ReusableTaskInput = syn::parse2(item_macro.mac.tokens.clone())
            .with_context(|| format!("parse reusable_task! in {}", path.display()))?;
        if parsed.body.sig.ident == task.contract.source.function {
            matches.push(parsed.body);
        }
    }
    let [body] = matches.as_slice() else {
        bail!(
            "task source {} must contain exactly one reusable_task! function `{}`; found {}",
            path.display(),
            task.contract.source.function,
            matches.len()
        );
    };
    let body_span = body.span();
    let line_starts = source_line_starts(&source);
    let start = source_offset(&source, &line_starts, body_span.start())?;
    let end = source_offset(&source, &line_starts, body_span.end())?;
    let original = source
        .get(start..end)
        .context("task function span is outside its source file")?;
    let source = dedent(original, body_span.start().column);
    let item =
        syn::parse_str::<ItemFn>(&source).context("parse extracted reusable task function")?;
    validate_task_function(task, &item)?;
    Ok(TaskSource { source, item })
}

fn resolve_source_path(source_root: &Path, declared: &str) -> Result<PathBuf> {
    let declared = Path::new(declared);
    if declared.is_absolute() {
        return Ok(declared.to_owned());
    }
    let direct = source_root.join(declared);
    if direct.is_file() {
        return Ok(direct);
    }
    let manifest_relative = source_root.join("xtask").join(declared);
    if manifest_relative.is_file() {
        return Ok(manifest_relative);
    }
    bail!(
        "declared task source `{}` does not exist below {}",
        declared.display(),
        source_root.display()
    )
}

fn validate_task_function(task: &ResolvedTask, body: &ItemFn) -> Result<()> {
    if body.sig.ident != task.contract.id {
        bail!(
            "task contract `{}` points to function `{}`",
            task.contract.id,
            body.sig.ident
        );
    }
    if !body.sig.generics.params.is_empty() || body.sig.generics.where_clause.is_some() {
        bail!("reusable task `{}` must not be generic", task.contract.id);
    }
    let context = body
        .sig
        .inputs
        .first()
        .with_context(|| format!("task `{}` has no context argument", task.id))?;
    let FnArg::Typed(context) = context else {
        bail!("task `{}` context cannot be a receiver", task.id);
    };
    let Type::Path(context_type) = context.ty.as_ref() else {
        bail!(
            "task `{}` context must use its generated Context type",
            task.id
        );
    };
    let Some(first) = context_type.path.segments.first() else {
        bail!("task `{}` context type has no task module", task.id);
    };
    if first.ident != task.contract.id {
        bail!(
            "task `{}` context uses `{}` instead of `{}`",
            task.id,
            first.ident,
            task.contract.id
        );
    }
    match task.trigger {
        ResolvedTaskTrigger::Interrupt(_) => {
            if body.sig.asyncness.is_some() {
                bail!("interrupt task `{}` must be synchronous", task.id);
            }
            if body.sig.inputs.len() != 1 {
                bail!("interrupt task `{}` cannot accept spawn arguments", task.id);
            }
        }
        ResolvedTaskTrigger::Software => {
            if body.sig.asyncness.is_none() {
                bail!(
                    "software task `{}` must be asynchronous for RTIC 2",
                    task.id
                );
            }
        }
    }
    Ok(())
}

fn validate_spawn_signatures(app: &ResolvedApp, sources: &[TaskSource]) -> Result<()> {
    let by_id = app
        .tasks
        .iter()
        .zip(sources)
        .map(|(task, source)| (task.id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    for task in &app.tasks {
        for spawn in &task.spawns {
            let target = by_id.get(spawn.target.as_str()).with_context(|| {
                format!(
                    "task `{}` spawn target `{}` disappeared after resolution",
                    task.id, spawn.target
                )
            })?;
            validate_task_arguments(&spawn.target, &target.item, spawn.arguments)?;
        }
    }
    Ok(())
}

fn validate_init_spawn_signatures(app: &ResolvedApp, sources: &[TaskSource]) -> Result<()> {
    for task_id in &app.init_spawns {
        let index = app
            .tasks
            .iter()
            .position(|task| task.id == *task_id)
            .with_context(|| {
                format!("init-spawned task `{task_id}` disappeared after resolution")
            })?;
        if sources[index].item.sig.inputs.len() != 1 {
            bail!("init-spawned task `{task_id}` requires arguments");
        }
    }
    Ok(())
}

fn validate_task_arguments(task_id: &str, body: &ItemFn, expected: &[SpawnArgument]) -> Result<()> {
    let actual = body.sig.inputs.iter().skip(1).collect::<Vec<_>>();
    if actual.len() != expected.len() {
        bail!(
            "spawn target `{task_id}` expects {} argument(s), but its spawn contract supplies {}",
            actual.len(),
            expected.len()
        );
    }
    for (actual, expected) in actual.into_iter().zip(expected) {
        let FnArg::Typed(actual) = actual else {
            bail!("spawn target `{task_id}` cannot contain a receiver");
        };
        let Pat::Ident(pattern) = actual.pat.as_ref() else {
            bail!(
                "spawn target `{task_id}` argument `{}` must be a named identifier",
                expected.id
            );
        };
        if pattern.ident != expected.id {
            bail!(
                "spawn target `{task_id}` argument is `{}`, expected `{}`",
                pattern.ident,
                expected.id
            );
        }
        let expected_type = syn::parse_str::<Type>(expected.rust_type)
            .with_context(|| format!("parse spawn argument type `{}`", expected.rust_type))?;
        if normalize_tokens(actual.ty.as_ref()) != normalize_tokens(&expected_type) {
            bail!(
                "spawn target `{task_id}` argument `{}` has type `{}`, expected `{}`",
                expected.id,
                actual.ty.to_token_stream(),
                expected.rust_type
            );
        }
    }
    Ok(())
}

fn normalize_tokens(tokens: &impl ToTokens) -> String {
    tokens.to_token_stream().to_string().replace(' ', "")
}

#[derive(Debug)]
struct SourceReplacement {
    span: Span,
    expected: Option<String>,
    replacement: String,
}

struct TaskBodyVisitor<'a> {
    task_id: &'a str,
    local: BTreeMap<&'a str, &'a str>,
    shared: BTreeMap<&'a str, &'a str>,
    config: BTreeMap<&'a str, ConfigValue>,
    spawns: BTreeMap<&'a str, &'a str>,
    used_config: BTreeSet<String>,
    replacements: Vec<SourceReplacement>,
    errors: Vec<String>,
}

impl TaskBodyVisitor<'_> {
    fn resource(&mut self, kind: &str, field: &syn::Ident) {
        let field_name = field.to_string();
        let bindings = if kind == "local" {
            &self.local
        } else {
            &self.shared
        };
        let Some(target) = bindings.get(field_name.as_str()) else {
            self.errors.push(format!(
                "task `{}` body uses cx.{kind}.{field_name} without a resolved binding",
                self.task_id
            ));
            return;
        };
        self.replacements.push(SourceReplacement {
            span: field.span(),
            expected: Some(field_name),
            replacement: (*target).to_owned(),
        });
    }

    fn config(&mut self, field: &syn::Ident, span: Span) {
        let field_name = field.to_string();
        let Some(value) = self.config.get(field_name.as_str()).copied() else {
            self.errors.push(format!(
                "task `{}` body uses cx.config.{field_name} without a resolved value",
                self.task_id
            ));
            return;
        };
        self.used_config.insert(field_name);
        self.replacements.push(SourceReplacement {
            span,
            expected: None,
            replacement: render_config_value(value),
        });
    }

    fn macro_tokens(&mut self, tokens: TokenStream) {
        let tokens = tokens.into_iter().collect::<Vec<_>>();
        for token in &tokens {
            if let TokenTree::Group(group) = token {
                self.macro_tokens(group.stream());
            }
        }
        for tokens in tokens.windows(5) {
            let [
                TokenTree::Ident(context),
                TokenTree::Punct(first_dot),
                TokenTree::Ident(kind),
                TokenTree::Punct(second_dot),
                TokenTree::Ident(field),
            ] = tokens
            else {
                continue;
            };
            if context == "cx" && first_dot.as_char() == '.' && second_dot.as_char() == '.' {
                if kind == "local" || kind == "shared" {
                    self.resource(if kind == "local" { "local" } else { "shared" }, field);
                } else if kind == "config" {
                    if let Some(span) = context.span().join(field.span()) {
                        self.config(field, span);
                    } else {
                        self.errors.push(format!(
                            "task `{}` cannot rewrite cx.config.{} inside macro tokens",
                            self.task_id, field
                        ));
                    }
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for TaskBodyVisitor<'_> {
    fn visit_expr_field(&mut self, field: &'ast ExprField) {
        if let Some((kind, resource)) = context_resource_field(field) {
            self.resource(kind, resource);
        } else if let Some(config) = context_config_field(field) {
            self.config(config, field.span());
        }
        syn::visit::visit_expr_field(self, field);
    }

    fn visit_expr_path(&mut self, path: &'ast ExprPath) {
        if path.path.segments.len() == 2
            && path.path.segments[1].ident == "spawn"
            && let Some(logical) = path.path.segments.first()
        {
            let logical_name = logical.ident.to_string();
            if let Some(target) = self.spawns.get(logical_name.as_str()) {
                self.replacements.push(SourceReplacement {
                    span: logical.ident.span(),
                    expected: Some(logical_name),
                    replacement: (*target).to_owned(),
                });
            }
        }
        syn::visit::visit_expr_path(self, path);
    }

    fn visit_macro(&mut self, task_macro: &'ast syn::Macro) {
        self.macro_tokens(task_macro.tokens.clone());
        syn::visit::visit_macro(self, task_macro);
    }
}

fn context_resource_field(field: &ExprField) -> Option<(&str, &syn::Ident)> {
    let Member::Named(resource) = &field.member else {
        return None;
    };
    let Expr::Field(group) = field.base.as_ref() else {
        return None;
    };
    let Member::Named(kind) = &group.member else {
        return None;
    };
    if kind != "local" && kind != "shared" {
        return None;
    }
    let Expr::Path(context) = group.base.as_ref() else {
        return None;
    };
    context
        .path
        .is_ident("cx")
        .then_some((if kind == "local" { "local" } else { "shared" }, resource))
}

fn context_config_field(field: &ExprField) -> Option<&syn::Ident> {
    let Member::Named(config) = &field.member else {
        return None;
    };
    let Expr::Field(group) = field.base.as_ref() else {
        return None;
    };
    let Member::Named(kind) = &group.member else {
        return None;
    };
    if kind != "config" {
        return None;
    }
    let Expr::Path(context) = group.base.as_ref() else {
        return None;
    };
    context.path.is_ident("cx").then_some(config)
}

fn render_task(task: &ResolvedTask, source: &TaskSource) -> Result<String> {
    let local = task
        .local
        .iter()
        .map(|binding| (binding.logical, binding.target.as_str()))
        .collect::<BTreeMap<_, _>>();
    let shared = task
        .shared
        .iter()
        .map(|binding| (binding.logical, binding.target.as_str()))
        .collect::<BTreeMap<_, _>>();
    let config = task
        .config
        .iter()
        .map(|binding| (binding.logical, binding.value))
        .collect::<BTreeMap<_, _>>();
    let spawns = task
        .spawns
        .iter()
        .map(|binding| (binding.logical, binding.target.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut visitor = TaskBodyVisitor {
        task_id: &task.id,
        local,
        shared,
        config,
        spawns,
        used_config: BTreeSet::new(),
        replacements: Vec::new(),
        errors: Vec::new(),
    };
    visitor.visit_item_fn(&source.item);
    if !visitor.errors.is_empty() {
        bail!(visitor.errors.join("; "));
    }
    for binding in &task.config {
        if !visitor.used_config.contains(binding.logical) {
            bail!(
                "task `{}` binds configuration `{}` but its body does not use it",
                task.id,
                binding.logical
            );
        }
    }

    visitor.replacements.push(SourceReplacement {
        span: source.item.sig.ident.span(),
        expected: Some(task.contract.id.to_owned()),
        replacement: task.id.clone(),
    });
    let context = source
        .item
        .sig
        .inputs
        .first()
        .context("validated task has no context")?;
    let FnArg::Typed(context) = context else {
        unreachable!("task context validation rejects receivers")
    };
    visitor.replacements.push(SourceReplacement {
        span: context.ty.span(),
        expected: None,
        replacement: format!("{}::Context", task.id),
    });
    let visibility = source.item.vis.span();
    visitor.replacements.push(SourceReplacement {
        span: visibility,
        expected: Some("pub".to_owned()),
        replacement: format!("{}\n", render_task_attribute(task)),
    });

    let rendered = apply_source_replacements(&source.source, visitor.replacements)?;
    Ok(rendered
        .replacen("\n async fn ", "\nasync fn ", 1)
        .replacen("\n fn ", "\nfn ", 1))
}

fn render_task_attribute(task: &ResolvedTask) -> String {
    let mut fields = Vec::new();
    if let ResolvedTaskTrigger::Interrupt(binding) = &task.trigger {
        fields.push(format!("binds = {binding}"));
    }
    fields.push(format!("priority = {}", task.priority));
    if !task.local.is_empty() {
        fields.push(format!(
            "local = [{}]",
            task.local
                .iter()
                .map(|binding| binding.target.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !task.shared.is_empty() {
        fields.push(format!(
            "shared = [{}]",
            task.shared
                .iter()
                .map(|binding| binding.target.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let compact = format!("#[task({})]", fields.join(", "));
    if compact.len() <= 84 {
        compact
    } else {
        format!("#[task(\n    {}\n)]", fields.join(",\n    "))
    }
}

fn render_config_value(value: ConfigValue) -> String {
    match value {
        ConfigValue::Bool(value) => value.to_string(),
        ConfigValue::U32(value) => render_u32(value),
        ConfigValue::Millis(value) => format!("{}.millis()", render_u32(value.ticks())),
    }
}

fn apply_source_replacements(source: &str, replacements: Vec<SourceReplacement>) -> Result<String> {
    let line_starts = source_line_starts(source);
    let mut replacements = replacements
        .into_iter()
        .map(|replacement| {
            let start = source_offset(source, &line_starts, replacement.span.start())?;
            let end = source_offset(source, &line_starts, replacement.span.end())?;
            if let Some(expected) = replacement.expected
                && source.get(start..end) != Some(expected.as_str())
            {
                bail!(
                    "task source span expected `{expected}`, found `{}`",
                    source.get(start..end).unwrap_or("<invalid UTF-8 boundary>")
                );
            }
            Ok((start, end, replacement.replacement))
        })
        .collect::<Result<Vec<_>>>()?;
    replacements.sort_by_key(|replacement| Reverse(replacement.0));
    let mut rendered = source.to_owned();
    let mut next_start = source.len();
    for (start, end, replacement) in replacements {
        if end > next_start {
            bail!("task source replacements overlap at byte {start}");
        }
        rendered.replace_range(start..end, &replacement);
        next_start = start;
    }
    Ok(rendered)
}

fn render_app(app: &ResolvedApp, hardware: &RenderedHardware, tasks: &str) -> String {
    let dispatchers = app.dispatchers.join(", ");
    let shared = render_resource_struct("Shared", &hardware.shared_fields);
    let local = render_resource_struct("Local", &hardware.local_fields);
    let init_attribute = render_init_attribute(&hardware.init_locals);
    let shared_value = render_resource_value("Shared", &hardware.shared_values);
    let local_value = render_resource_value("Local", &hardware.local_values);
    let init_spawns = app
        .init_spawns
        .iter()
        .map(|task| format!("{task}::spawn().expect(\"init must spawn declared task {task}\");"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        concat!(
            "// GENERATED FILE — DO NOT EDIT DIRECTLY\n",
            "// Generated from xtask/src/target/board.rs and app_composition.rs.\n\n",
            "#![no_main]\n",
            "#![no_std]\n",
            "#![forbid(unsafe_code)]\n",
            "#![deny(warnings)]\n\n",
            "// Endpoint exports may intentionally have no consumer in a partial composition.\n",
            "#![allow(dead_code)]\n\n",
            "use defmt_rtt as _;\n",
            "use panic_halt as _;\n\n",
            "#[rtic::app(\n",
            "    device = ferrowasp_stm32f4::rtic::hal::pac,\n",
            "    peripherals = true,\n",
            "    dispatchers = [{dispatchers}]\n",
            ")]\n",
            "mod app {{\n",
            "{imports}\n\n",
            "{timing}\n\n",
            "{shared}\n\n",
            "{local}\n\n",
            "{init_attribute}\n",
            "    fn init(cx: init::Context) -> (Shared, Local) {{\n",
            "{initialization}\n\n",
            "{init_spawns}\n\n",
            "        ({shared_value}, {local_value})\n",
            "    }}\n\n",
            "{tasks}\n",
            "}}\n",
        ),
        dispatchers = dispatchers,
        imports = indent(&hardware.imports, 4),
        timing = indent(&hardware.timing, 4),
        shared = indent(&shared, 4),
        local = indent(&local, 4),
        init_attribute = indent(&init_attribute, 4),
        initialization = indent(&hardware.initialization, 8),
        init_spawns = indent(&init_spawns, 8),
        shared_value = shared_value,
        local_value = local_value,
        tasks = indent(tasks, 4),
    )
}

fn render_resource_struct(name: &str, fields: &[String]) -> String {
    if fields.is_empty() {
        return format!("#[{}]\nstruct {name} {{}}", name.to_ascii_lowercase());
    }
    format!(
        "#[{}]\nstruct {name} {{\n{}\n}}",
        name.to_ascii_lowercase(),
        indent(&fields.join("\n"), 4)
    )
}

fn render_resource_value(name: &str, fields: &[String]) -> String {
    if fields.is_empty() {
        format!("{name} {{}}")
    } else {
        format!("{name} {{ {} }}", fields.join(", "))
    }
}

fn render_init_attribute(resources: &[RenderedInitLocal]) -> String {
    if resources.is_empty() {
        "#[init]".to_owned()
    } else {
        let fields = resources
            .iter()
            .map(|resource| {
                format!(
                    "{}: {} = {}",
                    resource.id, resource.rust_type, resource.initializer
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!("#[init(local = [\n{}\n])]", indent(&fields, 4))
    }
}

fn indent(source: &str, spaces: usize) -> String {
    if source.is_empty() {
        return String::new();
    }
    let prefix = " ".repeat(spaces);
    source
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn source_line_starts(source: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        )
        .collect()
}

fn source_offset(source: &str, line_starts: &[usize], location: LineColumn) -> Result<usize> {
    let line = location
        .line
        .checked_sub(1)
        .context("source spans use one-based line numbers")?;
    let line_start = *line_starts
        .get(line)
        .with_context(|| format!("source span line {} is outside the file", location.line))?;
    let offset = line_start + location.column;
    if offset > source.len() || !source.is_char_boundary(offset) {
        bail!("source span offset {offset} is outside a UTF-8 boundary");
    }
    Ok(offset)
}

fn dedent(source: &str, indentation: usize) -> String {
    let padding = " ".repeat(indentation);
    source
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                line
            } else {
                line.strip_prefix(&padding).unwrap_or(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_u32(value: u32) -> String {
    let digits = value.to_string();
    let mut rendered = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            rendered.push('_');
        }
        rendered.push(digit);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rtic::resolve, target::app_composition::APP_COMPOSITION};

    fn source_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn current_composition_renders_parseable_rtic_source() {
        let app = resolve::resolve(&APP_COMPOSITION).unwrap();
        let rendered = render(&source_root(), &app).unwrap();

        assert!(syn::parse_file(&rendered).is_ok());
        assert!(rendered.contains("binds = EXTI15_10"));
        assert!(rendered.contains("binds = DMA1_STREAM4"));
        assert!(rendered.contains("fn osd_uart_tx_worker"));
        assert!(rendered.contains("observe_button_change::spawn(enabled)"));
        assert!(rendered.contains("Mono::delay(500.millis()).await"));
        assert!(!rendered.contains("cx.config"));
        assert!(!rendered.contains("button_changed::spawn"));
        assert!(!rendered.contains("Context<'_>"));
    }

    #[test]
    fn rendering_is_deterministic() {
        let app = resolve::resolve(&APP_COMPOSITION).unwrap();
        assert_eq!(
            render(&source_root(), &app).unwrap(),
            render(&source_root(), &app).unwrap()
        );
    }
}
