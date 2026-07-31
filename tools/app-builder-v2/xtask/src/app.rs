use std::collections::BTreeSet;

use anyhow::{Result, bail};

use crate::task::{TaskDeclaration, TaskTrigger, validate_declaration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftwareResourceDeclaration {
    Bool { id: &'static str, initial: bool },
}

impl SoftwareResourceDeclaration {
    pub const fn bool(id: &'static str, initial: bool) -> Self {
        Self::Bool { id, initial }
    }

    pub const fn id(&self) -> &'static str {
        match self {
            Self::Bool { id, .. } => id,
        }
    }

    pub const fn rust_type(&self) -> &'static str {
        match self {
            Self::Bool { .. } => "bool",
        }
    }

    pub fn initial_value(&self) -> String {
        match self {
            Self::Bool { initial, .. } => initial.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoftwareResourcesDeclaration {
    pub shared: &'static [SoftwareResourceDeclaration],
    pub local: &'static [SoftwareResourceDeclaration],
}

impl SoftwareResourcesDeclaration {
    pub const EMPTY: Self = Self {
        shared: &[],
        local: &[],
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitDeclaration {
    pub spawns: &'static [TaskDeclaration],
}

impl InitDeclaration {
    pub const EMPTY: Self = Self { spawns: &[] };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDeclaration {
    pub init: InitDeclaration,
    pub tasks: &'static [TaskDeclaration],
    pub software_resources: SoftwareResourcesDeclaration,
}

impl AppDeclaration {
    #[allow(dead_code)]
    pub const EMPTY: Self = Self {
        init: InitDeclaration::EMPTY,
        tasks: &[],
        software_resources: SoftwareResourcesDeclaration::EMPTY,
    };
}

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
    for task in app.tasks {
        validate_declaration(task)?;
        if !task_ids.insert(task.id) {
            bail!("application repeats task `{}`", task.id);
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

    const TASK: TaskDeclaration = TaskDeclaration {
        id: "task",
        priority: 1,
        trigger: crate::task::TaskTrigger::Spawned,
        args: &[],
        local_resources: &[],
        shared_resources: &[],
    };
    const OTHER: TaskDeclaration = TaskDeclaration {
        id: "other",
        ..TASK
    };

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
}
