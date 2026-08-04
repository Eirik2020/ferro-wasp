//! Resolution of expanded task resources into validated board and software resources.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::{
    board::BoardDeclaration,
    component::{
        ExpandedApp, ExpandedResourceBinding, ExpandedResourceTarget, ExpandedSoftwareResource,
        ExpandedTask, ExpandedTaskTrigger,
    },
    hw_resources::{GpioMode, HardwareResource},
    task::{HardwareInterrupt, TaskResourceCapability, TaskResourceDefinition, TaskSafetyClass},
};

/// Expanded task declaration paired with resolved trigger and resource references.
#[derive(Debug)]
pub struct ResolvedTask<'a> {
    /// Expanded concrete task declaration.
    pub declaration: &'a ExpandedTask,

    /// Validated entry mechanism for the task.
    pub trigger: ResolvedTaskTrigger<'a>,

    /// Resources resolved from local task bindings.
    pub local_resources: Vec<ResolvedTaskResource<'a>>,

    /// Resources resolved from shared task bindings.
    pub shared_resources: Vec<ResolvedTaskResource<'a>>,
}

/// Validated mechanism that enters a resolved task.
#[derive(Clone, Copy, Debug)]
pub enum ResolvedTaskTrigger<'a> {
    /// Task is entered through RTIC's generated spawn API.
    Spawned,

    /// Task is entered by an interrupt belonging to board hardware.
    Interrupt {
        /// Board resource that owns the selected interrupt.
        resource: &'a HardwareResource,

        /// Interrupt role selected from the hardware resource.
        interrupt: HardwareInterrupt,
    },
}

/// Concrete declaration referenced by one expanded task binding.
#[derive(Clone, Copy, Debug)]
pub enum ResolvedTaskResource<'a> {
    /// Resource supplied by the selected board declaration.
    Hardware {
        /// Logical field used by the reusable task body.
        task_resource: &'a str,

        /// Matched board hardware declaration.
        hardware: &'a HardwareResource,
    },

    /// Resource supplied by the expanded software-resource declaration.
    Software {
        /// Logical field used by the reusable task body.
        task_resource: &'a str,

        /// Matched expanded software declaration.
        declaration: &'a ExpandedSoftwareResource,
    },
}

impl ResolvedTaskResource<'_> {
    /// Returns the logical field identifier used by the reusable task body.
    pub fn task_resource(&self) -> &str {
        match self {
            Self::Hardware { task_resource, .. } | Self::Software { task_resource, .. } => {
                task_resource
            }
        }
    }

    /// Returns the generated concrete resource identifier.
    pub fn id(&self) -> &str {
        match self {
            Self::Hardware { hardware, .. } => hardware.id(),
            Self::Software { declaration, .. } => &declaration.id,
        }
    }
}

/// RTIC ownership class and task ownership recorded for resolved hardware.
#[derive(Debug, Eq, PartialEq)]
pub enum ResolvedResourceUsage<'a> {
    /// Hardware is owned exclusively by one task.
    Local {
        /// Identifier of the owning task.
        task_id: &'a str,
    },

    /// Hardware is accessed through RTIC shared-resource locking.
    Shared {
        /// Identifiers of tasks that reference the shared resource.
        task_ids: Vec<&'a str>,
    },
}

/// Board hardware retained because at least one expanded task uses it.
#[derive(Debug)]
pub struct ResolvedResource<'a> {
    /// Matched board hardware declaration.
    pub hardware: &'a HardwareResource,

    /// Validated local or shared ownership of the hardware.
    pub usage: ResolvedResourceUsage<'a>,
}

/// Software resource retained because at least one expanded task uses it.
#[derive(Debug)]
pub struct ResolvedSoftwareResource<'a> {
    /// Matched expanded software-resource declaration.
    pub declaration: &'a ExpandedSoftwareResource,

    /// Identifiers of tasks that reference the software resource.
    pub task_ids: Vec<&'a str>,
}

/// Fully resolved inputs consumed by task and board renderers.
#[derive(Debug)]
pub struct ResolvedApp<'a> {
    /// Included tasks with resolved triggers and resources.
    pub tasks: Vec<ResolvedTask<'a>>,

    /// Used board hardware with validated RTIC ownership.
    pub resources: Vec<ResolvedResource<'a>>,

    /// Used software resources owned locally by one task.
    pub software_local_resources: Vec<ResolvedSoftwareResource<'a>>,

    /// Used software resources accessed as RTIC shared state.
    pub software_shared_resources: Vec<ResolvedSoftwareResource<'a>>,

    /// Included spawn-triggered tasks started by RTIC init.
    pub init_spawned_tasks: Vec<&'a ExpandedTask>,

    /// Hardware resource IDs in deterministic board initialization order.
    pub resource_initialization_order: Vec<&'static str>,

    /// Active static init-local storage produced by component expansion.
    pub init_local_resources: &'a [crate::component::ExpandedInitLocalResource],
}

enum PendingUsage<'a> {
    Local { task_id: &'a str },
    Shared { task_ids: Vec<&'a str> },
}

/// Resolves expanded task bindings and ownership against the selected board.
pub fn resolve<'a>(board: &'a BoardDeclaration, app: &'a ExpandedApp) -> Result<ResolvedApp<'a>> {
    let hardware_by_id = board
        .hardware
        .iter()
        .map(|hardware| (hardware.id(), hardware))
        .collect::<BTreeMap<_, _>>();
    let software_local_by_id = app
        .software_local_resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<BTreeMap<_, _>>();
    let software_shared_by_id = app
        .software_shared_resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<BTreeMap<_, _>>();
    let mut hardware_usage = BTreeMap::<&str, PendingUsage<'_>>::new();
    let mut software_local_usage = BTreeMap::<&str, Vec<&str>>::new();
    let mut software_shared_usage = BTreeMap::<&str, Vec<&str>>::new();
    let mut tasks = Vec::with_capacity(app.tasks.len());

    for task in &app.tasks {
        crate::task::validate_expanded_declaration(task)?;
        let local_resources = task
            .local_resources
            .iter()
            .map(|binding| {
                let required = definition_resource(
                    &task.id,
                    "local",
                    task.definition.local_resources,
                    binding,
                )?;
                resolve_local_binding(
                    task,
                    binding,
                    required.capability(),
                    &hardware_by_id,
                    &software_local_by_id,
                    &software_shared_by_id,
                    &mut hardware_usage,
                    &mut software_local_usage,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let shared_resources = task
            .shared_resources
            .iter()
            .map(|binding| {
                let required = definition_resource(
                    &task.id,
                    "shared",
                    task.definition.shared_resources,
                    binding,
                )?;
                resolve_shared_binding(
                    task,
                    binding,
                    required.capability(),
                    &hardware_by_id,
                    &software_local_by_id,
                    &software_shared_by_id,
                    &mut hardware_usage,
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
        .map(|task| (task.declaration.id.as_str(), task.declaration))
        .collect::<BTreeMap<_, _>>();
    let init_spawned_tasks = app
        .init_spawned_task_ids
        .iter()
        .map(|id| {
            included_tasks.get(id.as_str()).copied().ok_or_else(|| {
                anyhow::anyhow!("init spawns `{id}` but it is not included in expanded tasks")
            })
        })
        .collect::<Result<Vec<_>>>()?;

    for declaration in &app.software_local_resources {
        match declaration.kind.capability() {
            TaskResourceCapability::ObserverPublisher(_)
                if !software_local_usage.contains_key(declaration.id.as_str()) =>
            {
                bail!(
                    "observer publisher `{}` is not owned by a producing task",
                    declaration.id
                );
            }
            TaskResourceCapability::SafetyProducer(_)
            | TaskResourceCapability::SafetyConsumer(_) => {
                let owners = software_local_usage
                    .get(declaration.id.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                if owners.len() != 1 {
                    bail!(
                        "safety-channel handle `{}` requires exactly one owning task, found {}",
                        declaration.id,
                        owners.len()
                    );
                }
            }
            _ => {}
        }
    }

    let mut resources = Vec::with_capacity(hardware_usage.len());
    let mut resource_initialization_order = Vec::with_capacity(hardware_usage.len());
    for hardware in board.hardware {
        let Some(usage) = hardware_usage.remove(hardware.id()) else {
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
        .software_local_resources
        .iter()
        .filter_map(|declaration| {
            software_local_usage
                .remove(declaration.id.as_str())
                .map(|task_ids| ResolvedSoftwareResource {
                    declaration,
                    task_ids,
                })
        })
        .collect();
    let software_shared_resources = app
        .software_shared_resources
        .iter()
        .filter_map(|declaration| {
            software_shared_usage
                .remove(declaration.id.as_str())
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
        init_local_resources: &app.init_local_resources,
    })
}

fn definition_resource<'a>(
    task_id: &str,
    kind: &str,
    resources: &'a [TaskResourceDefinition],
    binding: &ExpandedResourceBinding,
) -> Result<&'a TaskResourceDefinition> {
    resources
        .iter()
        .find(|resource| resource.id() == binding.task_resource)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "task `{task_id}` has {kind} binding for undeclared body resource `{}`",
                binding.task_resource
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
        TaskResourceCapability::DigitalOutput => matches!(
            hardware.gpio().map(|gpio| gpio.mode),
            Some(GpioMode::Output { .. })
        ),
        TaskResourceCapability::InterruptInput => matches!(
            hardware.gpio().map(|gpio| gpio.mode),
            Some(GpioMode::Input { interrupt: Some(_) })
        ),
        TaskResourceCapability::UartRxDma => hardware.uart_rx_dma().is_some(),
        TaskResourceCapability::Software(_)
        | TaskResourceCapability::ObserverPublisher(_)
        | TaskResourceCapability::ObserverReader(_)
        | TaskResourceCapability::SafetyProducer(_)
        | TaskResourceCapability::SafetyConsumer(_) => false,
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
    task: &ExpandedTask,
    task_resource: &str,
    capability: TaskResourceCapability,
    declaration: &ExpandedSoftwareResource,
) -> Result<()> {
    if declaration.kind.capability() != capability {
        bail!(
            "task `{}` body resource `{task_resource}` requires {capability:?}, but software resource `{}` is incompatible",
            task.id,
            declaration.id
        );
    }
    if capability.is_safety_channel() && task.safety_class != TaskSafetyClass::SafetyCritical {
        bail!(
            "task `{}` is {:?} and cannot own authoritative safety-channel resource `{}`",
            task.id,
            task.safety_class,
            declaration.id
        );
    }
    if !declaration.exposed
        && declaration.owner_component.as_deref() != task.owner_component.as_deref()
    {
        bail!(
            "task `{}` cannot bind private resource `{}` owned by component `{}`",
            task.id,
            declaration.id,
            declaration
                .owner_component
                .as_deref()
                .unwrap_or("application")
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_local_binding<'a>(
    task: &'a ExpandedTask,
    binding: &'a ExpandedResourceBinding,
    capability: TaskResourceCapability,
    hardware_by_id: &BTreeMap<&str, &'a HardwareResource>,
    software_local_by_id: &BTreeMap<&str, &'a ExpandedSoftwareResource>,
    software_shared_by_id: &BTreeMap<&str, &'a ExpandedSoftwareResource>,
    hardware_usage: &mut BTreeMap<&'a str, PendingUsage<'a>>,
    software_usage: &mut BTreeMap<&'a str, Vec<&'a str>>,
) -> Result<ResolvedTaskResource<'a>> {
    match &binding.target {
        ExpandedResourceTarget::Hardware(resource_id) => {
            let hardware = resolve_hardware(hardware_by_id, &task.id, resource_id)?;
            validate_hardware_capability(&task.id, &binding.task_resource, capability, hardware)?;
            claim_local(hardware_usage, &task.id, hardware.id())?;
            Ok(ResolvedTaskResource::Hardware {
                task_resource: &binding.task_resource,
                hardware,
            })
        }
        ExpandedResourceTarget::Software(resource_id) => {
            let declaration = software_local_by_id.get(resource_id.as_str()).copied().ok_or_else(|| {
                if software_shared_by_id.contains_key(resource_id.as_str()) {
                    anyhow::anyhow!(
                        "task `{}` binds shared software resource `{resource_id}` as local",
                        task.id
                    )
                } else {
                    anyhow::anyhow!(
                        "task `{}` references software resource `{resource_id}` which is not declared by the expanded application",
                        task.id
                    )
                }
            })?;
            validate_software_capability(task, &binding.task_resource, capability, declaration)?;
            let owners = software_usage.entry(resource_id).or_default();
            if let Some(existing) = owners.first() {
                bail!(
                    "software resource `{resource_id}` is claimed locally by both `{existing}` and `{}`",
                    task.id
                );
            }
            owners.push(&task.id);
            Ok(ResolvedTaskResource::Software {
                task_resource: &binding.task_resource,
                declaration,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_shared_binding<'a>(
    task: &'a ExpandedTask,
    binding: &'a ExpandedResourceBinding,
    capability: TaskResourceCapability,
    hardware_by_id: &BTreeMap<&str, &'a HardwareResource>,
    software_local_by_id: &BTreeMap<&str, &'a ExpandedSoftwareResource>,
    software_shared_by_id: &BTreeMap<&str, &'a ExpandedSoftwareResource>,
    hardware_usage: &mut BTreeMap<&'a str, PendingUsage<'a>>,
    software_usage: &mut BTreeMap<&'a str, Vec<&'a str>>,
) -> Result<ResolvedTaskResource<'a>> {
    match &binding.target {
        ExpandedResourceTarget::Hardware(resource_id) => {
            let hardware = resolve_hardware(hardware_by_id, &task.id, resource_id)?;
            validate_hardware_capability(&task.id, &binding.task_resource, capability, hardware)?;
            claim_shared(hardware_usage, &task.id, hardware.id())?;
            Ok(ResolvedTaskResource::Hardware {
                task_resource: &binding.task_resource,
                hardware,
            })
        }
        ExpandedResourceTarget::Software(resource_id) => {
            let declaration = software_shared_by_id.get(resource_id.as_str()).copied().ok_or_else(|| {
                if software_local_by_id.contains_key(resource_id.as_str()) {
                    anyhow::anyhow!(
                        "task `{}` binds local software resource `{resource_id}` as shared",
                        task.id
                    )
                } else {
                    anyhow::anyhow!(
                        "task `{}` references software resource `{resource_id}` which is not declared by the expanded application",
                        task.id
                    )
                }
            })?;
            validate_software_capability(task, &binding.task_resource, capability, declaration)?;
            software_usage
                .entry(resource_id)
                .or_default()
                .push(&task.id);
            Ok(ResolvedTaskResource::Software {
                task_resource: &binding.task_resource,
                declaration,
            })
        }
    }
}

fn resolve_task_trigger<'a>(
    task: &'a ExpandedTask,
    local_resources: &[ResolvedTaskResource<'a>],
    shared_resources: &[ResolvedTaskResource<'a>],
) -> Result<ResolvedTaskTrigger<'a>> {
    let trigger = match &task.trigger {
        ExpandedTaskTrigger::Spawned => ResolvedTaskTrigger::Spawned,
        ExpandedTaskTrigger::Interrupt {
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
                    anyhow::anyhow!(
                        "interrupt task `{}` must bind trigger resource `{resource}` with the required ownership",
                        task.id
                    )
                })?;
            let ResolvedTaskResource::Hardware { hardware, .. } = resolved else {
                bail!(
                    "interrupt task `{}` trigger resource `{resource}` must bind board hardware",
                    task.id
                );
            };
            match interrupt {
                HardwareInterrupt::Primary
                    if !matches!(
                        hardware.gpio().map(|gpio| gpio.mode),
                        Some(GpioMode::Input { interrupt: Some(_) })
                    ) =>
                {
                    bail!(
                        "interrupt task `{}` trigger resource `{resource}` must be an interrupt-enabled digital input",
                        task.id
                    )
                }
                HardwareInterrupt::DmaRx | HardwareInterrupt::Peripheral
                    if hardware.uart_rx_dma().is_none() =>
                {
                    bail!(
                        "interrupt task `{}` trigger resource `{resource}` must be a DMA-backed UART receiver",
                        task.id
                    )
                }
                _ => {}
            }
            ResolvedTaskTrigger::Interrupt {
                resource: hardware,
                interrupt: *interrupt,
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

fn claim_local<'a>(
    usage_by_resource: &mut BTreeMap<&'a str, PendingUsage<'a>>,
    task_id: &'a str,
    resource_id: &'a str,
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

fn claim_shared<'a>(
    usage_by_resource: &mut BTreeMap<&'a str, PendingUsage<'a>>,
    task_id: &'a str,
    resource_id: &'a str,
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
        app::{AppDeclaration, InitDeclaration, SoftwareResourcesDeclaration},
        board::MonotonicDeclaration,
        component::expand,
        hw_resources::{ClockSource, Gpio, Level, Mcu, PinId, Target},
        task::{TaskDeclaration, TaskDefinition, digital_output, resource},
    };

    const LED: HardwareResource =
        HardwareResource::Gpio(Gpio::output("led", PinId::new(0, 5), Level::Low));
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "board",
        target: Target::internal_high_speed(Mcu::Stm32F401, 84_000_000),
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[LED],
    };
    const OUTPUT: TaskDefinition =
        TaskDefinition::asynchronous("output").with_local(&[digital_output("pin")]);
    const USE_LED: TaskDeclaration = OUTPUT
        .spawned_as("use_led")
        .priority(1)
        .with_local(&[resource("pin").to_hw("led")]);

    fn app(tasks: &'static [TaskDeclaration]) -> AppDeclaration {
        AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks,
            components: &[],
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        }
    }

    #[test]
    fn resolves_used_hardware_and_omits_unused_hardware() {
        let expanded = expand(&BOARD, &app(&[USE_LED])).unwrap();
        let resolved = resolve(&BOARD, &expanded).unwrap();
        assert_eq!(resolved.resources.len(), 1);
        assert_eq!(resolved.resources[0].hardware.id(), "led");

        let empty = expand(&BOARD, &AppDeclaration::EMPTY).unwrap();
        assert!(resolve(&BOARD, &empty).unwrap().resources.is_empty());
    }

    #[test]
    fn missing_hardware_fails() {
        const MISSING: TaskDeclaration = OUTPUT
            .spawned_as("missing")
            .priority(1)
            .with_local(&[resource("pin").to_hw("missing_led")]);
        let expanded = expand(&BOARD, &app(&[MISSING])).unwrap();
        assert!(
            resolve(&BOARD, &expanded)
                .unwrap_err()
                .to_string()
                .contains("not declared")
        );
    }

    #[test]
    fn duplicate_local_ownership_fails() {
        const OTHER: TaskDeclaration = OUTPUT
            .spawned_as("other")
            .priority(1)
            .with_local(&[resource("pin").to_hw("led")]);
        let expanded = expand(&BOARD, &app(&[USE_LED, OTHER])).unwrap();
        assert!(
            resolve(&BOARD, &expanded)
                .unwrap_err()
                .to_string()
                .contains("claimed locally")
        );
    }

    #[test]
    fn board_test_uses_expected_clock_source() {
        assert_eq!(BOARD.target.clock.source, ClockSource::InternalHighSpeed);
    }
}
