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

/// Generated Rust source files for one resolved application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedSources {
    /// RTIC application crate root.
    pub main: String,
    /// Crate-local imports consumed by the RTIC application module.
    pub prelude: String,
    /// Boot-time service assignments lowered for the generated firmware.
    pub platform_config: String,
}

/// Renders one resolved application and verifies that both files are Rust syntax.
pub fn render_sources(source_root: &Path, app: &ResolvedApp) -> Result<RenderedSources> {
    let hardware = lower::lower(app).context("lower resolved application for STM32F4")?;
    let safety = render_safety_resources(app);
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
    let main = render_app(app, &hardware, &safety, &tasks);
    let prelude = render_prelude(&hardware.imports, !app.safety_channels.is_empty());
    let platform_config = render_platform_config(&hardware.platform_config);
    syn::parse_file(&main).context("parse complete generated RTIC application")?;
    syn::parse_file(&prelude).context("parse generated RTIC prelude")?;
    syn::parse_file(&platform_config).context("parse generated platform configuration")?;
    Ok(RenderedSources {
        main,
        prelude,
        platform_config,
    })
}

/// Renders the generated RTIC crate root.
pub fn render(source_root: &Path, app: &ResolvedApp) -> Result<String> {
    Ok(render_sources(source_root, app)?.main)
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
        ConfigValue::Millis(value) => {
            format!("{}.millis().into()", render_u32(value.ticks()))
        }
        ConfigValue::FrameRotation(value) => format!(
            "FrameRotation::new([{}, {}, {}], [{}, {}, {}])",
            value.source_axes[0],
            value.source_axes[1],
            value.source_axes[2],
            value.signs[0],
            value.signs[1],
            value.signs[2],
        ),
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

#[derive(Default)]
struct RenderedSafetyResources {
    local_fields: Vec<String>,
    local_values: Vec<String>,
    init_locals: Vec<RenderedInitLocal>,
    initialization: String,
}

fn render_safety_resources(app: &ResolvedApp) -> RenderedSafetyResources {
    let mut rendered = RenderedSafetyResources::default();
    let mut initialization = Vec::new();
    for channel in &app.safety_channels {
        let section = format!("Safety channel `{}` resources", channel.id);
        let guard = format!("// ===== {section} =====");
        rendered.local_fields.push(guard.clone());
        rendered.local_values.push(guard);
        rendered.local_fields.push(format!(
            "{}: SafetyProducer<'static, {}>,",
            channel.producer.id,
            channel.message.rust_type()
        ));
        rendered.local_fields.push(format!(
            "{}: SafetyConsumer<'static, {}>,",
            channel.consumer.id,
            channel.message.rust_type()
        ));
        rendered.local_values.push(channel.producer.id.to_owned());
        rendered.local_values.push(channel.consumer.id.to_owned());

        rendered.init_locals.push(RenderedInitLocal {
            section,
            id: channel.id.to_owned(),
            rust_type: format!(
                "SafetyChannel<{}, {}>",
                channel.message.rust_type(),
                channel.usable_capacity + 1
            ),
            initializer: "SafetyChannel::new()".to_owned(),
        });
        initialization.push(format!(
            "// ===== Safety channel `{}` initialization =====\nlet ({}, {}) = cx.local.{}.split();",
            channel.id, channel.producer.id, channel.consumer.id, channel.id
        ));
    }
    rendered.initialization = initialization.join("\n\n");
    rendered
}

fn render_app(
    app: &ResolvedApp,
    hardware: &RenderedHardware,
    safety: &RenderedSafetyResources,
    tasks: &str,
) -> String {
    let dispatchers = app.dispatchers.join(", ");
    let shared = render_resource_struct("Shared", &hardware.shared_fields);
    let local_fields = hardware
        .local_fields
        .iter()
        .chain(&safety.local_fields)
        .cloned()
        .collect::<Vec<_>>();
    let local = render_resource_struct("Local", &local_fields);
    let init_locals = hardware
        .init_locals
        .iter()
        .chain(&safety.init_locals)
        .cloned()
        .collect::<Vec<_>>();
    let init_attribute = render_init_attribute(&init_locals);
    let shared_value = render_resource_value("Shared", &hardware.shared_values);
    let local_values = hardware
        .local_values
        .iter()
        .chain(&safety.local_values)
        .cloned()
        .collect::<Vec<_>>();
    let local_value = render_resource_value("Local", &local_values);
    let initialization = match (
        hardware.initialization.is_empty(),
        safety.initialization.is_empty(),
    ) {
        (false, false) => format!("{}\n\n{}", hardware.initialization, safety.initialization),
        (false, true) => hardware.initialization.clone(),
        (true, false) => safety.initialization.clone(),
        (true, true) => String::new(),
    };
    let init_spawns = app
        .init_spawns
        .iter()
        .map(|task| format!("{task}::spawn().expect(\"init must spawn declared task {task}\");"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        concat!(
            "// GENERATED FILE — DO NOT EDIT DIRECTLY\n",
            "// Generated from xtask/src/target/board.rs, app_composition.rs, and platform_config.rs.\n\n",
            "#![no_main]\n",
            "#![no_std]\n",
            "#![forbid(unsafe_code)]\n",
            "#![deny(warnings)]\n\n",
            "// Endpoint exports may intentionally have no consumer in a partial composition.\n",
            "#![allow(dead_code)]\n\n",
            "mod platform_config;\n",
            "mod prelude;\n\n",
            "#[rtic::app(\n",
            "    device = ferrowasp_stm32f4::rtic::hal::pac,\n",
            "    peripherals = true,\n",
            "    dispatchers = [{dispatchers}]\n",
            ")]\n",
            "mod app {{\n",
            "    use crate::prelude::*;\n\n",
            "{globals}\n\n",
            "{timing}\n\n",
            "{shared}\n\n",
            "{local}\n\n",
            "{init_attribute}\n",
            "    fn init(cx: init::Context) -> (Shared, Local) {{\n",
            "{initialization}\n\n",
            "        // ===== Initial task spawns =====\n",
            "{init_spawns}\n\n",
            "        // ===== RTIC resource handoff =====\n",
            "        ({shared_value}, {local_value})\n",
            "    }}\n\n",
            "{tasks}\n",
            "}}\n",
        ),
        dispatchers = dispatchers,
        globals = indent(&hardware.globals, 4),
        timing = indent(&hardware.timing, 4),
        shared = indent(&shared, 4),
        local = indent(&local, 4),
        init_attribute = indent(&init_attribute, 4),
        initialization = indent(&initialization, 8),
        init_spawns = indent(&init_spawns, 8),
        shared_value = shared_value,
        local_value = local_value,
        tasks = indent(tasks, 4),
    )
}

fn render_prelude(imports: &str, has_safety_channels: bool) -> String {
    let safety_imports = if has_safety_channels {
        concat!(
            "pub(crate) use ferrowasp_core::{\n",
            "    safety::{\n",
            "        ActuatorAuthority, ActuatorCmd, ActuatorGuardReport, ActuatorPreparationReport,\n",
            "        ArmingAbortReason, MotorCmd, PreArmHealth, PreArmHealthReport, RcLinkInvalidation,\n",
            "        MOTOR_CMD_MAX_AGE_MS,\n",
            "    },\n",
            "    safety_channel::{SafetyChannel, SafetyConsumer, SafetyProducer},\n",
            "};\n",
            "pub(crate) use ferrowasp_io_core::serial::RcInputSnapshot;\n"
        )
    } else {
        ""
    };
    format!(
        concat!(
            "// GENERATED FILE — DO NOT EDIT DIRECTLY\n",
            "// Generated with src/main.rs from the selected application declarations.\n\n",
            "//! Crate-local imports used by the generated RTIC application.\n\n",
            "#![allow(unused_imports)]\n\n",
            "use defmt_rtt as _;\n",
            "use panic_halt as _;\n\n",
            "pub(crate) use crate::platform_config::load_platform_config;\n",
            "{safety_imports}",
            "{imports}\n",
        ),
        safety_imports = safety_imports,
        imports = imports,
    )
}

fn render_platform_config(configuration: &str) -> String {
    format!(
        concat!(
            "// GENERATED FILE — DO NOT EDIT DIRECTLY\n",
            "// Lowered from xtask/src/target/platform_config.rs.\n\n",
            "//! Boot-time service assignments for the generated firmware.\n\n",
            "use ferrowasp_io_core::platform_config::{{\n",
            "    ImuInstallationId, RuntimePlatformConfig, SerialService, SpiService,\n",
            "}};\n\n",
            "{configuration}\n",
        ),
        configuration = configuration,
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
        let fields = fields
            .iter()
            .map(|field| {
                if field.starts_with("//") {
                    field.clone()
                } else {
                    format!("{field},")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("{name} {{\n{}\n}}", indent(&fields, 4))
    }
}

fn render_init_attribute(resources: &[RenderedInitLocal]) -> String {
    if resources.is_empty() {
        "#[init]".to_owned()
    } else {
        let mut lines = Vec::new();
        let mut previous_section = None;
        for resource in resources {
            if previous_section != Some(resource.section.as_str()) {
                lines.push(format!("// ===== {} =====", resource.section));
                previous_section = Some(resource.section.as_str());
            }
            lines.push(format!(
                "{}: {} = {},",
                resource.id, resource.rust_type, resource.initializer
            ));
        }
        format!("#[init(local = [\n{}\n])]", indent(&lines.join("\n"), 4))
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
    use crate::{
        rtic::{
            platform_config::{ImuInstallationId, PlatformConfig, SerialService, SpiService},
            resolve,
        },
        target::app_composition::APP_COMPOSITION,
    };

    fn source_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn current_composition_renders_parseable_rtic_source() {
        let app = resolve::resolve(&APP_COMPOSITION).unwrap();
        let rendered = render_sources(&source_root(), &app).unwrap();
        let main = &rendered.main;
        let prelude = &rendered.prelude;
        let platform_config = &rendered.platform_config;

        syn::parse_file(main).unwrap_or_else(|error| panic!("generated main must parse: {error}"));
        syn::parse_file(prelude)
            .unwrap_or_else(|error| panic!("generated prelude must parse: {error}"));
        assert!(main.contains("mod prelude;"));
        assert!(main.contains("mod platform_config;"));
        assert!(main.contains("use crate::prelude::*;"));
        assert!(main.contains("binds = EXTI4"));
        assert!(main.contains("binds = DMA1_STREAM4"));
        assert!(main.contains("binds = USART2"));
        assert!(main.contains("binds = DMA1_STREAM5"));
        assert!(main.contains("binds = DMA2_STREAM0"));
        assert!(main.contains("binds = TIM4"));
        assert!(main.contains("priority = 14"));
        assert!(main.contains("fn serial2_tx_worker"));
        assert!(main.contains("priority = 16"));
        assert!(main.contains("async fn safety_master"));
        assert!(main.contains("FoxeerSafetyMaster::output_inhibited()"));
        assert!(!main.contains("FoxeerSafetyMaster::new(true)"));
        assert!(main.contains("rc_sbus_rx_discontinuities"));
        assert!(main.contains("RcLinkInvalidation::DmaError"));
        assert!(main.contains("RcLinkInvalidation::TransportDiscontinuity"));
        assert!(main.contains("Mono::timeout_after"));
        assert!(main.contains("10.millis().into()"));
        assert!(main.contains(".foxeer_safety_state.poll(now_us)"));
        assert!(main.contains("async fn msp_osd"));
        assert!(main.contains("next_menu_frame"));
        assert!(main.contains("next_overlay_frame"));
        assert!(!main.contains("async fn simple_osd"));
        assert!(main.contains("async fn imu_control_bridge"));
        assert!(main.contains("fn control_loop"));
        assert!(!main.contains("async fn control_loop"));
        assert!(main.contains("acknowledge_control_tick(cx.local.control_scheduler)"));
        assert!(main.contains("if *cx.local.control_phase < 2"));
        assert!(main.contains("flight_controller: dt::FlightController"));
        assert!(main.contains("let samples_per_control_loop = 2"));
        assert!(main.contains("dt::ImuRateLowPassFilter::new(0.55)"));
        assert!(main.contains("dt::GyroBiasCalibrator::new(800, 1_000)"));
        assert!(main.contains("ferrowasp_core::frames::BODY_RATE_TO_RATE_CONTROLLER_MAP"));
        assert!(main.contains("motor_cmd_seq: u32"));
        assert!(main.contains("local = [control_scheduler, control_phase"));
        assert!(main.contains("sbus_to_control: SafetyChannel<RcInputSnapshot, 5>"));
        assert!(main.contains("imu_to_control: SafetyChannel<ImuData, 5>"));
        assert!(main.contains("control_to_actuator: SafetyChannel<MotorCmd, 4>"));
        assert!(main.contains("control_to_safety_health: SafetyChannel<PreArmHealthReport, 5>"));
        assert!(
            main.contains("prearm_health_producer: SafetyProducer<'static, PreArmHealthReport>")
        );
        assert!(
            main.contains("prearm_health_consumer: SafetyConsumer<'static, PreArmHealthReport>")
        );
        assert!(main.contains("observe_control_health(report, now_us)"));
        assert!(main.contains("imu_bias_calibrated: cx.local.gyro_bias_calibrator.ready()"));
        assert!(main.contains("armed: false"));
        assert!(main.contains("run_foxeer_control_step"));
        assert!(main.contains("cx.local.motor_cmd_producer.try_send(command)"));
        assert!(main.contains("MotorQueueOutcome::Accepted"));
        assert!(main.contains("MotorQueueOutcome::Full"));
        assert!(main.contains("MotorPublishOutcome::WakeRejected"));
        assert!(main.contains("actuator_output::spawn(ActuatorCmd::ApplyLatestThrottle).is_ok()"));
        assert!(main.contains("local = [motor_cmd_consumer, actuator_guard_consumer, actuator_completion_producer, esc_telemetry_update_consumer]"));
        assert!(main.contains("shared = [dshot_motors]"));
        assert!(main.contains("async fn actuator_output"));
        assert!(main.contains("request: ferrowasp_core::safety::ActuatorCmd"));
        assert!(main.contains("handle_inhibited_actuator_wake"));
        assert!(!main.contains("actuator_output::spawn()"));
        for required in [
            "cx.device.TIM1",
            "cx.device.TIM8",
            "cx.device.USART1",
            "gpioa.pa8",
            "gpioc.pc9",
            "gpioc.pc8",
            "gpiob.pb15",
            "dma2.1",
            "dma2.7",
            "dma2.2",
            "dma2.6",
            "dma2.5",
            "if !false",
            "DshotMotorBank::new_foxeer",
            "DSHOT_IDLE_QUALIFICATION_CONFIG",
        ] {
            assert!(
                main.contains(required),
                "missing physical actuator evidence `{required}`"
            );
        }
        assert!(!main.contains("ActuatorHardware"));
        assert!(main.contains("service_snapshot.msp_snapshot(now_ms)"));
        assert!(main.contains("MSP DisplayPort RX discontinuity"));
        assert!(
            platform_config
                .contains("pub(crate) fn load_platform_config() -> RuntimePlatformConfig")
        );
        assert!(main.contains("match (platform_config.serial1, platform_config.serial2)"));
        assert!(main.contains("(Some(SerialService::RcSbus), Some(SerialService::MspV1Osd))"));
        assert!(main.contains("(Some(SerialService::MspV1Osd), Some(SerialService::RcSbus))"));
        assert!(!main.contains("does not match generated RTIC resources"));
        assert!(!main.contains("rc_sbus_rx_buffer"));
        assert!(!main.contains("osd_uart"));
        assert!(main.contains("match platform_config.spi1"));
        assert!(main.contains("static SPI1_MAILBOX: Spi1ImuMailbox"));
        assert!(main.contains("// ===== Common initialization ====="));
        assert!(main.contains("// ===== Serial endpoint `serial2` initialization ====="));
        assert!(main.contains("// ===== Boot-time serial service routing ====="));
        assert!(main.contains("// ===== SPI endpoint `spi1` initialization ====="));
        assert!(main.contains("// ===== Initial task spawns ====="));
        assert!(main.contains("// ===== RTIC resource handoff ====="));
        assert!(
            main.matches("// ===== Serial endpoint `serial2` resources =====")
                .count()
                >= 3
        );
        assert!(
            main.matches("// ===== SPI endpoint `spi1` resources =====")
                .count()
                >= 3
        );
        assert!(prelude.contains("spi_imu_endpoint::{"));
        assert!(!main.contains("struct Spi1ImuEndpointOwner"));
        assert!(!main.contains("struct Spi1ImuDevice"));
        assert!(!main.contains("struct Spi1ImuParser"));
        assert!(main.contains("fn spi1_poll"));
        assert!(main.contains("spi1_poll::spawn(observed_at_us)"));
        assert!(main.contains("FrameRotation::new([1, 0, 2], [-1, -1, -1]).map_f32(acc)"));
        assert!(main.contains("ImuInstallationId::new(1)"));
        assert!(main.contains("stm32_tim2_monotonic!(Mono, 1_000_000)"));
        assert!(main.contains("Mono::start(rcc.clocks.timclk1().raw())"));
        assert!(main.contains("Mono::now().duration_since_epoch().to_micros()"));
        assert!(!main.contains("Spi1ImuTimebase"));
        assert!(!main.contains("spi1_timebase"));
        assert!(!main.contains("spi1_delay"));
        assert!(!main.contains("cx.device.TIM2"));
        assert!(main.contains("let mut init_delay = cx.device.TIM5.delay::<1_000_000>"));
        assert!(main.contains("cx.device.TIM4"));
        assert!(main.contains("800.Hz()"));
        assert!(prelude.contains("scheduler::{acknowledge_control_tick, init_control_scheduler}"));
        assert!(main.contains("Mono::delay(1.millis().into()).await"));
        assert!(!main.contains("cx.config"));
        assert!(!main.contains("if poll::spawn"));
        assert!(!main.contains("Context<'_>"));
    }

    #[test]
    fn safety_channel_renders_private_storage_and_exclusive_local_handles() {
        let app = resolve::resolve(&APP_COMPOSITION).unwrap();

        let rendered = render_sources(&source_root(), &app).unwrap();
        assert!(
            rendered.main.contains(
                "control_to_actuator: SafetyChannel<MotorCmd, 4> = SafetyChannel::new(),"
            )
        );
        assert!(rendered.main.contains(
            "let (motor_cmd_producer, motor_cmd_consumer) = cx.local.control_to_actuator.split();"
        ));
        assert_eq!(
            rendered
                .main
                .matches("cx.local.control_to_actuator.split()")
                .count(),
            1
        );
        assert!(
            rendered
                .main
                .contains("motor_cmd_producer: SafetyProducer<'static, MotorCmd>,")
        );
        assert!(
            rendered
                .main
                .contains("motor_cmd_consumer: SafetyConsumer<'static, MotorCmd>,")
        );
        assert!(
            rendered
                .prelude
                .contains("safety_channel::{SafetyChannel, SafetyConsumer, SafetyProducer}")
        );
        assert!(rendered.prelude.contains("ActuatorGuardReport"));
        assert!(rendered.prelude.contains("ActuatorPreparationReport"));
    }

    fn swapped_platform_config() -> PlatformConfig {
        PlatformConfig::empty()
            .serial1(SerialService::MspV1Osd)
            .serial2(SerialService::RcSbus)
            .spi1(SpiService::Imu(ImuInstallationId::new(1)))
    }

    #[test]
    fn serial_services_can_swap_endpoints_without_changing_the_rtic_topology() {
        const SWAPPED: crate::rtic::composition::AppComposition =
            crate::rtic::composition::AppComposition {
                platform_config: swapped_platform_config,
                ..APP_COMPOSITION
            };

        let normal =
            render_sources(&source_root(), &resolve::resolve(&APP_COMPOSITION).unwrap()).unwrap();
        let swapped = render_sources(&source_root(), &resolve::resolve(&SWAPPED).unwrap()).unwrap();

        for task in [
            "serial1_rx_idle_irq",
            "serial1_tx_worker",
            "serial2_rx_idle_irq",
            "serial2_tx_worker",
        ] {
            assert!(normal.main.contains(&format!("fn {task}")));
            assert!(swapped.main.contains(&format!("fn {task}")));
        }
        assert!(
            swapped
                .platform_config
                .contains("serial1: Some(SerialService::MspV1Osd)")
        );
        assert!(
            swapped
                .platform_config
                .contains("serial2: Some(SerialService::RcSbus)")
        );
    }

    #[test]
    fn rendering_is_deterministic() {
        let app = resolve::resolve(&APP_COMPOSITION).unwrap();
        assert_eq!(
            render_sources(&source_root(), &app).unwrap(),
            render_sources(&source_root(), &app).unwrap()
        );
    }
}
