//! Resolution of application resource names into validated board and software resources.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::{
    app::{AppDeclaration, SoftwareResourceDeclaration},
    board::BoardDeclaration,
    hw_resources::{GpioMode, HardwareResource},
    task::{
        HardwareInterrupt, ResourceBinding, ResourceTarget, TaskDeclaration,
        TaskResourceCapability, TaskResourceDefinition, TaskTrigger, validate_declaration,
    },
};

/// Task declaration paired with its resolved trigger and resource references.
#[derive(Debug)]
pub struct ResolvedTask<'a> {
    /// Original task declaration.
    pub declaration: &'a TaskDeclaration,

    /// Validated entry mechanism for the task.
    pub trigger: ResolvedTaskTrigger<'a>,

    /// Resources resolved from the task's local-resource identifiers.
    pub local_resources: Vec<ResolvedTaskResource<'a>>,

    /// Resources resolved from the task's shared-resource identifiers.
    pub shared_resources: Vec<ResolvedTaskResource<'a>>,
}

/// Validated mechanism that enters a resolved task.
#[derive(Clone, Copy, Debug)]
pub enum ResolvedTaskTrigger<'a> {
    /// Task is entered through RTIC's generated `spawn` API.
    Spawned,

    /// Task is entered by the interrupt belonging to a hardware input.
    Interrupt {
        /// Interrupt-enabled local hardware resource bound to the task.
        resource: &'a HardwareResource,

        /// Interrupt role selected from the hardware resource.
        interrupt: HardwareInterrupt,
    },
}

/// Concrete declaration referenced by a task resource identifier.
#[derive(Clone, Copy, Debug)]
pub enum ResolvedTaskResource<'a> {
    /// Resource supplied by the selected board declaration.
    Hardware {
        /// Logical field used by the reusable task body.
        task_resource: &'static str,

        /// Matched board hardware declaration.
        hardware: &'a HardwareResource,
    },

    /// Resource supplied by the application software-resource declaration.
    Software {
        /// Logical field used by the reusable task body.
        task_resource: &'static str,

        /// Matched application software-resource declaration.
        declaration: &'a SoftwareResourceDeclaration,
    },
}

impl ResolvedTaskResource<'_> {
    /// Returns the logical field identifier used by the reusable task body.
    pub fn task_resource(&self) -> &'static str {
        match self {
            Self::Hardware { task_resource, .. } | Self::Software { task_resource, .. } => {
                task_resource
            }
        }
    }

    /// Returns the generated field identifier of the resolved resource.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Hardware { hardware, .. } => hardware.id(),
            Self::Software { declaration, .. } => declaration.id(),
        }
    }
}

/// RTIC ownership class and task ownership recorded for resolved hardware.
#[derive(Debug, Eq, PartialEq)]
pub enum ResolvedResourceUsage {
    /// Hardware is owned exclusively by one task.
    Local {
        /// Identifier of the owning task.
        task_id: &'static str,
    },

    /// Hardware is accessed through RTIC shared-resource locking.
    Shared {
        /// Identifiers of tasks that reference the shared resource.
        task_ids: Vec<&'static str>,
    },
}

/// Board hardware retained because at least one included task uses it.
#[derive(Debug)]
pub struct ResolvedResource<'a> {
    /// Matched board hardware declaration.
    pub hardware: &'a HardwareResource,

    /// Validated local or shared ownership of the hardware.
    pub usage: ResolvedResourceUsage,
}

/// Software resource retained because at least one included task uses it.
#[derive(Debug)]
pub struct ResolvedSoftwareResource<'a> {
    /// Matched application software-resource declaration.
    pub declaration: &'a SoftwareResourceDeclaration,

    /// Identifiers of tasks that reference the software resource.
    pub task_ids: Vec<&'static str>,
}

/// Fully resolved inputs consumed by task and board renderers.
#[derive(Debug)]
pub struct ResolvedApp<'a> {
    /// Included tasks with their resolved triggers and resources.
    pub tasks: Vec<ResolvedTask<'a>>,

    /// Used board hardware with validated RTIC ownership.
    pub resources: Vec<ResolvedResource<'a>>,

    /// Used application resources owned locally by one task.
    pub software_local_resources: Vec<ResolvedSoftwareResource<'a>>,

    /// Used application resources accessed as RTIC shared resources.
    pub software_shared_resources: Vec<ResolvedSoftwareResource<'a>>,

    /// Included spawn-triggered tasks started by RTIC `init`.
    pub init_spawned_tasks: Vec<&'a TaskDeclaration>,

    /// Hardware resource IDs in deterministic board initialization order.
    pub resource_initialization_order: Vec<&'static str>,
}

enum PendingUsage {
    Local { task_id: &'static str },
    Shared { task_ids: Vec<&'static str> },
}

/// Resolves task resource identifiers and init spawns against an application and board.
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
        validate_declaration(task)?;
        let local_resources = task
            .local_resources
            .iter()
            .map(|binding| {
                let required = definition_resource(
                    task.id,
                    "local",
                    task.definition.local_resources,
                    binding,
                )?;
                resolve_local_binding(
                    binding,
                    required.capability(),
                    task.id,
                    &hardware_by_id,
                    &software_local_by_id,
                    &software_shared_by_id,
                    &mut usage_by_resource,
                    &mut software_local_usage,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let shared_resources = task
            .shared_resources
            .iter()
            .map(|binding| {
                let required = definition_resource(
                    task.id,
                    "shared",
                    task.definition.shared_resources,
                    binding,
                )?;
                resolve_shared_binding(
                    binding,
                    required.capability(),
                    task.id,
                    &hardware_by_id,
                    &software_local_by_id,
                    &software_shared_by_id,
                    &mut usage_by_resource,
                    &mut software_shared_usage,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let trigger = resolve_task_trigger(task, &local_resources, &shared_resources)?;

        tasks.push(ResolvedTask {
            declaration: task,
            trigger,
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

fn definition_resource<'a>(
    task_id: &str,
    kind: &str,
    resources: &'a [TaskResourceDefinition],
    binding: &ResourceBinding,
) -> Result<&'a TaskResourceDefinition> {
    resources
        .iter()
        .find(|resource| resource.id() == binding.task_resource())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "task `{task_id}` has {kind} binding for undeclared body resource `{}`",
                binding.task_resource()
            )
        })
}

fn validate_hardware_capability(
    task_id: &str,
    task_resource: &str,
    capability: TaskResourceCapability,
    hardware: &HardwareResource,
) -> Result<()> {
    let compatible = match capability {
        TaskResourceCapability::DigitalOutput => {
            matches!(
                hardware.gpio().map(|gpio| gpio.mode),
                Some(GpioMode::Output { .. })
            )
        }
        TaskResourceCapability::InterruptInput => {
            matches!(
                hardware.gpio().map(|gpio| gpio.mode),
                Some(GpioMode::Input { interrupt: Some(_) })
            )
        }
        TaskResourceCapability::Bool => false,
        TaskResourceCapability::UartRxDma => hardware.uart_rx_dma().is_some(),
    };
    if !compatible {
        bail!(
            "task `{task_id}` body resource `{task_resource}` requires {capability:?}, but hardware `{}` is incompatible",
            hardware.id()
        );
    }
    Ok(())
}

fn validate_software_capability(
    task_id: &str,
    task_resource: &str,
    capability: TaskResourceCapability,
    declaration: &SoftwareResourceDeclaration,
) -> Result<()> {
    let compatible = matches!(
        (capability, declaration),
        (
            TaskResourceCapability::Bool,
            SoftwareResourceDeclaration::Bool { .. }
        )
    );
    if !compatible {
        bail!(
            "task `{task_id}` body resource `{task_resource}` requires {capability:?}, but software resource `{}` is incompatible",
            declaration.id()
        );
    }
    Ok(())
}

fn resolve_local_binding<'a>(
    binding: &ResourceBinding,
    capability: TaskResourceCapability,
    task_id: &'static str,
    hardware_by_id: &BTreeMap<&str, &'a HardwareResource>,
    software_local_by_id: &BTreeMap<&str, &'a SoftwareResourceDeclaration>,
    software_shared_by_id: &BTreeMap<&str, &'a SoftwareResourceDeclaration>,
    hardware_usage: &mut BTreeMap<&str, PendingUsage>,
    software_usage: &mut BTreeMap<&str, Vec<&'static str>>,
) -> Result<ResolvedTaskResource<'a>> {
    let task_resource = binding.task_resource();
    match binding.target() {
        ResourceTarget::Hardware(resource_id) => {
            let hardware = resolve_hardware(hardware_by_id, task_id, resource_id)?;
            validate_hardware_capability(task_id, task_resource, capability, hardware)?;
            claim_local(hardware_usage, task_id, resource_id)?;
            Ok(ResolvedTaskResource::Hardware {
                task_resource,
                hardware,
            })
        }
        ResourceTarget::Software(resource_id) => {
            let declaration = software_local_by_id
                .get(resource_id)
                .copied()
                .ok_or_else(|| {
                    if software_shared_by_id.contains_key(resource_id) {
                        anyhow::anyhow!(
                            "task `{task_id}` binds shared software resource `{resource_id}` as local"
                        )
                    } else {
                        anyhow::anyhow!(
                            "task `{task_id}` references software resource `{resource_id}` which is not declared by the application"
                        )
                    }
                })?;
            validate_software_capability(task_id, task_resource, capability, declaration)?;
            let owners = software_usage.entry(resource_id).or_default();
            if let Some(existing) = owners.first() {
                bail!(
                    "software resource `{resource_id}` is claimed locally by both `{existing}` and `{task_id}`"
                );
            }
            owners.push(task_id);
            Ok(ResolvedTaskResource::Software {
                task_resource,
                declaration,
            })
        }
    }
}

fn resolve_shared_binding<'a>(
    binding: &ResourceBinding,
    capability: TaskResourceCapability,
    task_id: &'static str,
    hardware_by_id: &BTreeMap<&str, &'a HardwareResource>,
    software_local_by_id: &BTreeMap<&str, &'a SoftwareResourceDeclaration>,
    software_shared_by_id: &BTreeMap<&str, &'a SoftwareResourceDeclaration>,
    hardware_usage: &mut BTreeMap<&str, PendingUsage>,
    software_usage: &mut BTreeMap<&str, Vec<&'static str>>,
) -> Result<ResolvedTaskResource<'a>> {
    let task_resource = binding.task_resource();
    match binding.target() {
        ResourceTarget::Hardware(resource_id) => {
            let hardware = resolve_hardware(hardware_by_id, task_id, resource_id)?;
            validate_hardware_capability(task_id, task_resource, capability, hardware)?;
            claim_shared(hardware_usage, task_id, resource_id)?;
            Ok(ResolvedTaskResource::Hardware {
                task_resource,
                hardware,
            })
        }
        ResourceTarget::Software(resource_id) => {
            let declaration = software_shared_by_id
                .get(resource_id)
                .copied()
                .ok_or_else(|| {
                    if software_local_by_id.contains_key(resource_id) {
                        anyhow::anyhow!(
                            "task `{task_id}` binds local software resource `{resource_id}` as shared"
                        )
                    } else {
                        anyhow::anyhow!(
                            "task `{task_id}` references software resource `{resource_id}` which is not declared by the application"
                        )
                    }
                })?;
            validate_software_capability(task_id, task_resource, capability, declaration)?;
            software_usage.entry(resource_id).or_default().push(task_id);
            Ok(ResolvedTaskResource::Software {
                task_resource,
                declaration,
            })
        }
    }
}

fn resolve_task_trigger<'a>(
    task: &TaskDeclaration,
    local_resources: &[ResolvedTaskResource<'a>],
    shared_resources: &[ResolvedTaskResource<'a>],
) -> Result<ResolvedTaskTrigger<'a>> {
    let trigger = match task.trigger {
        TaskTrigger::Spawned => ResolvedTaskTrigger::Spawned,
        TaskTrigger::Interrupt {
            resource,
            interrupt,
        } => {
            let candidates = match interrupt {
                HardwareInterrupt::Primary => local_resources,
                HardwareInterrupt::DmaRx | HardwareInterrupt::Peripheral => shared_resources,
            };
            let resolved = candidates
                .iter()
                .find(|resolved| resolved.task_resource() == resource)
                .ok_or_else(|| {
                    let ownership = match interrupt {
                        HardwareInterrupt::Primary => "locally",
                        HardwareInterrupt::DmaRx | HardwareInterrupt::Peripheral => "as shared",
                    };
                    anyhow::anyhow!(
                        "interrupt task `{}` must bind trigger body resource `{resource}` {ownership}",
                        task.id
                    )
                })?;
            let ResolvedTaskResource::Hardware { hardware, .. } = resolved else {
                bail!(
                    "interrupt task `{}` trigger body resource `{resource}` must bind board hardware",
                    task.id
                );
            };
            match interrupt {
                HardwareInterrupt::Primary => {
                    if !matches!(
                        hardware.gpio().map(|gpio| gpio.mode),
                        Some(GpioMode::Input { interrupt: Some(_) })
                    ) {
                        bail!(
                            "interrupt task `{}` trigger resource `{resource}` must be an interrupt-enabled digital input",
                            task.id
                        );
                    }
                }
                HardwareInterrupt::DmaRx | HardwareInterrupt::Peripheral => {
                    if hardware.uart_rx_dma().is_none() {
                        bail!(
                            "interrupt task `{}` trigger resource `{resource}` must be a DMA-backed UART receiver",
                            task.id
                        );
                    }
                }
            }
            ResolvedTaskTrigger::Interrupt {
                resource: hardware,
                interrupt,
            }
        }
    };

    let trigger_resource_id = match trigger {
        ResolvedTaskTrigger::Spawned => None,
        ResolvedTaskTrigger::Interrupt { resource, .. } => Some(resource.id()),
    };
    for local in local_resources {
        let ResolvedTaskResource::Hardware { hardware, .. } = local else {
            continue;
        };
        if matches!(
            hardware.gpio().map(|gpio| gpio.mode),
            Some(GpioMode::Input { interrupt: Some(_) })
        ) && trigger_resource_id != Some(hardware.id())
        {
            bail!(
                "task `{}` locally claims interrupt-enabled input `{}` without using it as its interrupt trigger",
                task.id,
                hardware.id()
            );
        }
    }

    Ok(trigger)
}

fn resolve_hardware<'a>(
    hardware_by_id: &BTreeMap<&str, &'a HardwareResource>,
    task_id: &str,
    resource_id: &str,
) -> Result<&'a HardwareResource> {
    hardware_by_id.get(resource_id).copied().ok_or_else(|| {
        anyhow::anyhow!(
            "task `{task_id}` references hardware resource `{resource_id}` which is not declared by the board"
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
        board::MonotonicDeclaration,
        hw_resources::{
            Clock, ClockSource, Gpio, HardwareResource, InterruptEdge, Level, Mcu, PinId, Target,
        },
        task::{TaskDefinition, boolean, digital_output, interrupt_input, resource},
    };
    use std::collections::BTreeSet;

    const fn output(id: &'static str, port: u8, pin: u8) -> HardwareResource {
        HardwareResource::Gpio(Gpio::output(id, PinId::new(port, pin), Level::Low))
    }

    const fn exti_input(id: &'static str, port: u8, pin: u8) -> HardwareResource {
        HardwareResource::Gpio(
            Gpio::input(id, PinId::new(port, pin))
                .pull_up()
                .interrupt_on(InterruptEdge::Falling),
        )
    }

    const LED2: HardwareResource = output("led2", 0, 5);
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "nucleo_f401re",
        target: Target {
            mcu: Mcu::Stm32F401,
            clock: Clock {
                source: ClockSource::InternalHighSpeed,
                sysclk_hz: 84_000_000,
            },
        },
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[LED2],
    };
    const BLINK_DEFINITION: TaskDefinition =
        TaskDefinition::asynchronous("blink").with_local(&[digital_output("led")]);
    const BLINK: TaskDeclaration = BLINK_DEFINITION
        .spawned_as("blink_led")
        .priority(1)
        .with_local(&[resource("led").to_hw("led2")]);

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
        assert_eq!(resolved.tasks[0].local_resources[0].task_resource(), "led");
        assert_eq!(resolved.tasks[0].local_resources[0].id(), "led2");
        assert!(resolved.tasks[0].shared_resources.is_empty());
        assert_eq!(resolved.resources[0].hardware.id(), "led2");
        assert_eq!(resolved.resource_initialization_order, ["led2"]);
        assert_eq!(resolved.init_spawned_tasks[0].id, "blink_led");
    }

    #[test]
    fn missing_hardware_resource_fails() {
        const MISSING_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("missing").with_local(&[digital_output("missing")]);
        const MISSING: TaskDeclaration = MISSING_DEFINITION
            .spawned_as("missing")
            .priority(1)
            .with_local(&[resource("missing").to_hw("not_on_board")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[MISSING],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve_valid(&app).unwrap_err();
        assert!(error.to_string().contains("not declared by the board"));
    }

    #[test]
    fn hardware_binding_does_not_fall_back_to_software() {
        const ENABLED: &[SoftwareResourceDeclaration] =
            &[SoftwareResourceDeclaration::bool("enabled", true)];
        const TASK_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("task").with_shared(&[boolean("enabled")]);
        const TASK: TaskDeclaration = TASK_DEFINITION
            .spawned_as("task")
            .priority(1)
            .with_shared(&[resource("enabled").to_hw("enabled")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[TASK],
            software_resources: SoftwareResourcesDeclaration {
                shared: ENABLED,
                local: &[],
            },
        };

        let error = resolve_valid(&app).unwrap_err();
        assert!(error.to_string().contains("not declared by the board"));
    }

    #[test]
    fn software_binding_does_not_fall_back_to_hardware() {
        const TASK_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("task").with_local(&[digital_output("led")]);
        const TASK: TaskDeclaration = TASK_DEFINITION
            .spawned_as("task")
            .priority(1)
            .with_local(&[resource("led").to_sw("led2")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve_valid(&app).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("not declared by the application")
        );
    }

    #[test]
    fn digital_output_capability_rejects_an_input_binding() {
        const INPUT: HardwareResource =
            HardwareResource::Gpio(Gpio::input("input", PinId::new(2, 13)));
        const INPUT_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[INPUT],
            ..BOARD
        };
        const DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("output").with_local(&[digital_output("pin")]);
        const TASK: TaskDeclaration = DEFINITION
            .spawned_as("output_task")
            .priority(1)
            .with_local(&[resource("pin").to_hw("input")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve(&INPUT_BOARD, &app).unwrap_err();
        assert!(error.to_string().contains("requires DigitalOutput"));
    }

    #[test]
    fn interrupt_input_capability_rejects_an_output_binding() {
        const DEFINITION: TaskDefinition =
            TaskDefinition::synchronous("interrupt").with_local(&[interrupt_input("pin")]);
        const TASK: TaskDeclaration = DEFINITION
            .interrupt_as("interrupt_task", "pin")
            .priority(1)
            .with_local(&[resource("pin").to_hw("led2")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve(&BOARD, &app).unwrap_err();
        assert!(error.to_string().contains("requires InterruptInput"));
    }

    #[test]
    fn boolean_capability_rejects_a_hardware_binding() {
        const DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("boolean").with_local(&[boolean("value")]);
        const TASK: TaskDeclaration = DEFINITION
            .spawned_as("boolean_task")
            .priority(1)
            .with_local(&[resource("value").to_hw("led2")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve(&BOARD, &app).unwrap_err();
        assert!(error.to_string().contains("requires Bool"));
    }

    #[test]
    fn resolution_does_not_apply_stm32_backend_pin_rules() {
        const BACKEND_UNSUPPORTED_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[output("led2", 25, 0)],
            ..BOARD
        };
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BLINK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let resolved = resolve(&BACKEND_UNSUPPORTED_BOARD, &app).unwrap();
        assert_eq!(resolved.resources[0].hardware.id(), "led2");
        assert_eq!(resolved.resources[0].hardware.pin(), PinId::new(25, 0));
    }

    #[test]
    fn duplicate_local_ownership_fails() {
        const OTHER: TaskDeclaration = BLINK_DEFINITION
            .spawned_as("other")
            .priority(1)
            .with_local(&[resource("led").to_hw("led2")]);
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
        const SHARED_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("shared").with_shared(&[digital_output("led")]);
        const SHARED: TaskDeclaration = SHARED_DEFINITION
            .spawned_as("shared")
            .priority(1)
            .with_shared(&[resource("led").to_hw("led2")]);
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
        const SHARED_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("shared_output").with_shared(&[digital_output("led")]);
        const FIRST: TaskDeclaration = SHARED_DEFINITION
            .spawned_as("first")
            .priority(1)
            .with_shared(&[resource("led").to_hw("led2")]);
        const SECOND: TaskDeclaration = SHARED_DEFINITION
            .spawned_as("second")
            .priority(1)
            .with_shared(&[resource("led").to_hw("led2")]);
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
        const SOFTWARE_DEFINITION: TaskDefinition = TaskDefinition::asynchronous("software")
            .with_local(&[boolean("previous")])
            .with_shared(&[boolean("enabled")]);
        const SOFTWARE_TASK: TaskDeclaration = SOFTWARE_DEFINITION
            .spawned_as("software_task")
            .priority(1)
            .with_local(&[resource("previous").to_sw("previous_level")])
            .with_shared(&[resource("enabled").to_sw("enabled")]);
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
    fn interrupt_trigger_resolves_its_local_hardware_input() {
        const BUTTON: HardwareResource = exti_input("button", 2, 13);
        const BUTTON_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[BUTTON],
            ..BOARD
        };
        const BUTTON_DEFINITION: TaskDefinition =
            TaskDefinition::synchronous("button").with_local(&[interrupt_input("input")]);
        const BUTTON_TASK: TaskDeclaration = BUTTON_DEFINITION
            .interrupt_as("button_irq", "input")
            .priority(2)
            .with_local(&[resource("input").to_hw("button")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[BUTTON_TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let resolved = resolve(&BUTTON_BOARD, &app).unwrap();
        let ResolvedTaskTrigger::Interrupt { resource, .. } = resolved.tasks[0].trigger else {
            panic!("expected resolved interrupt trigger")
        };
        assert_eq!(resource.id(), "button");
    }

    #[test]
    fn interrupt_trigger_must_be_claimed_locally() {
        const INTERRUPT_DEFINITION: TaskDefinition =
            TaskDefinition::synchronous("invalid").with_shared(&[digital_output("led")]);
        const INTERRUPT_TASK: TaskDeclaration = INTERRUPT_DEFINITION
            .interrupt_as("invalid_irq", "led")
            .priority(2)
            .with_shared(&[resource("led").to_hw("led2")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[INTERRUPT_TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve(&BOARD, &app).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must bind trigger body resource `led` locally")
        );
    }

    #[test]
    fn interrupt_trigger_must_be_interrupt_enabled_input() {
        const POLLED_INPUT: HardwareResource =
            HardwareResource::Gpio(Gpio::input("polled_input", PinId::new(2, 13)).pull_up());
        const INPUT_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[POLLED_INPUT],
            ..BOARD
        };
        const INTERRUPT_DEFINITION: TaskDefinition =
            TaskDefinition::synchronous("invalid").with_local(&[interrupt_input("input")]);
        const INTERRUPT_TASK: TaskDeclaration = INTERRUPT_DEFINITION
            .interrupt_as("invalid_irq", "input")
            .priority(2)
            .with_local(&[resource("input").to_hw("polled_input")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[INTERRUPT_TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve(&INPUT_BOARD, &app).unwrap_err();
        assert!(error.to_string().contains("requires InterruptInput"));
    }

    #[test]
    fn spawned_task_cannot_own_an_enabled_interrupt_input() {
        const BUTTON: HardwareResource = exti_input("button", 2, 13);
        const BUTTON_BOARD: BoardDeclaration = BoardDeclaration {
            hardware: &[BUTTON],
            ..BOARD
        };
        const POLL_DEFINITION: TaskDefinition =
            TaskDefinition::asynchronous("poll").with_local(&[interrupt_input("input")]);
        const POLL_TASK: TaskDeclaration = POLL_DEFINITION
            .spawned_as("poll_button")
            .priority(1)
            .with_local(&[resource("input").to_hw("button")]);
        let app = AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks: &[POLL_TASK],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        };

        let error = resolve(&BUTTON_BOARD, &app).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("without using it as its interrupt trigger")
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
            .map(HardwareResource::id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), BOARD.hardware.len());
    }
}
