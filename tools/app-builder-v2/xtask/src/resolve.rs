use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::{
    app::{AppDeclaration, SoftwareResourceDeclaration},
    board::{BoardDeclaration, HardwareDeclaration},
    task::TaskDeclaration,
};

#[derive(Debug)]
pub struct ResolvedTask<'a> {
    pub declaration: &'a TaskDeclaration,
    pub local_resources: Vec<ResolvedTaskResource<'a>>,
    pub shared_resources: Vec<ResolvedTaskResource<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub enum ResolvedTaskResource<'a> {
    Hardware(&'a HardwareDeclaration),
    Software(&'a SoftwareResourceDeclaration),
}

impl ResolvedTaskResource<'_> {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Hardware(hardware) => hardware.id(),
            Self::Software(resource) => resource.id(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ResolvedResourceUsage {
    Local { task_id: &'static str },
    Shared { task_ids: Vec<&'static str> },
}

#[derive(Debug)]
pub struct ResolvedResource<'a> {
    pub hardware: &'a HardwareDeclaration,
    pub usage: ResolvedResourceUsage,
}

#[derive(Debug)]
pub struct ResolvedSoftwareResource<'a> {
    pub declaration: &'a SoftwareResourceDeclaration,
    pub task_ids: Vec<&'static str>,
}

#[derive(Debug)]
pub struct ResolvedApp<'a> {
    pub tasks: Vec<ResolvedTask<'a>>,
    pub resources: Vec<ResolvedResource<'a>>,
    pub software_local_resources: Vec<ResolvedSoftwareResource<'a>>,
    pub software_shared_resources: Vec<ResolvedSoftwareResource<'a>>,
    pub init_spawned_tasks: Vec<&'a TaskDeclaration>,
    pub resource_initialization_order: Vec<&'static str>,
}

enum PendingUsage {
    Local { task_id: &'static str },
    Shared { task_ids: Vec<&'static str> },
}

pub fn resolve<'a>(
    board: &'a BoardDeclaration,
    app: &'a AppDeclaration,
) -> Result<ResolvedApp<'a>> {
    let hardware_by_id = board
        .hardware
        .iter()
        .map(|hardware| (hardware.id(), hardware))
        .collect::<BTreeMap<_, _>>();
    let software_local_by_id = app
        .software_resources
        .local
        .iter()
        .map(|resource| (resource.id(), resource))
        .collect::<BTreeMap<_, _>>();
    let software_shared_by_id = app
        .software_resources
        .shared
        .iter()
        .map(|resource| (resource.id(), resource))
        .collect::<BTreeMap<_, _>>();
    for id in software_local_by_id
        .keys()
        .chain(software_shared_by_id.keys())
    {
        if hardware_by_id.contains_key(id) {
            bail!("resource `{id}` is declared by both the board and application");
        }
    }
    let mut usage_by_resource = BTreeMap::<&str, PendingUsage>::new();
    let mut software_local_usage = BTreeMap::<&str, Vec<&'static str>>::new();
    let mut software_shared_usage = BTreeMap::<&str, Vec<&'static str>>::new();
    let mut tasks = Vec::with_capacity(app.tasks.len());

    for task in app.tasks {
        let local_resources = task
            .local_resources
            .iter()
            .map(|resource_id| {
                if let Some(resource) = software_local_by_id.get(resource_id).copied() {
                    let owners = software_local_usage.entry(resource_id).or_default();
                    if let Some(existing) = owners.first() {
                        bail!(
                            "software resource `{resource_id}` is claimed locally by both `{existing}` and `{}`",
                            task.id
                        );
                    }
                    owners.push(task.id);
                    Ok(ResolvedTaskResource::Software(resource))
                } else if software_shared_by_id.contains_key(resource_id) {
                    bail!(
                        "task `{}` uses shared software resource `{resource_id}` as local",
                        task.id
                    )
                } else {
                    let hardware = resolve_hardware(&hardware_by_id, task.id, resource_id)?;
                    claim_local(&mut usage_by_resource, task.id, resource_id)?;
                    Ok(ResolvedTaskResource::Hardware(hardware))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let shared_resources = task
            .shared_resources
            .iter()
            .map(|resource_id| {
                if let Some(resource) = software_shared_by_id.get(resource_id).copied() {
                    software_shared_usage
                        .entry(resource_id)
                        .or_default()
                        .push(task.id);
                    Ok(ResolvedTaskResource::Software(resource))
                } else if software_local_by_id.contains_key(resource_id) {
                    bail!(
                        "task `{}` uses local software resource `{resource_id}` as shared",
                        task.id
                    )
                } else {
                    let hardware = resolve_hardware(&hardware_by_id, task.id, resource_id)?;
                    claim_shared(&mut usage_by_resource, task.id, resource_id)?;
                    Ok(ResolvedTaskResource::Hardware(hardware))
                }
            })
            .collect::<Result<Vec<_>>>()?;

        tasks.push(ResolvedTask {
            declaration: task,
            local_resources,
            shared_resources,
        });
    }

    let included_tasks = tasks
        .iter()
        .map(|task| (task.declaration.id, task.declaration))
        .collect::<BTreeMap<_, _>>();
    let init_spawned_tasks = app
        .init
        .spawns
        .iter()
        .map(|spawned| {
            included_tasks.get(spawned.id).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "init spawns `{}` but it is not included in tasks",
                    spawned.id
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut resources = Vec::with_capacity(usage_by_resource.len());
    let mut resource_initialization_order = Vec::with_capacity(usage_by_resource.len());
    for hardware in board.hardware {
        let Some(usage) = usage_by_resource.remove(hardware.id()) else {
            continue;
        };
        let usage = match usage {
            PendingUsage::Local { task_id } => ResolvedResourceUsage::Local { task_id },
            PendingUsage::Shared { task_ids } => ResolvedResourceUsage::Shared { task_ids },
        };
        resource_initialization_order.push(hardware.id());
        resources.push(ResolvedResource { hardware, usage });
    }
    let software_local_resources = app
        .software_resources
        .local
        .iter()
        .filter_map(|declaration| {
            software_local_usage
                .remove(declaration.id())
                .map(|task_ids| ResolvedSoftwareResource {
                    declaration,
                    task_ids,
                })
        })
        .collect();
    let software_shared_resources = app
        .software_resources
        .shared
        .iter()
        .filter_map(|declaration| {
            software_shared_usage
                .remove(declaration.id())
                .map(|task_ids| ResolvedSoftwareResource {
                    declaration,
                    task_ids,
                })
        })
        .collect();

    Ok(ResolvedApp {
        tasks,
        resources,
        software_local_resources,
        software_shared_resources,
        init_spawned_tasks,
        resource_initialization_order,
    })
}

fn resolve_hardware<'a>(
    hardware_by_id: &BTreeMap<&str, &'a HardwareDeclaration>,
    task_id: &str,
    resource_id: &str,
) -> Result<&'a HardwareDeclaration> {
    hardware_by_id.get(resource_id).copied().ok_or_else(|| {
        anyhow::anyhow!(
            "task `{task_id}` references resource `{resource_id}` which is not declared by the board or application"
        )
    })
}

fn claim_local(
    usage_by_resource: &mut BTreeMap<&str, PendingUsage>,
    task_id: &'static str,
    resource_id: &'static str,
) -> Result<()> {
    match usage_by_resource.get(resource_id) {
        Some(PendingUsage::Local {
            task_id: existing_task,
        }) => bail!(
            "hardware resource `{resource_id}` is claimed locally by both `{existing_task}` and `{task_id}`"
        ),
        Some(PendingUsage::Shared { .. }) => {
            bail!("hardware resource `{resource_id}` cannot be used as both local and shared")
        }
        None => {
            usage_by_resource.insert(resource_id, PendingUsage::Local { task_id });
            Ok(())
        }
    }
}

fn claim_shared(
    usage_by_resource: &mut BTreeMap<&str, PendingUsage>,
    task_id: &'static str,
    resource_id: &'static str,
) -> Result<()> {
    match usage_by_resource.get_mut(resource_id) {
        Some(PendingUsage::Local { .. }) => {
            bail!("hardware resource `{resource_id}` cannot be used as both local and shared")
        }
        Some(PendingUsage::Shared { task_ids }) => {
            task_ids.push(task_id);
            Ok(())
        }
        None => {
            usage_by_resource.insert(
                resource_id,
                PendingUsage::Shared {
                    task_ids: vec![task_id],
                },
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{
            InitDeclaration, SoftwareResourceDeclaration, SoftwareResourcesDeclaration, validate,
        },
        board::{
            ClockDeclaration, ClockSource, Mcu, MonotonicDeclaration, PhysicalPin, ResourceId,
        },
        task::TaskTrigger,
    };
    use std::collections::BTreeSet;

    const LED2: HardwareDeclaration =
        HardwareDeclaration::digital_output(ResourceId::new("led2"), PhysicalPin::new("PA5"));
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "nucleo_f401re",
        mcu: Mcu::Stm32F401RE,
        clocks: ClockDeclaration {
            source: ClockSource::Hsi,
            sysclk_hz: 84_000_000,
        },
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[LED2],
    };
    const BLINK: TaskDeclaration = TaskDeclaration {
        id: "blink_led",
        priority: 1,
        trigger: TaskTrigger::Spawned,
        args: &[],
        local_resources: &["led2"],
        shared_resources: &[],
    };

    fn resolve_valid(app: &AppDeclaration) -> Result<ResolvedApp<'_>> {
        validate(app)?;
        resolve(&BOARD, app)
    }

    #[test]
    fn blink_task_resolves_led2() {
        let app = AppDeclaration {
            init: InitDeclaration { spawns: &[BLINK] },
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        let resolved = resolve_valid(&app).unwrap();

        assert_eq!(resolved.tasks.len(), 1);
        assert_eq!(resolved.tasks[0].local_resources[0].id(), "led2");
        assert!(resolved.tasks[0].shared_resources.is_empty());
        assert_eq!(resolved.resources[0].hardware.id(), "led2");
        assert_eq!(resolved.resource_initialization_order, ["led2"]);
        assert_eq!(resolved.init_spawned_tasks[0].id, "blink_led");
    }

    #[test]
    fn missing_hardware_resource_fails() {
        const MISSING: TaskDeclaration = TaskDeclaration {
            id: "missing",
            priority: 1,
            trigger: TaskTrigger::Spawned,
            args: &[],
            local_resources: &["not_on_board"],
            shared_resources: &[],
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[MISSING],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve_valid(&app).unwrap_err();
        assert!(error.to_string().contains("not declared by the board"));
    }

    #[test]
    fn resolution_does_not_apply_stm32_backend_pin_rules() {
        const BACKEND_UNSUPPORTED_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[HardwareDeclaration::digital_output(
                ResourceId::new("led2"),
                PhysicalPin::new("PB0"),
            )],
            ..BOARD
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let resolved = resolve(&BACKEND_UNSUPPORTED_BOARD, &app).unwrap();
        assert_eq!(resolved.resources[0].hardware.id(), "led2");
        assert_eq!(
            resolved.resources[0].hardware.pin(),
            PhysicalPin::new("PB0")
        );
    }

    #[test]
    fn duplicate_local_ownership_fails() {
        const OTHER: TaskDeclaration = TaskDeclaration {
            id: "other",
            ..BLINK
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK, OTHER],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve_valid(&app).unwrap_err();
        assert!(error.to_string().contains("claimed locally by both"));
    }

    #[test]
    fn local_and_shared_use_fails() {
        const SHARED: TaskDeclaration = TaskDeclaration {
            id: "shared",
            priority: 1,
            trigger: TaskTrigger::Spawned,
            args: &[],
            local_resources: &[],
            shared_resources: &["led2"],
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK, SHARED],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve_valid(&app).unwrap_err();
        assert!(error.to_string().contains("both local and shared"));
    }

    #[test]
    fn empty_application_resolves_without_resources() {
        let resolved = resolve_valid(&AppDeclaration::EMPTY).unwrap();
        assert!(resolved.tasks.is_empty());
        assert!(resolved.resources.is_empty());
        assert!(resolved.init_spawned_tasks.is_empty());
        assert!(resolved.resource_initialization_order.is_empty());
    }

    #[test]
    fn resource_usage_tracks_shared_owners() {
        const FIRST: TaskDeclaration = TaskDeclaration {
            id: "first",
            priority: 1,
            trigger: TaskTrigger::Spawned,
            args: &[],
            local_resources: &[],
            shared_resources: &["led2"],
        };
        const SECOND: TaskDeclaration = TaskDeclaration {
            id: "second",
            ..FIRST
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[FIRST, SECOND],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let resolved = resolve_valid(&app).unwrap();
        assert_eq!(
            resolved.resources[0].usage,
            ResolvedResourceUsage::Shared {
                task_ids: vec!["first", "second"]
            }
        );
    }

    #[test]
    fn software_resources_resolve_by_shared_and_local_section() {
        const SOFTWARE_SHARED: &[SoftwareResourceDeclaration] =
            &[SoftwareResourceDeclaration::bool("enabled", true)];
        const SOFTWARE_LOCAL: &[SoftwareResourceDeclaration] = &[
            SoftwareResourceDeclaration::bool("previous_level", false),
            SoftwareResourceDeclaration::bool("unused", false),
        ];
        const SOFTWARE_TASK: TaskDeclaration = TaskDeclaration {
            id: "software_task",
            priority: 1,
            trigger: TaskTrigger::Spawned,
            args: &[],
            local_resources: &["previous_level"],
            shared_resources: &["enabled"],
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[SOFTWARE_TASK],
            software_resources: SoftwareResourcesDeclaration {
                shared: SOFTWARE_SHARED,
                local: SOFTWARE_LOCAL,
            },
        };

        let resolved = resolve_valid(&app).unwrap();
        assert_eq!(resolved.tasks[0].local_resources[0].id(), "previous_level");
        assert_eq!(resolved.tasks[0].shared_resources[0].id(), "enabled");
        assert_eq!(resolved.software_local_resources.len(), 1);
        assert_eq!(resolved.software_shared_resources.len(), 1);
        assert_eq!(
            resolved.software_local_resources[0].task_ids,
            ["software_task"]
        );
    }

    #[test]
    fn declaration_validation_rejects_duplicate_task_ids() {
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK, BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        assert!(
            validate(&app)
                .unwrap_err()
                .to_string()
                .contains("repeats task")
        );
    }

    #[test]
    fn declaration_validation_rejects_unincluded_init_spawn() {
        let app = AppDeclaration {
            init: InitDeclaration { spawns: &[BLINK] },
            tasks: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };
        assert!(
            validate(&app)
                .unwrap_err()
                .to_string()
                .contains("not included")
        );
    }

    #[test]
    fn hardware_lookup_is_unambiguous() {
        let ids = BOARD
            .hardware
            .iter()
            .map(HardwareDeclaration::id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), BOARD.hardware.len());
    }
}
