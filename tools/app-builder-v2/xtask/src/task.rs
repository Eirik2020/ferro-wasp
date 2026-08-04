//! RTIC task declarations, handwritten-body validation, and task rendering.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use proc_macro2::{Span, TokenTree};
use quote::ToTokens;
use syn::{
    Expr, ExprField, FnArg, Item, ItemFn, Member, Pat, Type, parse::Parse, spanned::Spanned,
    visit::Visit,
};

/// Places a reusable task definition and its handwritten function in one file.
///
/// The definition is always compiled as ordinary Rust. The function is also
/// emitted when the host task checker enables `app_builder_task_check`; normal
/// xtask builds leave it as unexpanded input for the firmware renderer.
#[macro_export]
macro_rules! app_task {
    (
        $(#[$definition_attribute:meta])*
        $visibility:vis const $definition:ident: $definition_type:ty = $value:expr;

        $body:item
    ) => {
        $(#[$definition_attribute])*
        $visibility const $definition: $definition_type = $value;

        #[cfg(app_builder_task_check)]
        $body
    };
}

/// Declares one argument accepted by a spawn-triggered RTIC task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskArgument {
    /// Rust identifier used for the generated function parameter.
    pub name: &'static str,

    /// Rust type written for the generated function parameter.
    pub rust_type: &'static str,
}

impl TaskArgument {
    /// Creates a task argument from its generated name and Rust type.
    pub const fn new(name: &'static str, rust_type: &'static str) -> Self {
        Self { name, rust_type }
    }
}

/// Selects whether a reusable task body is synchronous or asynchronous.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskExecution {
    /// A normal synchronous Rust function, used by interrupt-bound tasks.
    Synchronous,

    /// An asynchronous Rust function, used by spawn-triggered RTIC tasks.
    Asynchronous,
}

/// Portable operation set required from a logical task resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskResourceCapability {
    /// An embedded-hal digital output supporting state changes and queries.
    DigitalOutput,

    /// A digital input configured to clear and receive an external interrupt.
    InterruptInput,

    /// A Boolean software value.
    Bool,

    /// A DMA-backed serial receiver that exposes IRQ service and bounded chunk reads.
    UartRxDma,
}

/// Declares one logical resource and the capability required by a task body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskResourceDefinition {
    id: &'static str,
    capability: TaskResourceCapability,
}

impl TaskResourceDefinition {
    /// Returns the logical field used after `cx.local` or `cx.shared`.
    pub const fn id(self) -> &'static str {
        self.id
    }

    /// Returns the portable operation set required by the body.
    pub const fn capability(self) -> TaskResourceCapability {
        self.capability
    }
}

/// Declares a logical digital-output field in a reusable task body.
pub const fn digital_output(id: &'static str) -> TaskResourceDefinition {
    TaskResourceDefinition {
        id,
        capability: TaskResourceCapability::DigitalOutput,
    }
}

/// Declares a logical external-interrupt input field in a reusable task body.
pub const fn interrupt_input(id: &'static str) -> TaskResourceDefinition {
    TaskResourceDefinition {
        id,
        capability: TaskResourceCapability::InterruptInput,
    }
}

/// Declares a logical Boolean software-resource field in a reusable task body.
pub const fn boolean(id: &'static str) -> TaskResourceDefinition {
    TaskResourceDefinition {
        id,
        capability: TaskResourceCapability::Bool,
    }
}

/// Declares a logical DMA-backed UART receiver field in a reusable task body.
pub const fn uart_rx_dma(id: &'static str) -> TaskResourceDefinition {
    TaskResourceDefinition {
        id,
        capability: TaskResourceCapability::UartRxDma,
    }
}

/// Selects which interrupt exposed by a hardware resource enters a task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareInterrupt {
    /// Primary interrupt, currently used for GPIO EXTI resources.
    Primary,

    /// DMA receive-stream interrupt.
    DmaRx,

    /// UART peripheral interrupt used to observe the IDLE condition.
    Peripheral,
}

/// Reusable interface paired with one handwritten task function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskDefinition {
    /// File stem, function name, and generated host-context module name.
    pub id: &'static str,

    /// Whether the handwritten function is synchronous or asynchronous.
    pub execution: TaskExecution,

    /// Arguments accepted after the RTIC context parameter.
    pub args: &'static [TaskArgument],

    /// Logical resources owned exclusively by a concrete task instance.
    pub local_resources: &'static [TaskResourceDefinition],

    /// Logical resources accessed through RTIC shared-resource locking.
    pub shared_resources: &'static [TaskResourceDefinition],
}

impl TaskDefinition {
    /// Creates an asynchronous definition without arguments or resources.
    pub const fn asynchronous(id: &'static str) -> Self {
        Self {
            id,
            execution: TaskExecution::Asynchronous,
            args: &[],
            local_resources: &[],
            shared_resources: &[],
        }
    }

    /// Creates a synchronous definition without arguments or resources.
    pub const fn synchronous(id: &'static str) -> Self {
        Self {
            execution: TaskExecution::Synchronous,
            ..Self::asynchronous(id)
        }
    }

    /// Sets the arguments accepted after the generated context.
    pub const fn with_args(mut self, args: &'static [TaskArgument]) -> Self {
        self.args = args;
        self
    }

    /// Sets the logical resources owned locally by each concrete instance.
    pub const fn with_local(mut self, resources: &'static [TaskResourceDefinition]) -> Self {
        self.local_resources = resources;
        self
    }

    /// Sets the logical resources accessed through RTIC shared locking.
    pub const fn with_shared(mut self, resources: &'static [TaskResourceDefinition]) -> Self {
        self.shared_resources = resources;
        self
    }

    /// Creates a spawn-triggered concrete task instance from this definition.
    pub const fn spawned_as(self, id: &'static str) -> TaskDeclaration {
        TaskDeclaration {
            id,
            definition: self,
            priority: 0,
            trigger: TaskTrigger::Spawned,
            local_resources: &[],
            shared_resources: &[],
        }
    }

    /// Creates an interrupt-bound concrete task instance from this definition.
    pub const fn interrupt_as(
        self,
        id: &'static str,
        task_resource: &'static str,
    ) -> TaskDeclaration {
        TaskDeclaration {
            trigger: TaskTrigger::Interrupt {
                resource: task_resource,
                interrupt: HardwareInterrupt::Primary,
            },
            ..self.spawned_as(id)
        }
    }

    /// Creates a task bound to the DMA RX interrupt of a shared UART resource.
    pub const fn dma_rx_interrupt_as(
        self,
        id: &'static str,
        task_resource: &'static str,
    ) -> TaskDeclaration {
        TaskDeclaration {
            trigger: TaskTrigger::Interrupt {
                resource: task_resource,
                interrupt: HardwareInterrupt::DmaRx,
            },
            ..self.spawned_as(id)
        }
    }

    /// Creates a task bound to the peripheral interrupt of a shared UART resource.
    pub const fn peripheral_interrupt_as(
        self,
        id: &'static str,
        task_resource: &'static str,
    ) -> TaskDeclaration {
        TaskDeclaration {
            trigger: TaskTrigger::Interrupt {
                resource: task_resource,
                interrupt: HardwareInterrupt::Peripheral,
            },
            ..self.spawned_as(id)
        }
    }
}

/// Concrete declaration namespace targeted by a task resource binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceTarget {
    /// Resource declared by the selected board.
    Hardware(&'static str),

    /// Software resource declared by the application composition.
    Software(&'static str),
}

impl ResourceTarget {
    /// Returns the concrete board or application resource identifier.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Hardware(id) | Self::Software(id) => id,
        }
    }
}

/// Maps one logical resource field in a reusable body to a concrete resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceBinding {
    task_resource: &'static str,
    target: ResourceTarget,
}

impl ResourceBinding {
    /// Returns the logical field used after `cx.local` or `cx.shared` in the body.
    pub const fn task_resource(self) -> &'static str {
        self.task_resource
    }

    /// Returns the concrete resource targeted by this binding.
    pub const fn target(self) -> ResourceTarget {
        self.target
    }
}

/// Logical task-body resource waiting to be bound to hardware or software.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnboundResource {
    task_resource: &'static str,
}

impl UnboundResource {
    /// Binds the logical body field to a board hardware resource ID.
    pub const fn to_hw(self, hardware_id: &'static str) -> ResourceBinding {
        ResourceBinding {
            task_resource: self.task_resource,
            target: ResourceTarget::Hardware(hardware_id),
        }
    }

    /// Binds the logical body field to an application software resource ID.
    pub const fn to_sw(self, software_id: &'static str) -> ResourceBinding {
        ResourceBinding {
            task_resource: self.task_resource,
            target: ResourceTarget::Software(software_id),
        }
    }
}

/// Starts a resource binding for a logical field used by a reusable task body.
pub const fn resource(task_resource: &'static str) -> UnboundResource {
    UnboundResource { task_resource }
}

/// Selects how an RTIC task is entered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskTrigger {
    /// The task is started through RTIC's generated `spawn` API.
    Spawned,

    /// The task is bound to the interrupt derived from a hardware resource.
    Interrupt {
        /// Interrupt-enabled local hardware resource that triggers the task.
        resource: &'static str,

        /// Interrupt role selected from the bound hardware resource.
        interrupt: HardwareInterrupt,
    },
}

/// Scheduling and resource metadata attached to a handwritten task body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskDeclaration {
    /// Concrete RTIC task identifier.
    pub id: &'static str,

    /// Reusable interface and handwritten-body identity.
    pub definition: TaskDefinition,

    /// RTIC scheduling priority, which must be greater than zero.
    pub priority: u8,

    /// Mechanism that enters the task.
    pub trigger: TaskTrigger,

    /// Resources owned exclusively by this task.
    pub local_resources: &'static [ResourceBinding],

    /// Resources accessed through RTIC shared-resource locking.
    pub shared_resources: &'static [ResourceBinding],
}

impl TaskDeclaration {
    /// Sets the RTIC scheduling priority.
    pub const fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Sets bindings for resources owned locally by the task.
    pub const fn with_local(mut self, resources: &'static [ResourceBinding]) -> Self {
        self.local_resources = resources;
        self
    }

    /// Sets bindings for resources accessed through RTIC shared locking.
    pub const fn with_shared(mut self, resources: &'static [ResourceBinding]) -> Self {
        self.shared_resources = resources;
        self
    }
}

/// Combines a task declaration and handwritten function into an RTIC task.
///
/// Interrupt tasks must receive a backend-resolved binding. Spawn-triggered
/// tasks must not receive one. Logical `cx.local` and `cx.shared` fields are
/// rewritten at their parsed identifier spans, leaving unrelated identifiers,
/// comments, and string literals unchanged.
pub fn render(
    declaration: &TaskDeclaration,
    body_source: &str,
    interrupt_binding: Option<&str>,
) -> Result<String> {
    validate_declaration(declaration)?;
    let body: ItemFn = syn::parse_str(body_source).context("parse handwritten task body")?;
    validate_body(&declaration.definition, declaration.id, &body)?;
    let body_source = rewrite_body(declaration, body_source, &body)?;

    let mut fields = vec![format!("priority = {}", declaration.priority)];
    match (declaration.trigger, interrupt_binding) {
        (TaskTrigger::Spawned, None) => {}
        (TaskTrigger::Interrupt { .. }, Some(binding)) => {
            validate_identifier(binding, "resolved interrupt binding")?;
            fields.insert(0, format!("binds = {binding}"));
        }
        (TaskTrigger::Spawned, Some(_)) => {
            bail!(
                "spawned task `{}` received an interrupt binding",
                declaration.id
            )
        }
        (TaskTrigger::Interrupt { .. }, None) => {
            bail!(
                "interrupt task `{}` has no resolved binding",
                declaration.id
            )
        }
    }
    if !declaration.local_resources.is_empty() {
        fields.push(format!(
            "local = [{}]",
            declaration
                .local_resources
                .iter()
                .map(|binding| binding.target().id())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !declaration.shared_resources.is_empty() {
        fields.push(format!(
            "shared = [{}]",
            declaration
                .shared_resources
                .iter()
                .map(|binding| binding.target().id())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(format!(
        "#[task({})]\n{}",
        fields.join(", "),
        body_source.trim()
    ))
}

/// Validates a task declaration independently of its handwritten body.
pub(crate) fn validate_declaration(declaration: &TaskDeclaration) -> Result<()> {
    validate_identifier(declaration.id, "task ID")?;
    validate_definition(&declaration.definition)?;
    if declaration.priority == 0 {
        bail!(
            "task `{}` priority must be greater than zero",
            declaration.id
        );
    }
    if let TaskTrigger::Interrupt { resource, .. } = declaration.trigger {
        validate_identifier(resource, "interrupt resource")?;
        if !declaration.definition.args.is_empty() {
            bail!(
                "interrupt task `{}` cannot declare spawn arguments",
                declaration.id
            );
        }
    }
    validate_resource_list(declaration.id, "local", declaration.local_resources)?;
    validate_resource_list(declaration.id, "shared", declaration.shared_resources)?;
    validate_bindings_match_definition(
        declaration.id,
        "local",
        declaration.definition.local_resources,
        declaration.local_resources,
    )?;
    validate_bindings_match_definition(
        declaration.id,
        "shared",
        declaration.definition.shared_resources,
        declaration.shared_resources,
    )?;

    match (declaration.trigger, declaration.definition.execution) {
        (TaskTrigger::Spawned, TaskExecution::Synchronous) => bail!(
            "spawned task `{}` requires an asynchronous task definition",
            declaration.id
        ),
        (TaskTrigger::Interrupt { .. }, TaskExecution::Asynchronous) => bail!(
            "interrupt task `{}` requires a synchronous task definition",
            declaration.id
        ),
        _ => {}
    }

    for resource in declaration.local_resources {
        if declaration
            .shared_resources
            .iter()
            .any(|shared| shared.task_resource() == resource.task_resource())
        {
            bail!(
                "task `{}` body resource `{}` cannot be both local and shared",
                declaration.id,
                resource.task_resource()
            );
        }
        if declaration
            .shared_resources
            .iter()
            .any(|shared| shared.target() == resource.target())
        {
            bail!(
                "task `{}` concrete resource `{}` cannot be both local and shared",
                declaration.id,
                resource.target().id()
            );
        }
    }
    Ok(())
}

fn validate_definition(definition: &TaskDefinition) -> Result<()> {
    validate_identifier(definition.id, "task definition ID")?;
    for argument in definition.args {
        validate_identifier(argument.name, "task argument")?;
        syn::parse_str::<Type>(argument.rust_type).with_context(|| {
            format!(
                "task definition `{}` argument `{}` has an invalid Rust type",
                definition.id, argument.name
            )
        })?;
    }
    validate_definition_resources(definition.id, "local", definition.local_resources)?;
    validate_definition_resources(definition.id, "shared", definition.shared_resources)?;
    for resource in definition.local_resources {
        if definition
            .shared_resources
            .iter()
            .any(|shared| shared.id() == resource.id())
        {
            bail!(
                "task definition `{}` resource `{}` cannot be both local and shared",
                definition.id,
                resource.id()
            );
        }
    }
    Ok(())
}

fn validate_body(definition: &TaskDefinition, task_id: &str, body: &ItemFn) -> Result<()> {
    if !body.attrs.is_empty() {
        bail!(
            "handwritten task `{}` must not contain attributes; xtask generates them",
            task_id
        );
    }
    if body.sig.ident != definition.id {
        bail!(
            "task body ID `{}` does not match handwritten function `{}`",
            definition.id,
            body.sig.ident
        );
    }
    match (definition.execution, body.sig.asyncness.is_some()) {
        (TaskExecution::Asynchronous, false) => {
            bail!(
                "handwritten task definition `{}` must be async",
                definition.id
            )
        }
        (TaskExecution::Synchronous, true) => {
            bail!(
                "handwritten task definition `{}` must not be async",
                definition.id
            )
        }
        _ => {}
    }
    if !body.sig.generics.params.is_empty() || body.sig.generics.where_clause.is_some() {
        bail!("handwritten task `{task_id}` must not be generic");
    }

    let mut inputs = body.sig.inputs.iter();
    let context = inputs
        .next()
        .ok_or_else(|| anyhow::anyhow!("task `{task_id}` is missing its RTIC context"))?;
    validate_context(task_id, definition.id, context)?;

    let actual_args = inputs.collect::<Vec<_>>();
    if actual_args.len() != definition.args.len() {
        bail!(
            "task `{}` declares {} argument(s) but its body has {}",
            task_id,
            definition.args.len(),
            actual_args.len()
        );
    }
    for (expected, actual) in definition.args.iter().zip(actual_args) {
        validate_argument(task_id, expected, actual)?;
    }
    Ok(())
}

fn validate_context<'a>(
    task_id: &str,
    body_id: &str,
    argument: &'a FnArg,
) -> Result<&'a syn::Ident> {
    let FnArg::Typed(argument) = argument else {
        bail!("task `{task_id}` context cannot be a receiver");
    };
    let Pat::Ident(pattern) = argument.pat.as_ref() else {
        bail!("task `{task_id}` context must be named `cx`");
    };
    if pattern.ident != "cx" {
        bail!("task `{task_id}` context must be named `cx`");
    }

    let expected = format!("{body_id}::Context").replace(' ', "");
    let actual = argument.ty.to_token_stream().to_string().replace(' ', "");
    if actual != expected {
        bail!("task `{task_id}` context type must be `{body_id}::Context`, got `{actual}`");
    }
    let Type::Path(context_type) = argument.ty.as_ref() else {
        bail!("task `{task_id}` context type must be `{body_id}::Context`");
    };
    context_type
        .path
        .segments
        .first()
        .map(|segment| &segment.ident)
        .ok_or_else(|| anyhow::anyhow!("task `{task_id}` context type has no task identifier"))
}

fn validate_argument(task_id: &str, expected: &TaskArgument, actual: &FnArg) -> Result<()> {
    let FnArg::Typed(actual) = actual else {
        bail!(
            "task `{task_id}` argument `{}` cannot be a receiver",
            expected.name
        );
    };
    let Pat::Ident(pattern) = actual.pat.as_ref() else {
        bail!(
            "task `{task_id}` argument `{}` must be an identifier",
            expected.name
        );
    };
    if pattern.ident != expected.name {
        bail!(
            "task `{task_id}` expected argument `{}`, got `{}`",
            expected.name,
            pattern.ident
        );
    }

    let expected_type = syn::parse_str::<Type>(expected.rust_type)?
        .to_token_stream()
        .to_string();
    let actual_type = actual.ty.to_token_stream().to_string();
    if actual_type != expected_type {
        bail!(
            "task `{task_id}` argument `{}` expected type `{}`, got `{}`",
            expected.name,
            expected.rust_type,
            actual_type
        );
    }
    Ok(())
}

fn validate_resource_list(task_id: &str, kind: &str, resources: &[ResourceBinding]) -> Result<()> {
    for (index, resource) in resources.iter().enumerate() {
        validate_identifier(
            resource.task_resource(),
            &format!("task `{task_id}` {kind} body resource"),
        )?;
        validate_identifier(
            resource.target().id(),
            &format!("task `{task_id}` {kind} concrete resource"),
        )?;
        if resources[..index]
            .iter()
            .any(|existing| existing.task_resource() == resource.task_resource())
        {
            bail!(
                "task `{task_id}` repeats {kind} body resource `{}`",
                resource.task_resource()
            );
        }
        if resources[..index]
            .iter()
            .any(|existing| existing.target() == resource.target())
        {
            bail!(
                "task `{task_id}` binds {kind} concrete resource `{}` more than once",
                resource.target().id()
            );
        }
    }
    Ok(())
}

fn validate_definition_resources(
    definition_id: &str,
    kind: &str,
    resources: &[TaskResourceDefinition],
) -> Result<()> {
    for (index, resource) in resources.iter().enumerate() {
        validate_identifier(
            resource.id(),
            &format!("task definition `{definition_id}` {kind} resource"),
        )?;
        if resources[..index]
            .iter()
            .any(|existing| existing.id() == resource.id())
        {
            bail!(
                "task definition `{definition_id}` repeats {kind} resource `{}`",
                resource.id()
            );
        }
    }
    Ok(())
}

fn validate_bindings_match_definition(
    task_id: &str,
    kind: &str,
    resources: &[TaskResourceDefinition],
    bindings: &[ResourceBinding],
) -> Result<()> {
    for resource in resources {
        if !bindings
            .iter()
            .any(|binding| binding.task_resource() == resource.id())
        {
            bail!(
                "task `{task_id}` is missing {kind} binding for body resource `{}`",
                resource.id()
            );
        }
    }
    for binding in bindings {
        if !resources
            .iter()
            .any(|resource| resource.id() == binding.task_resource())
        {
            bail!(
                "task `{task_id}` has extra {kind} binding for body resource `{}`",
                binding.task_resource()
            );
        }
    }
    Ok(())
}

#[derive(Debug)]
struct SourceReplacement {
    span: Span,
    expected: String,
    replacement: String,
}

struct ResourceUseVisitor<'a> {
    task_id: &'a str,
    local_resources: BTreeMap<&'a str, &'a str>,
    shared_resources: BTreeMap<&'a str, &'a str>,
    replacements: Vec<SourceReplacement>,
    errors: Vec<String>,
}

impl ResourceUseVisitor<'_> {
    fn record_resource_use(&mut self, kind: &str, field: &syn::Ident) {
        let field_name = field.to_string();
        let resources = match kind {
            "local" => &self.local_resources,
            "shared" => &self.shared_resources,
            _ => return,
        };
        let Some(concrete_resource) = resources.get(field_name.as_str()) else {
            self.errors.push(format!(
                "task `{}` body uses cx.{kind}.{field_name} without a {kind} resource binding",
                self.task_id
            ));
            return;
        };
        self.replacements.push(SourceReplacement {
            span: field.span(),
            expected: field_name,
            replacement: (*concrete_resource).to_owned(),
        });
    }

    fn visit_macro_tokens(&mut self, tokens: proc_macro2::TokenStream) {
        let tokens = tokens.into_iter().collect::<Vec<_>>();
        for token in &tokens {
            if let TokenTree::Group(group) = token {
                self.visit_macro_tokens(group.stream());
            }
        }
        for tokens in tokens.windows(5) {
            let [
                TokenTree::Ident(context),
                TokenTree::Punct(first_dot),
                TokenTree::Ident(kind),
                TokenTree::Punct(second_dot),
                TokenTree::Ident(resource),
            ] = tokens
            else {
                continue;
            };
            if context == "cx"
                && first_dot.as_char() == '.'
                && second_dot.as_char() == '.'
                && (kind == "local" || kind == "shared")
            {
                self.record_resource_use(
                    if kind == "local" { "local" } else { "shared" },
                    resource,
                );
            }
        }
    }
}

impl<'ast> Visit<'ast> for ResourceUseVisitor<'_> {
    fn visit_expr_field(&mut self, field: &'ast ExprField) {
        if let Some((kind, resource)) = context_resource_field(field) {
            self.record_resource_use(kind, resource);
        }
        syn::visit::visit_expr_field(self, field);
    }

    fn visit_macro(&mut self, task_macro: &'ast syn::Macro) {
        self.visit_macro_tokens(task_macro.tokens.clone());
        syn::visit::visit_macro(self, task_macro);
    }
}

fn context_resource_field(field: &ExprField) -> Option<(&str, &syn::Ident)> {
    let Member::Named(resource) = &field.member else {
        return None;
    };
    let Expr::Field(context_group) = field.base.as_ref() else {
        return None;
    };
    let Member::Named(kind) = &context_group.member else {
        return None;
    };
    if kind != "local" && kind != "shared" {
        return None;
    }
    let Expr::Path(context) = context_group.base.as_ref() else {
        return None;
    };
    context
        .path
        .is_ident("cx")
        .then_some((if kind == "local" { "local" } else { "shared" }, resource))
}

fn rewrite_body(declaration: &TaskDeclaration, body_source: &str, body: &ItemFn) -> Result<String> {
    let local_resources = declaration
        .local_resources
        .iter()
        .map(|binding| (binding.task_resource(), binding.target().id()))
        .collect();
    let shared_resources = declaration
        .shared_resources
        .iter()
        .map(|binding| (binding.task_resource(), binding.target().id()))
        .collect();
    let mut visitor = ResourceUseVisitor {
        task_id: declaration.id,
        local_resources,
        shared_resources,
        replacements: Vec::new(),
        errors: Vec::new(),
    };
    visitor.visit_item_fn(body);
    if !visitor.errors.is_empty() {
        bail!(visitor.errors.join("; "));
    }

    if declaration.definition.id != declaration.id {
        visitor.replacements.push(SourceReplacement {
            span: body.sig.ident.span(),
            expected: declaration.definition.id.to_owned(),
            replacement: declaration.id.to_owned(),
        });
        let context = body.sig.inputs.first().ok_or_else(|| {
            anyhow::anyhow!("task `{}` is missing its RTIC context", declaration.id)
        })?;
        let context_task = validate_context(declaration.id, declaration.definition.id, context)?;
        visitor.replacements.push(SourceReplacement {
            span: context_task.span(),
            expected: declaration.definition.id.to_owned(),
            replacement: declaration.id.to_owned(),
        });
    }

    apply_source_replacements(body_source, visitor.replacements)
}

fn apply_source_replacements(source: &str, replacements: Vec<SourceReplacement>) -> Result<String> {
    let line_starts = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        )
        .collect::<Vec<_>>();
    let mut replacements = replacements
        .into_iter()
        .map(|replacement| {
            let start = source_offset(source, &line_starts, replacement.span.start())?;
            let end = source_offset(source, &line_starts, replacement.span.end())?;
            if source.get(start..end) != Some(replacement.expected.as_str()) {
                bail!(
                    "task source span expected `{}`, found `{}`",
                    replacement.expected,
                    source.get(start..end).unwrap_or("<invalid UTF-8 boundary>")
                );
            }
            Ok((start, end, replacement.replacement))
        })
        .collect::<Result<Vec<_>>>()?;
    replacements.sort_by(|left, right| right.0.cmp(&left.0));

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

struct AppTaskInput {
    definition_name: syn::Ident,
    body: ItemFn,
}

impl Parse for AppTaskInput {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let _attributes = input.call(syn::Attribute::parse_outer)?;
        let _visibility: syn::Visibility = input.parse()?;
        input.parse::<syn::Token![const]>()?;
        let definition_name = input.parse()?;
        input.parse::<syn::Token![:]>()?;
        let _definition_type: Type = input.parse()?;
        input.parse::<syn::Token![=]>()?;
        let _definition_value: Expr = input.parse()?;
        input.parse::<syn::Token![;]>()?;
        let body = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("app_task! accepts exactly one definition and one function"));
        }
        Ok(Self {
            definition_name,
            body,
        })
    }
}

/// Reads and validates the handwritten function from a unified task module.
pub(crate) fn read_body_source(
    repository_root: &Path,
    definition: &TaskDefinition,
) -> Result<String> {
    validate_safe_filename(definition.id)?;
    let path = repository_root
        .join("tasks")
        .join(format!("{}.rs", definition.id));
    let source = fs::read_to_string(&path)
        .with_context(|| format!("read unified task module {}", path.display()))?;
    let (_, body_source, body) = parse_task_module_source(&source)
        .with_context(|| format!("parse unified task module {}", path.display()))?;
    validate_body(definition, definition.id, &body)?;
    Ok(body_source)
}

fn parse_task_module_source(source: &str) -> Result<(String, String, ItemFn)> {
    let file = syn::parse_file(source).context("parse unified task module as Rust")?;
    let mut task_macros = file.items.iter().filter_map(|item| {
        let Item::Macro(item_macro) = item else {
            return None;
        };
        item_macro
            .mac
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "app_task")
            .then_some(item_macro)
    });
    let task_macro = task_macros
        .next()
        .ok_or_else(|| anyhow::anyhow!("task module must contain one app_task! invocation"))?;
    if task_macros.next().is_some() {
        bail!("task module must not contain more than one app_task! invocation");
    }
    let parsed: AppTaskInput = syn::parse2(task_macro.mac.tokens.clone())
        .context("parse app_task! definition and function")?;
    let body_span = parsed.body.span();
    let line_starts = source_line_starts(source);
    let start = source_offset(source, &line_starts, body_span.start())?;
    let end = source_offset(source, &line_starts, body_span.end())?;
    let body_source = source
        .get(start..end)
        .ok_or_else(|| anyhow::anyhow!("task body span is outside its source file"))?
        .to_owned();
    let body_source = dedent_body_source(&body_source, body_span.start().column);
    Ok((parsed.definition_name.to_string(), body_source, parsed.body))
}

fn dedent_body_source(source: &str, indentation: usize) -> String {
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

fn validate_safe_filename(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        bail!("task definition ID `{id}` is not a safe filename");
    }
    Ok(())
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

fn source_offset(
    source: &str,
    line_starts: &[usize],
    location: proc_macro2::LineColumn,
) -> Result<usize> {
    let line_start = line_starts
        .get(location.line.saturating_sub(1))
        .copied()
        .ok_or_else(|| {
            anyhow::anyhow!("task source span references missing line {}", location.line)
        })?;
    let offset = line_start + location.column;
    if offset > source.len() || !source.is_char_boundary(offset) {
        bail!(
            "task source span references invalid column {} on line {}",
            location.column,
            location.line
        );
    }
    Ok(offset)
}

fn validate_identifier(value: &str, label: &str) -> Result<()> {
    syn::parse_str::<syn::Ident>(value)
        .map(|_| ())
        .with_context(|| format!("{label} `{value}` is not a Rust identifier"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLINK_DEFINITION: TaskDefinition =
        TaskDefinition::asynchronous("blink_led").with_local(&[digital_output("led")]);
    const BLINK: TaskDeclaration = BLINK_DEFINITION
        .spawned_as("blink_led")
        .priority(1)
        .with_local(&[resource("led").to_hw("led3")]);

    #[test]
    fn declaration_is_attached_to_handwritten_function() {
        let rendered = render(
            &BLINK,
            "async fn blink_led(mut cx: blink_led::Context) { loop {} }",
            None,
        )
        .unwrap();
        assert!(rendered.starts_with("#[task(priority = 1, local = [led3])]"));
        assert!(syn::parse_file(&rendered).is_ok());
    }

    #[test]
    fn mismatched_function_name_is_rejected() {
        let error = render(
            &BLINK,
            "async fn other(mut cx: other::Context) { loop {} }",
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn interrupt_binding_is_attached_to_synchronous_body() {
        const BUTTON_DEFINITION: TaskDefinition = TaskDefinition::synchronous("button_exti")
            .with_local(&[interrupt_input("button")])
            .with_shared(&[boolean("enabled")]);
        const BUTTON: TaskDeclaration = BUTTON_DEFINITION
            .interrupt_as("button_exti", "button")
            .priority(2)
            .with_local(&[resource("button").to_hw("user_button")])
            .with_shared(&[resource("enabled").to_sw("blink_enabled")]);
        let rendered = render(
            &BUTTON,
            "fn button_exti(mut cx: button_exti::Context) { let _ = &mut cx; }",
            Some("EXTI15_10"),
        )
        .unwrap();
        assert!(rendered.starts_with(
            "#[task(binds = EXTI15_10, priority = 2, local = [user_button], shared = [blink_enabled])]"
        ));
    }

    #[test]
    fn reusable_body_names_and_resources_are_rewritten_exactly() {
        const REUSABLE_DEFINITION: TaskDefinition = TaskDefinition::asynchronous("blink")
            .with_local(&[digital_output("led")])
            .with_shared(&[boolean("enabled")]);
        const REUSABLE_BLINK: TaskDeclaration = REUSABLE_DEFINITION
            .spawned_as("blink_led")
            .priority(1)
            .with_local(&[resource("led").to_hw("led3")])
            .with_shared(&[resource("enabled").to_sw("blink_enabled")]);
        let rendered = render(
            &REUSABLE_BLINK,
            r#"async fn blink(mut cx: blink::Context) {
    let unchanged = "cx.local.led";
    let _ = cx.local.led;
    let _ = cx.shared.enabled;
    consume!(cx.local.led);
    let _ = unchanged;
}"#,
            None,
        )
        .unwrap();

        assert!(rendered.contains("async fn blink_led(mut cx: blink_led::Context)"));
        assert!(rendered.contains("let _ = cx.local.led3;"));
        assert!(rendered.contains("let _ = cx.shared.blink_enabled;"));
        assert!(rendered.contains("consume!(cx.local.led3);"));
        assert!(rendered.contains("\"cx.local.led\""));
    }

    #[test]
    fn body_resource_use_requires_a_matching_binding() {
        const EMPTY: TaskDeclaration = TaskDefinition::asynchronous("blink")
            .spawned_as("blink")
            .priority(1);
        let error = render(
            &EMPTY,
            "async fn blink(mut cx: blink::Context) { let _ = cx.local.led; }",
            None,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("without a local resource binding")
        );
    }

    #[test]
    fn definition_requires_exact_concrete_bindings() {
        const DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("task").with_local(&[digital_output("output")]);
        const MISSING: TaskDeclaration = DEFINITION.spawned_as("missing").priority(1);
        const EXTRA_DEFINITION: TaskDefinition = TaskDefinition::asynchronous("empty");
        const EXTRA: TaskDeclaration = EXTRA_DEFINITION
            .spawned_as("extra")
            .priority(1)
            .with_local(&[resource("output").to_hw("led")]);

        assert!(
            validate_declaration(&MISSING)
                .unwrap_err()
                .to_string()
                .contains("missing local binding")
        );
        assert!(
            validate_declaration(&EXTRA)
                .unwrap_err()
                .to_string()
                .contains("extra local binding")
        );
    }

    #[test]
    fn trigger_kind_must_match_definition_execution() {
        const SYNC: TaskDeclaration = TaskDefinition::synchronous("sync")
            .spawned_as("sync_task")
            .priority(1);
        const ASYNC: TaskDeclaration = TaskDefinition::asynchronous("async_task")
            .interrupt_as("async_irq", "input")
            .priority(1);

        assert!(
            validate_declaration(&SYNC)
                .unwrap_err()
                .to_string()
                .contains("requires an asynchronous")
        );
        assert!(
            validate_declaration(&ASYNC)
                .unwrap_err()
                .to_string()
                .contains("requires a synchronous")
        );
    }

    #[test]
    fn unified_task_parser_preserves_and_dedents_the_function() {
        let source = r#"use crate::task::TaskDefinition;

crate::app_task! {
    pub const BLINK: TaskDefinition = TaskDefinition::asynchronous("blink");

    async fn blink(cx: blink::Context) {
        let _ = cx;
    }
}
"#;
        let (name, body, parsed) = parse_task_module_source(source).unwrap();

        assert_eq!(name, "BLINK");
        assert_eq!(
            body,
            "async fn blink(cx: blink::Context) {\n    let _ = cx;\n}"
        );
        assert_eq!(parsed.sig.ident, "blink");
    }

    #[test]
    fn unified_task_parser_requires_exactly_one_macro() {
        assert!(
            parse_task_module_source("fn task() {}")
                .err()
                .expect("missing task macro must fail")
                .to_string()
                .contains("must contain one")
        );

        let duplicate = r#"
crate::app_task! {
    pub const FIRST: TaskDefinition = TaskDefinition::asynchronous("first");
    async fn first(cx: first::Context) { let _ = cx; }
}
crate::app_task! {
    pub const SECOND: TaskDefinition = TaskDefinition::asynchronous("second");
    async fn second(cx: second::Context) { let _ = cx; }
}
"#;
        assert!(
            parse_task_module_source(duplicate)
                .err()
                .expect("duplicate task macros must fail")
                .to_string()
                .contains("more than one")
        );
    }

    #[test]
    fn concrete_resource_namespace_is_part_of_the_binding() {
        let hardware = resource("port").to_hw("uart1");
        let software = resource("enabled").to_sw("uart1_enabled");

        assert_eq!(hardware.task_resource(), "port");
        assert_eq!(hardware.target(), ResourceTarget::Hardware("uart1"));
        assert_eq!(software.task_resource(), "enabled");
        assert_eq!(software.target(), ResourceTarget::Software("uart1_enabled"));
    }
}
