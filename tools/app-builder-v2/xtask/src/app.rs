use std::collections::BTreeSet;

use anyhow::{Result, bail};

use crate::task::TaskDeclaration;

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
}

impl AppDeclaration {
    #[allow(dead_code)]
    pub const EMPTY: Self = Self {
        init: InitDeclaration::EMPTY,
        tasks: &[],
    };
}

pub fn validate(app: &AppDeclaration) -> Result<()> {
    let mut task_ids = BTreeSet::new();
    for task in app.tasks {
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASK: TaskDeclaration = TaskDeclaration {
        id: "task",
        priority: 1,
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
        };
        assert!(validate(&app).is_err());
    }

    #[test]
    fn empty_application_is_valid() {
        assert!(validate(&AppDeclaration::EMPTY).is_ok());
    }
}
