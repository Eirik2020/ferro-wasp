//! Application-composition declarations and structural validation.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use crate::task::{TaskDeclaration, TaskTrigger, validate_declaration};

/// Declares a software-owned value made available as an RTIC resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftwareResourceDeclaration {
    /// Declares a Boolean resource with its generated field name and initial value.
    Bool {
        /// Rust identifier used for the generated resource field.
        id: &'static str,

        /// Value assigned to the resource during RTIC initialization.
        initial: bool,
    },
}

impl SoftwareResourceDeclaration {
    /// Creates a Boolean software-resource declaration.
    pub const fn bool(id: &'static str, initial: bool) -> Self {
        Self::Bool { id, initial }
    }

    /// Returns the generated resource-field identifier.
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Bool { id, .. } => id,
        }
    }

    /// Returns the Rust type emitted for the resource field.
    pub const fn rust_type(&self) -> &'static str {
        match self {
            Self::Bool { .. } => "bool",
        }
    }

    /// Renders the Rust expression used to initialize the resource.
    pub fn initial_value(&self) -> String {
        match self {
            Self::Bool { initial, .. } => initial.to_string(),
        }
    }
}

/// Groups software resources by their RTIC ownership class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoftwareResourcesDeclaration {
    /// Resources accessed through RTIC shared-resource locking.
    pub shared: &'static [SoftwareResourceDeclaration],

    /// Resources owned locally by exactly one task.
    pub local: &'static [SoftwareResourceDeclaration],
}

impl SoftwareResourcesDeclaration {
    /// Empty software-resource declaration for applications without software state.
    pub const EMPTY: Self = Self {
        shared: &[],
        local: &[],
    };
}

/// Declares work started by the generated RTIC `init` function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitDeclaration {
    /// Spawn-triggered tasks that `init` starts once initialization completes.
    pub spawns: &'static [TaskDeclaration],
}

impl InitDeclaration {
    /// Empty init declaration that starts no tasks.
    pub const EMPTY: Self = Self { spawns: &[] };
}

/// Describes the tasks and software state included in a generated application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDeclaration {
    /// Initialization behavior for the application.
    pub init: InitDeclaration,

    /// Complete set of task declarations included in the RTIC application.
    pub tasks: &'static [TaskDeclaration],

    /// Software-owned local and shared resources available to tasks.
    pub software_resources: SoftwareResourcesDeclaration,
}

impl AppDeclaration {
    #[allow(dead_code)]
    /// Empty application declaration used for empty-app generation and tests.
    pub const EMPTY: Self = Self {
        init: InitDeclaration::EMPTY,
        tasks: &[],
        software_resources: SoftwareResourcesDeclaration::EMPTY,
    };
}

/// Validates application identifiers, task uniqueness, and init-spawn rules.
pub fn validate(app: &AppDeclaration) -> Result<()> {
    let mut software_resource_ids = BTreeSet::new();
    for (kind, resources) in [
        ("shared", app.software_resources.shared),
        ("local", app.software_resources.local),
    ] {
        for resource in resources {
            let id = resource.id();
            syn::parse_str::<syn::Ident>(id).map_err(|_| {
                anyhow::anyhow!("{kind} software resource ID `{id}` is not a Rust identifier")
            })?;
            if !software_resource_ids.insert(id) {
                bail!("application repeats software resource `{id}`");
            }
        }
    }

    let mut task_ids = BTreeSet::new();
    let mut definitions = BTreeMap::new();
    for task in app.tasks {
        validate_declaration(task)?;
        if !task_ids.insert(task.id) {
            bail!("application repeats task `{}`", task.id);
        }
        if let Some(existing) = definitions.insert(task.definition.id, task.definition)
            && existing != task.definition
        {
            bail!(
                "application uses conflicting definitions for task body `{}`",
                task.definition.id
            );
        }
    }
    let mut spawned_ids = BTreeSet::new();
    for task in app.init.spawns {
        if !task_ids.contains(task.id) {
            bail!("init spawns `{}` but it is not included in tasks", task.id);
        }
        if !spawned_ids.insert(task.id) {
            bail!("init spawns task `{}` more than once", task.id);
        }
        if !matches!(task.trigger, TaskTrigger::Spawned) {
            bail!("init cannot spawn interrupt task `{}`", task.id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{TaskArgument, TaskDefinition};

    const TASK: TaskDeclaration = TaskDefinition::asynchronous("task")
        .spawned_as("task")
        .priority(1);
    const OTHER: TaskDeclaration = TaskDefinition::asynchronous("other")
        .spawned_as("other")
        .priority(1);

    #[test]
    fn init_may_only_spawn_included_tasks() {
        let app = AppDeclaration {
            init: InitDeclaration { spawns: &[OTHER] },
            tasks: &[TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        assert!(validate(&app).is_err());
    }

    #[test]
    fn empty_application_is_valid() {
        assert!(validate(&AppDeclaration::EMPTY).is_ok());
    }

    #[test]
    fn conflicting_definitions_for_one_body_id_are_rejected() {
        const FIRST_DEFINITION: TaskDefinition = TaskDefinition::asynchronous("reused");
        const SECOND_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("reused").with_args(&[TaskArgument::new("value", "u32")]);
        const FIRST: TaskDeclaration = FIRST_DEFINITION.spawned_as("first").priority(1);
        const SECOND: TaskDeclaration = SECOND_DEFINITION.spawned_as("second").priority(1);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[FIRST, SECOND],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        assert!(
            validate(&app)
                .unwrap_err()
                .to_string()
                .contains("conflicting definitions")
        );
    }
}
