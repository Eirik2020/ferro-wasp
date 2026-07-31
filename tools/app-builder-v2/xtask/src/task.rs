use anyhow::{Context, Result, bail};
use quote::ToTokens;
use syn::{FnArg, ItemFn, Pat, Type};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskArgument {
    pub name: &'static str,
    pub rust_type: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskTrigger {
    Spawned,
    Interrupt { binds: &'static str },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskDeclaration {
    pub id: &'static str,
    pub priority: u8,
    pub trigger: TaskTrigger,
    pub args: &'static [TaskArgument],
    pub local_resources: &'static [&'static str],
    pub shared_resources: &'static [&'static str],
}

pub fn render(declaration: &TaskDeclaration, body_source: &str) -> Result<String> {
    validate_declaration(declaration)?;
    let body: ItemFn = syn::parse_str(body_source).context("parse handwritten task body")?;
    validate_body(declaration, &body)?;

    let mut fields = vec![format!("priority = {}", declaration.priority)];
    if let TaskTrigger::Interrupt { binds } = declaration.trigger {
        fields.insert(0, format!("binds = {binds}"));
    }
    if !declaration.local_resources.is_empty() {
        fields.push(format!(
            "local = [{}]",
            declaration.local_resources.join(", ")
        ));
    }
    if !declaration.shared_resources.is_empty() {
        fields.push(format!(
            "shared = [{}]",
            declaration.shared_resources.join(", ")
        ));
    }

    Ok(format!(
        "#[task({})]\n{}",
        fields.join(", "),
        body_source.trim()
    ))
}

pub(crate) fn validate_declaration(declaration: &TaskDeclaration) -> Result<()> {
    validate_identifier(declaration.id, "task ID")?;
    if declaration.priority == 0 {
        bail!(
            "task `{}` priority must be greater than zero",
            declaration.id
        );
    }
    if let TaskTrigger::Interrupt { binds } = declaration.trigger {
        validate_identifier(binds, "interrupt binding")?;
        if !declaration.args.is_empty() {
            bail!(
                "interrupt task `{}` cannot declare spawn arguments",
                declaration.id
            );
        }
    }

    for argument in declaration.args {
        validate_identifier(argument.name, "task argument")?;
        syn::parse_str::<Type>(argument.rust_type).with_context(|| {
            format!(
                "task `{}` argument `{}` has an invalid Rust type",
                declaration.id, argument.name
            )
        })?;
    }
    validate_resource_list(declaration.id, "local", declaration.local_resources)?;
    validate_resource_list(declaration.id, "shared", declaration.shared_resources)?;

    for resource in declaration.local_resources {
        if declaration.shared_resources.contains(resource) {
            bail!(
                "task `{}` resource `{resource}` cannot be both local and shared",
                declaration.id
            );
        }
    }
    Ok(())
}

fn validate_body(declaration: &TaskDeclaration, body: &ItemFn) -> Result<()> {
    if !body.attrs.is_empty() {
        bail!(
            "handwritten task `{}` must not contain attributes; xtask generates them",
            declaration.id
        );
    }
    if body.sig.ident != declaration.id {
        bail!(
            "task declaration ID `{}` does not match handwritten function `{}`",
            declaration.id,
            body.sig.ident
        );
    }
    match (declaration.trigger, body.sig.asyncness.is_some()) {
        (TaskTrigger::Spawned, false) => {
            bail!(
                "handwritten spawned task `{}` must be async",
                declaration.id
            )
        }
        (TaskTrigger::Interrupt { .. }, true) => {
            bail!(
                "handwritten interrupt task `{}` must not be async",
                declaration.id
            )
        }
        _ => {}
    }
    if !body.sig.generics.params.is_empty() || body.sig.generics.where_clause.is_some() {
        bail!("handwritten task `{}` must not be generic", declaration.id);
    }

    let mut inputs = body.sig.inputs.iter();
    let context = inputs
        .next()
        .ok_or_else(|| anyhow::anyhow!("task `{}` is missing its RTIC context", declaration.id))?;
    validate_context(declaration.id, context)?;

    let actual_args = inputs.collect::<Vec<_>>();
    if actual_args.len() != declaration.args.len() {
        bail!(
            "task `{}` declares {} argument(s) but its body has {}",
            declaration.id,
            declaration.args.len(),
            actual_args.len()
        );
    }
    for (expected, actual) in declaration.args.iter().zip(actual_args) {
        validate_argument(declaration.id, expected, actual)?;
    }
    Ok(())
}

fn validate_context(task_id: &str, argument: &FnArg) -> Result<()> {
    let FnArg::Typed(argument) = argument else {
        bail!("task `{task_id}` context cannot be a receiver");
    };
    let Pat::Ident(pattern) = argument.pat.as_ref() else {
        bail!("task `{task_id}` context must be named `cx`");
    };
    if pattern.ident != "cx" {
        bail!("task `{task_id}` context must be named `cx`");
    }

    let expected = format!("{task_id}::Context").replace(' ', "");
    let actual = argument.ty.to_token_stream().to_string().replace(' ', "");
    if actual != expected {
        bail!("task `{task_id}` context type must be `{task_id}::Context`, got `{actual}`");
    }
    Ok(())
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

fn validate_resource_list(task_id: &str, kind: &str, resources: &[&str]) -> Result<()> {
    for (index, resource) in resources.iter().enumerate() {
        validate_identifier(resource, &format!("task `{task_id}` {kind} resource"))?;
        if resources[..index].contains(resource) {
            bail!("task `{task_id}` repeats {kind} resource `{resource}`");
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, label: &str) -> Result<()> {
    syn::parse_str::<syn::Ident>(value)
        .map(|_| ())
        .with_context(|| format!("{label} `{value}` is not a Rust identifier"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLINK: TaskDeclaration = TaskDeclaration {
        id: "blink_led",
        priority: 1,
        trigger: TaskTrigger::Spawned,
        args: &[],
        local_resources: &["led2"],
        shared_resources: &[],
    };

    #[test]
    fn declaration_is_attached_to_handwritten_function() {
        let rendered = render(
            &BLINK,
            "async fn blink_led(mut cx: blink_led::Context) { loop {} }",
        )
        .unwrap();
        assert!(rendered.starts_with("#[task(priority = 1, local = [led2])]"));
        assert!(syn::parse_file(&rendered).is_ok());
    }

    #[test]
    fn mismatched_function_name_is_rejected() {
        let error =
            render(&BLINK, "async fn other(mut cx: other::Context) { loop {} }").unwrap_err();
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn interrupt_binding_is_attached_to_synchronous_body() {
        const BUTTON: TaskDeclaration = TaskDeclaration {
            id: "button_exti",
            priority: 2,
            trigger: TaskTrigger::Interrupt { binds: "EXTI15_10" },
            args: &[],
            local_resources: &["user_button"],
            shared_resources: &["blink_enabled"],
        };
        let rendered = render(
            &BUTTON,
            "fn button_exti(mut cx: button_exti::Context) { let _ = &mut cx; }",
        )
        .unwrap();
        assert!(rendered.starts_with(
            "#[task(binds = EXTI15_10, priority = 2, local = [user_button], shared = [blink_enabled])]"
        ));
    }
}
