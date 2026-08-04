//! Generic component declarations and deterministic expansion into app resources and tasks.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use ferrowasp_io_core::serial::{RcProtocol, SerialPortAssignment, SerialProtocol};

use crate::{
    app::{AppDeclaration, SoftwareResourceDeclaration},
    board::BoardDeclaration,
    task::{
        HardwareInterrupt, ResourceBinding, ResourceTarget, TaskDefinition, TaskParameterBinding,
        TaskResourceCapability, TaskTrigger,
    },
};

/// Configuration family accepted by a reusable component definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentConfigurationKind {
    /// A receive-only boot-assigned serial port.
    SerialPort,
}

/// Typed configuration supplied to one component instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentConfiguration {
    /// Startup assignment for a serial-port component.
    SerialPort(SerialPortAssignment),
}

impl ComponentConfiguration {
    const fn kind(self) -> ComponentConfigurationKind {
        match self {
            Self::SerialPort(_) => ComponentConfigurationKind::SerialPort,
        }
    }
}

/// Controls whether a component artifact exists for a selected configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentActivation {
    /// Artifact is present for every configuration.
    Always,

    /// Artifact is omitted when a serial port is disabled.
    SerialEnabled,
}

impl ComponentActivation {
    fn active(self, configuration: ComponentConfiguration) -> bool {
        match (self, configuration) {
            (Self::Always, _) => true,
            (Self::SerialEnabled, ComponentConfiguration::SerialPort(assignment)) => {
                assignment != SerialPortAssignment::Disabled
            }
        }
    }
}

/// Visibility of a software resource owned by a component instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentResourceVisibility {
    /// Only tasks expanded from the owning component instance may bind it.
    Private,

    /// Standalone tasks and other components may bind the namespaced output.
    Exposed,
}

/// Software-resource type created by a component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentSoftwareResource {
    /// Latest decoded radio-control sample.
    RcInputSnapshot,

    /// Parser selected from the component's serial-port configuration.
    SerialConsumer,
}

/// Backend-owned static storage requested by a component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentInitLocalResource {
    /// Four fixed UART receive buffers.
    UartRxBuffers,

    /// Queue containing available UART receive buffers.
    UartRxFreeQueue,

    /// Queue containing completed UART receive chunks.
    UartRxFilledQueue,
}

/// One logical resource slot or internally-created component resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentResourceKind {
    /// Hardware slot that every component instance must bind.
    ExternalHardware {
        /// Capability required from the bound board resource.
        capability: TaskResourceCapability,
    },

    /// Software slot that every component instance must bind.
    ExternalSoftware {
        /// Capability required from the bound application resource.
        capability: TaskResourceCapability,
    },

    /// Software resource created and initialized by component expansion.
    InternalSoftware {
        /// Generated software-resource type.
        resource: ComponentSoftwareResource,

        /// Whether consumers outside the instance may bind the resource.
        visibility: ComponentResourceVisibility,
    },

    /// Static init-local storage created by the selected backend.
    InitLocal(ComponentInitLocalResource),
}

/// Logical component resource and its configuration-dependent presence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentResource {
    /// Logical name referenced by component task bindings.
    pub id: &'static str,

    /// External, software, or init-local resource role.
    pub kind: ComponentResourceKind,

    /// Configuration condition controlling whether the resource is expanded.
    pub activation: ComponentActivation,
}

/// Maps a task-body field to a logical resource in its component definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentTaskBinding {
    /// Logical field declared by the reusable task definition.
    pub task_resource: &'static str,

    /// Logical resource declared by the component definition.
    pub component_resource: &'static str,
}

impl ComponentTaskBinding {
    /// Creates one logical task-to-component resource binding.
    pub const fn new(task_resource: &'static str, component_resource: &'static str) -> Self {
        Self {
            task_resource,
            component_resource,
        }
    }
}

/// Entry mechanism for a task contained in a reusable component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentTaskTrigger {
    /// Software-spawned asynchronous task.
    Spawned,

    /// Interrupt derived from one logical component hardware resource.
    Interrupt {
        /// Logical component resource that owns the interrupt.
        resource: &'static str,

        /// Interrupt role selected from the hardware endpoint.
        interrupt: HardwareInterrupt,
    },
}

/// Scheduling and bindings for one task contained in a component definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentTask {
    /// Reusable task definition and handwritten body.
    pub definition: TaskDefinition,

    /// Logical task name appended to the component instance ID.
    pub id: &'static str,

    /// RTIC priority of the generated task.
    pub priority: u8,

    /// Spawned or hardware-interrupt entry mechanism.
    pub trigger: ComponentTaskTrigger,

    /// Concrete generation-time values supplied to the reusable task body.
    pub parameters: &'static [TaskParameterBinding],

    /// Logical local-resource bindings.
    pub local_resources: &'static [ComponentTaskBinding],

    /// Logical shared-resource bindings.
    pub shared_resources: &'static [ComponentTaskBinding],

    /// Whether generated RTIC init spawns this task.
    pub init_spawn: bool,

    /// Configuration condition controlling whether the task is expanded.
    pub activation: ComponentActivation,
}

/// Reusable collection of tasks, resources, and one typed configuration family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentDefinition {
    /// Stable component-definition identifier.
    pub id: &'static str,

    /// Configuration family accepted by every instance.
    pub configuration_kind: ComponentConfigurationKind,

    /// Tasks expanded for each active instance.
    pub tasks: &'static [ComponentTask],

    /// External and internal resources expanded for each instance.
    pub resources: &'static [ComponentResource],
}

/// One target-specific component instance and its external bindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentDeclaration {
    /// Namespace prefix for all generated artifacts.
    pub id: &'static str,

    /// Reusable component definition being instantiated.
    pub definition: &'static ComponentDefinition,

    /// Typed startup configuration for this instance.
    pub configuration: ComponentConfiguration,

    /// Required external resource bindings.
    pub bindings: &'static [ResourceBinding],
}

/// Owned concrete target namespace used after component expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedResourceTarget {
    /// Board hardware resource identifier.
    Hardware(String),

    /// Application or component software-resource identifier.
    Software(String),
}

impl ExpandedResourceTarget {
    /// Returns the concrete generated resource identifier.
    pub fn id(&self) -> &str {
        match self {
            Self::Hardware(id) | Self::Software(id) => id,
        }
    }
}

/// Owned binding used by validation, resolution, and rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedResourceBinding {
    /// Logical task-body resource field.
    pub task_resource: String,

    /// Concrete board or software resource.
    pub target: ExpandedResourceTarget,
}

/// Owned entry mechanism used after component expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedTaskTrigger {
    /// Software-spawned task.
    Spawned,

    /// Hardware-interrupt task with a concrete board resource.
    Interrupt {
        /// Concrete board resource providing the interrupt.
        resource: String,

        /// Selected interrupt role.
        interrupt: HardwareInterrupt,
    },
}

/// Owned task declaration produced from standalone and component tasks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedTask {
    /// Concrete generated RTIC task identifier.
    pub id: String,

    /// Reusable definition and handwritten-body identity.
    pub definition: TaskDefinition,

    /// RTIC task priority.
    pub priority: u8,

    /// Concrete task entry mechanism.
    pub trigger: ExpandedTaskTrigger,

    /// Concrete generation-time values supplied to the reusable task body.
    pub parameters: Vec<TaskParameterBinding>,

    /// Concrete local-resource bindings.
    pub local_resources: Vec<ExpandedResourceBinding>,

    /// Concrete shared-resource bindings.
    pub shared_resources: Vec<ExpandedResourceBinding>,

    /// Owning component instance, or `None` for a standalone task.
    pub owner_component: Option<String>,
}

/// Software-resource type and initializer retained after expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpandedSoftwareResourceKind {
    /// Boolean initialized to the contained value.
    Bool(bool),

    /// Empty latest RC-input snapshot.
    RcInputSnapshot,

    /// Serial parser selected from the contained startup assignment.
    SerialConsumer(SerialPortAssignment),
}

impl ExpandedSoftwareResourceKind {
    /// Returns the task capability supplied by this software resource.
    pub const fn capability(self) -> TaskResourceCapability {
        match self {
            Self::Bool(_) => TaskResourceCapability::Bool,
            Self::RcInputSnapshot => TaskResourceCapability::RcInputSnapshot,
            Self::SerialConsumer(_) => TaskResourceCapability::SerialConsumer,
        }
    }

    /// Returns the concrete Rust type rendered in an RTIC resource struct.
    pub const fn rust_type(self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::RcInputSnapshot => "RcInputSnapshot",
            Self::SerialConsumer(_) => "SerialConsumer",
        }
    }

    /// Renders the deterministic init expression for this resource.
    pub fn initial_value(self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::RcInputSnapshot => "RcInputSnapshot::new()".to_owned(),
            Self::SerialConsumer(SerialPortAssignment::Disabled) => {
                "SerialConsumer::new(SerialPortAssignment::Disabled)".to_owned()
            }
            Self::SerialConsumer(SerialPortAssignment::Rc(RcProtocol::Sbus)) => {
                "SerialConsumer::new(SerialPortAssignment::Rc(RcProtocol::Sbus))".to_owned()
            }
            Self::SerialConsumer(SerialPortAssignment::ComPort) => {
                "SerialConsumer::new(SerialPortAssignment::ComPort)".to_owned()
            }
        }
    }
}

/// Owned software resource produced during component expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedSoftwareResource {
    /// Concrete generated RTIC resource identifier.
    pub id: String,

    /// Resource type and initializer.
    pub kind: ExpandedSoftwareResourceKind,

    /// Owning component, or `None` for an application resource.
    pub owner_component: Option<String>,

    /// Whether tasks outside the owner may bind this resource.
    pub exposed: bool,
}

/// Owned init-local resource produced during component expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedInitLocalResource {
    /// Concrete generated static-local identifier.
    pub id: String,

    /// Backend storage type requested by the component.
    pub kind: ComponentInitLocalResource,

    /// Owning component instance.
    pub owner_component: String,
}

/// Complete owned application produced before resource resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedApp {
    /// Standalone and component-generated tasks.
    pub tasks: Vec<ExpandedTask>,

    /// Software resources declared locally in RTIC ownership terms.
    pub software_local_resources: Vec<ExpandedSoftwareResource>,

    /// Software resources declared as RTIC shared state.
    pub software_shared_resources: Vec<ExpandedSoftwareResource>,

    /// Concrete task IDs spawned from RTIC init.
    pub init_spawned_task_ids: Vec<String>,

    /// Backend static storage requested by components.
    pub init_local_resources: Vec<ExpandedInitLocalResource>,
}

/// Expands every component instance into ordinary owned tasks and resources.
pub fn expand(board: &BoardDeclaration, app: &AppDeclaration) -> Result<ExpandedApp> {
    let mut expanded = ExpandedApp {
        tasks: app.tasks.iter().map(expand_standalone_task).collect(),
        software_local_resources: app
            .software_resources
            .local
            .iter()
            .map(expand_application_resource)
            .collect(),
        software_shared_resources: app
            .software_resources
            .shared
            .iter()
            .map(expand_application_resource)
            .collect(),
        init_spawned_task_ids: app
            .init
            .spawns
            .iter()
            .map(|task| task.id.to_owned())
            .collect(),
        init_local_resources: Vec::new(),
    };

    validate_component_instances(board, app)?;
    for declaration in app.components {
        expand_component(declaration, &mut expanded)?;
    }
    validate_expanded_ids(board, &expanded)?;
    validate_expanded_tasks(&expanded)?;
    Ok(expanded)
}

fn expand_standalone_task(task: &crate::task::TaskDeclaration) -> ExpandedTask {
    ExpandedTask {
        id: task.id.to_owned(),
        definition: task.definition,
        priority: task.priority,
        trigger: match task.trigger {
            TaskTrigger::Spawned => ExpandedTaskTrigger::Spawned,
            TaskTrigger::Interrupt {
                resource,
                interrupt,
            } => ExpandedTaskTrigger::Interrupt {
                resource: resource.to_owned(),
                interrupt,
            },
        },
        parameters: task.parameters.to_vec(),
        local_resources: task.local_resources.iter().map(expand_binding).collect(),
        shared_resources: task.shared_resources.iter().map(expand_binding).collect(),
        owner_component: None,
    }
}

fn expand_binding(binding: &ResourceBinding) -> ExpandedResourceBinding {
    ExpandedResourceBinding {
        task_resource: binding.task_resource().to_owned(),
        target: match binding.target() {
            ResourceTarget::Hardware(id) => ExpandedResourceTarget::Hardware(id.to_owned()),
            ResourceTarget::Software(id) => ExpandedResourceTarget::Software(id.to_owned()),
        },
    }
}

fn expand_application_resource(resource: &SoftwareResourceDeclaration) -> ExpandedSoftwareResource {
    let kind = match *resource {
        SoftwareResourceDeclaration::Bool { initial, .. } => {
            ExpandedSoftwareResourceKind::Bool(initial)
        }
    };
    ExpandedSoftwareResource {
        id: resource.id().to_owned(),
        kind,
        owner_component: None,
        exposed: true,
    }
}

fn expand_component(declaration: &ComponentDeclaration, expanded: &mut ExpandedApp) -> Result<()> {
    let resources = declaration
        .definition
        .resources
        .iter()
        .map(|resource| (resource.id, resource))
        .collect::<BTreeMap<_, _>>();
    let external_bindings = declaration
        .bindings
        .iter()
        .map(|binding| (binding.task_resource(), expand_binding(binding).target))
        .collect::<BTreeMap<_, _>>();

    for resource in declaration.definition.resources {
        if !resource.activation.active(declaration.configuration) {
            continue;
        }
        let id = namespaced(declaration.id, resource.id);
        match resource.kind {
            ComponentResourceKind::ExternalHardware { .. }
            | ComponentResourceKind::ExternalSoftware { .. } => {}
            ComponentResourceKind::InternalSoftware {
                resource,
                visibility,
            } => {
                let kind = match resource {
                    ComponentSoftwareResource::RcInputSnapshot => {
                        ExpandedSoftwareResourceKind::RcInputSnapshot
                    }
                    ComponentSoftwareResource::SerialConsumer => {
                        let ComponentConfiguration::SerialPort(assignment) =
                            declaration.configuration;
                        ExpandedSoftwareResourceKind::SerialConsumer(assignment)
                    }
                };
                let declaration = ExpandedSoftwareResource {
                    id,
                    kind,
                    owner_component: Some(declaration.id.to_owned()),
                    exposed: visibility == ComponentResourceVisibility::Exposed,
                };
                match resource {
                    ComponentSoftwareResource::SerialConsumer => {
                        expanded.software_local_resources.push(declaration)
                    }
                    ComponentSoftwareResource::RcInputSnapshot => {
                        expanded.software_shared_resources.push(declaration)
                    }
                }
            }
            ComponentResourceKind::InitLocal(kind) => {
                expanded
                    .init_local_resources
                    .push(ExpandedInitLocalResource {
                        id,
                        kind,
                        owner_component: declaration.id.to_owned(),
                    });
            }
        }
    }

    for task in declaration.definition.tasks {
        if !task.activation.active(declaration.configuration) {
            continue;
        }
        let task_id = namespaced(declaration.id, task.id);
        let resolve_bindings = |bindings: &[ComponentTaskBinding]| {
            bindings
                .iter()
                .map(|binding| {
                    let component_resource =
                        resources.get(binding.component_resource).ok_or_else(|| {
                            anyhow::anyhow!(
                                "component `{}` task `{}` binds unknown component resource `{}`",
                                declaration.id,
                                task.id,
                                binding.component_resource
                            )
                        })?;
                    if !component_resource
                        .activation
                        .active(declaration.configuration)
                    {
                        bail!(
                            "component `{}` task `{}` binds inactive resource `{}`",
                            declaration.id,
                            task.id,
                            binding.component_resource
                        );
                    }
                    let target = match component_resource.kind {
                        ComponentResourceKind::ExternalHardware { .. }
                        | ComponentResourceKind::ExternalSoftware { .. } => external_bindings
                            .get(binding.component_resource)
                            .cloned()
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "component `{}` is missing binding for `{}`",
                                    declaration.id,
                                    binding.component_resource
                                )
                            })?,
                        ComponentResourceKind::InternalSoftware { .. } => {
                            ExpandedResourceTarget::Software(namespaced(
                                declaration.id,
                                binding.component_resource,
                            ))
                        }
                        ComponentResourceKind::InitLocal(_) => {
                            bail!(
                                "component `{}` task `{}` cannot bind init-local resource `{}`",
                                declaration.id,
                                task.id,
                                binding.component_resource
                            )
                        }
                    };
                    Ok(ExpandedResourceBinding {
                        task_resource: binding.task_resource.to_owned(),
                        target,
                    })
                })
                .collect::<Result<Vec<_>>>()
        };
        let trigger = match task.trigger {
            ComponentTaskTrigger::Spawned => ExpandedTaskTrigger::Spawned,
            ComponentTaskTrigger::Interrupt {
                resource,
                interrupt,
            } => {
                external_bindings.get(resource).ok_or_else(|| {
                    anyhow::anyhow!(
                        "component `{}` interrupt task `{}` has no external binding for `{resource}`",
                        declaration.id,
                        task.id
                    )
                })?;
                let task_resource = task
                    .local_resources
                    .iter()
                    .chain(task.shared_resources)
                    .find(|binding| binding.component_resource == resource)
                    .map(|binding| binding.task_resource)
                    .ok_or_else(|| anyhow::anyhow!(
                        "component `{}` interrupt task `{}` does not bind trigger resource `{resource}`",
                        declaration.id,
                        task.id
                    ))?;
                ExpandedTaskTrigger::Interrupt {
                    resource: task_resource.to_owned(),
                    interrupt,
                }
            }
        };
        expanded.tasks.push(ExpandedTask {
            id: task_id.clone(),
            definition: task.definition,
            priority: task.priority,
            trigger,
            parameters: task.parameters.to_vec(),
            local_resources: resolve_bindings(task.local_resources)?,
            shared_resources: resolve_bindings(task.shared_resources)?,
            owner_component: Some(declaration.id.to_owned()),
        });
        if task.init_spawn {
            expanded.init_spawned_task_ids.push(task_id);
        }
    }
    Ok(())
}

fn validate_component_instances(board: &BoardDeclaration, app: &AppDeclaration) -> Result<()> {
    let declarations = app.components;
    let hardware = board
        .hardware
        .iter()
        .map(|resource| (resource.id(), resource))
        .collect::<BTreeMap<_, _>>();
    let mut software = BTreeMap::<String, (TaskResourceCapability, bool)>::new();
    for resource in app
        .software_resources
        .local
        .iter()
        .chain(app.software_resources.shared)
    {
        software.insert(
            resource.id().to_owned(),
            (TaskResourceCapability::Bool, true),
        );
    }
    for component in declarations {
        for resource in component.definition.resources {
            let ComponentResourceKind::InternalSoftware {
                resource: software_resource,
                visibility,
            } = resource.kind
            else {
                continue;
            };
            let capability = match software_resource {
                ComponentSoftwareResource::RcInputSnapshot => {
                    TaskResourceCapability::RcInputSnapshot
                }
                ComponentSoftwareResource::SerialConsumer => TaskResourceCapability::SerialConsumer,
            };
            software.insert(
                namespaced(component.id, resource.id),
                (
                    capability,
                    visibility == ComponentResourceVisibility::Exposed,
                ),
            );
        }
    }
    let mut instance_ids = BTreeSet::new();
    for declaration in declarations {
        validate_identifier(declaration.id, "component instance ID")?;
        validate_identifier(declaration.definition.id, "component definition ID")?;
        if !instance_ids.insert(declaration.id) {
            bail!(
                "application repeats component instance `{}`",
                declaration.id
            );
        }
        if declaration.configuration.kind() != declaration.definition.configuration_kind {
            bail!(
                "component `{}` configuration does not match definition `{}`",
                declaration.id,
                declaration.definition.id
            );
        }
        let mut resource_ids = BTreeSet::new();
        let mut external = BTreeMap::new();
        for resource in declaration.definition.resources {
            validate_identifier(resource.id, "component resource ID")?;
            if !resource_ids.insert(resource.id) {
                bail!(
                    "component definition `{}` repeats resource `{}`",
                    declaration.definition.id,
                    resource.id
                );
            }
            if matches!(
                resource.kind,
                ComponentResourceKind::ExternalHardware { .. }
                    | ComponentResourceKind::ExternalSoftware { .. }
            ) {
                external.insert(resource.id, resource);
            }
        }
        let mut bound = BTreeSet::new();
        for binding in declaration.bindings {
            let Some(resource) = external.get(binding.task_resource()) else {
                bail!(
                    "component `{}` has extra binding for `{}`",
                    declaration.id,
                    binding.task_resource()
                );
            };
            if !bound.insert(binding.task_resource()) {
                bail!(
                    "component `{}` binds `{}` more than once",
                    declaration.id,
                    binding.task_resource()
                );
            }
            match (resource.kind, binding.target()) {
                (
                    ComponentResourceKind::ExternalHardware { capability },
                    ResourceTarget::Hardware(id),
                ) => {
                    let hardware = hardware.get(id).ok_or_else(|| {
                        anyhow::anyhow!(
                            "component `{}` binds missing board resource `{id}`",
                            declaration.id
                        )
                    })?;
                    validate_hardware_capability(
                        declaration.id,
                        resource.id,
                        capability,
                        hardware,
                    )?;
                    if capability == TaskResourceCapability::UartRxDma {
                        let ComponentConfiguration::SerialPort(assignment) =
                            declaration.configuration;
                        let uart = hardware.uart_rx_dma().ok_or_else(|| {
                            anyhow::anyhow!(
                                "component `{}` endpoint `{id}` is not a UART RX DMA resource",
                                declaration.id
                            )
                        })?;
                        let protocol = assignment.profile().protocol;
                        if protocol != SerialProtocol::Disabled && !uart.supports_protocol(protocol)
                        {
                            bail!(
                                "component `{}` assignment {assignment:?} is not supported by endpoint `{id}`",
                                declaration.id
                            );
                        }
                    }
                }
                (
                    ComponentResourceKind::ExternalSoftware { capability },
                    ResourceTarget::Software(id),
                ) => {
                    let (actual, exposed) = software.get(id).copied().ok_or_else(|| {
                        anyhow::anyhow!(
                            "component `{}` binds missing software resource `{id}`",
                            declaration.id
                        )
                    })?;
                    if actual != capability {
                        bail!(
                            "component `{}` resource `{}` requires {capability:?}, but software resource `{id}` provides {actual:?}",
                            declaration.id,
                            resource.id
                        );
                    }
                    if !exposed {
                        bail!(
                            "component `{}` cannot bind private software resource `{id}`",
                            declaration.id
                        );
                    }
                }
                (ComponentResourceKind::ExternalHardware { .. }, ResourceTarget::Software(_)) => {
                    bail!(
                        "component `{}` resource `{}` requires a hardware binding",
                        declaration.id,
                        resource.id
                    )
                }
                (ComponentResourceKind::ExternalSoftware { .. }, ResourceTarget::Hardware(_)) => {
                    bail!(
                        "component `{}` resource `{}` requires a software binding",
                        declaration.id,
                        resource.id
                    )
                }
                _ => unreachable!("external map contains only external resources"),
            }
        }
        for id in external.keys() {
            if !bound.contains(id) {
                bail!(
                    "component `{}` is missing binding for `{id}`",
                    declaration.id
                );
            }
        }
    }
    Ok(())
}

fn validate_hardware_capability(
    component_id: &str,
    resource_id: &str,
    capability: TaskResourceCapability,
    hardware: &crate::hw_resources::HardwareResource,
) -> Result<()> {
    let compatible = match capability {
        TaskResourceCapability::DigitalOutput => matches!(
            hardware.gpio().map(|gpio| gpio.mode),
            Some(crate::hw_resources::GpioMode::Output { .. })
        ),
        TaskResourceCapability::InterruptInput => matches!(
            hardware.gpio().map(|gpio| gpio.mode),
            Some(crate::hw_resources::GpioMode::Input { interrupt: Some(_) })
        ),
        TaskResourceCapability::UartRxDma => hardware.uart_rx_dma().is_some(),
        TaskResourceCapability::Bool
        | TaskResourceCapability::RcInputSnapshot
        | TaskResourceCapability::SerialConsumer => false,
    };
    if !compatible {
        bail!(
            "component `{component_id}` resource `{resource_id}` requires {capability:?}, but board resource `{}` is incompatible",
            hardware.id()
        );
    }
    Ok(())
}

fn validate_expanded_ids(board: &BoardDeclaration, app: &ExpandedApp) -> Result<()> {
    let mut ids = BTreeMap::<&str, &str>::new();
    for hardware in board.hardware {
        insert_unique(&mut ids, hardware.id(), "board resource")?;
    }
    for resource in app
        .software_local_resources
        .iter()
        .chain(&app.software_shared_resources)
    {
        insert_unique(&mut ids, &resource.id, "software resource")?;
    }
    for resource in &app.init_local_resources {
        insert_unique(&mut ids, &resource.id, "init-local resource")?;
    }
    for task in &app.tasks {
        insert_unique(&mut ids, &task.id, "task")?;
    }
    Ok(())
}

fn insert_unique<'a>(
    ids: &mut BTreeMap<&'a str, &'static str>,
    id: &'a str,
    kind: &'static str,
) -> Result<()> {
    validate_identifier(id, kind)?;
    if let Some(existing) = ids.insert(id, kind) {
        bail!("generated identifier `{id}` collides between {existing} and {kind}");
    }
    Ok(())
}

fn validate_expanded_tasks(app: &ExpandedApp) -> Result<()> {
    let task_ids = app
        .tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut spawned = BTreeSet::new();
    for task in &app.tasks {
        crate::task::validate_expanded_declaration(task)?;
    }
    for task_id in &app.init_spawned_task_ids {
        if !task_ids.contains(task_id.as_str()) {
            bail!("init spawns `{task_id}` but it is not included in expanded tasks");
        }
        if !spawned.insert(task_id) {
            bail!("init spawns task `{task_id}` more than once");
        }
        let task = app.tasks.iter().find(|task| task.id == *task_id).unwrap();
        if !matches!(task.trigger, ExpandedTaskTrigger::Spawned) {
            bail!("init cannot spawn interrupt task `{task_id}`");
        }
    }
    Ok(())
}

fn namespaced(instance: &str, local: &str) -> String {
    format!("{instance}_{local}")
}

fn validate_identifier(id: &str, label: &str) -> Result<()> {
    syn::parse_str::<syn::Ident>(id)
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("{label} `{id}` is not a Rust identifier"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{InitDeclaration, SoftwareResourcesDeclaration},
        board::{BoardDeclaration, MonotonicDeclaration},
        hw_resources::{DmaChannel, HardwareResource, Mcu, PinId, SerialPortId, Target, UartRxDma},
        resolve,
        serial_port::SERIAL_PORT_COMPONENT,
        task::{
            TaskDeclaration, TaskDefinition, rc_input_snapshot, resource, serial_consumer_state,
        },
    };

    const UART2: HardwareResource = HardwareResource::UartRxDma(
        UartRxDma::new(
            "uart2_endpoint",
            SerialPortId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 5, 4),
        )
        .supports(&[SerialProtocol::Sbus, SerialProtocol::Raw]),
    );
    const UART3: HardwareResource = HardwareResource::UartRxDma(
        UartRxDma::new(
            "uart3_endpoint",
            SerialPortId::new(3),
            PinId::new(1, 11),
            DmaChannel::new(0, 1, 4),
        )
        .supports(&[SerialProtocol::Raw]),
    );
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "board",
        target: Target::internal_high_speed(Mcu::Stm32F401, 84_000_000),
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[UART2, UART3],
    };
    const F401_BOARD: BoardDeclaration = BoardDeclaration {
        hardware: &[UART2],
        ..BOARD
    };
    const UART2_COMPONENT: ComponentDeclaration = ComponentDeclaration {
        id: "uart2",
        definition: &SERIAL_PORT_COMPONENT,
        configuration: ComponentConfiguration::SerialPort(SerialPortAssignment::ComPort),
        bindings: &[resource("endpoint").to_hw("uart2_endpoint")],
    };

    fn app(
        tasks: &'static [TaskDeclaration],
        components: &'static [ComponentDeclaration],
    ) -> AppDeclaration {
        AppDeclaration {
            init: InitDeclaration::EMPTY,
            tasks,
            components,
            software_resources: SoftwareResourcesDeclaration::EMPTY,
        }
    }

    #[test]
    fn serial_component_expands_namespaced_tasks_resources_and_init_spawn() {
        let expanded = expand(&BOARD, &app(&[], &[UART2_COMPONENT])).unwrap();
        let task_ids = expanded
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(task_ids.contains("uart2_dma_irq"));
        assert!(task_ids.contains("uart2_idle_irq"));
        assert!(task_ids.contains("uart2_consumer"));
        assert!(
            expanded
                .init_spawned_task_ids
                .contains(&"uart2_consumer".to_owned())
        );
        assert!(
            expanded
                .software_local_resources
                .iter()
                .any(|resource| resource.id == "uart2_consumer_state")
        );
        assert!(
            expanded
                .software_shared_resources
                .iter()
                .any(|resource| resource.id == "uart2_rc_input" && resource.exposed)
        );
        assert!(
            expanded
                .init_local_resources
                .iter()
                .any(|resource| resource.id == "uart2_rx_buffers")
        );
    }

    #[test]
    fn two_instances_have_isolated_namespaces() {
        const UART3_COMPONENT: ComponentDeclaration = ComponentDeclaration {
            id: "uart3",
            definition: &SERIAL_PORT_COMPONENT,
            configuration: ComponentConfiguration::SerialPort(SerialPortAssignment::ComPort),
            bindings: &[resource("endpoint").to_hw("uart3_endpoint")],
        };
        let expanded = expand(&BOARD, &app(&[], &[UART2_COMPONENT, UART3_COMPONENT])).unwrap();
        assert!(
            expanded
                .tasks
                .iter()
                .any(|task| task.id == "uart2_consumer")
        );
        assert!(
            expanded
                .tasks
                .iter()
                .any(|task| task.id == "uart3_consumer")
        );
        assert!(
            expanded
                .software_shared_resources
                .iter()
                .any(|resource| resource.id == "uart2_rc_input")
        );
        assert!(
            expanded
                .software_shared_resources
                .iter()
                .any(|resource| resource.id == "uart3_rc_input")
        );
    }

    #[test]
    fn disabled_assignment_keeps_exposed_output_but_omits_uart_work() {
        const HEARTBEAT_DEF: TaskDefinition =
            TaskDefinition::asynchronous("heartbeat").with_shared(&[rc_input_snapshot("input")]);
        const HEARTBEAT: TaskDeclaration = HEARTBEAT_DEF
            .spawned_as("heartbeat")
            .priority(1)
            .with_shared(&[resource("input").to_sw("uart2_rc_input")]);
        const DISABLED: ComponentDeclaration = ComponentDeclaration {
            configuration: ComponentConfiguration::SerialPort(SerialPortAssignment::Disabled),
            ..UART2_COMPONENT
        };
        let expanded = expand(&F401_BOARD, &app(&[HEARTBEAT], &[DISABLED])).unwrap();
        assert_eq!(expanded.tasks.len(), 1);
        assert!(expanded.init_spawned_task_ids.is_empty());
        assert!(expanded.init_local_resources.is_empty());
        assert!(expanded.software_local_resources.is_empty());
        assert!(
            expanded
                .software_shared_resources
                .iter()
                .any(|resource| resource.id == "uart2_rc_input")
        );
        let resolved = resolve::resolve(&F401_BOARD, &expanded).unwrap();
        assert!(resolved.resources.is_empty());
        let board = crate::backend::validate(&F401_BOARD).unwrap();
        let rendered = crate::backend::render(&board, &resolved).unwrap();
        assert!(!rendered.initialization.contains("USART2"));
        assert!(!rendered.initialization.contains("DMA1"));
    }

    #[test]
    fn sbus_assignment_reaches_the_backend_as_an_sbus_profile() {
        const SBUS: ComponentDeclaration = ComponentDeclaration {
            configuration: ComponentConfiguration::SerialPort(SerialPortAssignment::Rc(
                RcProtocol::Sbus,
            )),
            ..UART2_COMPONENT
        };
        let expanded = expand(&F401_BOARD, &app(&[], &[SBUS])).unwrap();
        let resolved = resolve::resolve(&F401_BOARD, &expanded).unwrap();
        let board = crate::backend::validate(&F401_BOARD).unwrap();
        let rendered = crate::backend::render(&board, &resolved).unwrap();
        assert!(rendered.initialization.contains("SerialProtocol::Sbus"));
    }

    #[test]
    fn missing_duplicate_extra_and_wrong_namespace_bindings_fail() {
        const MISSING: ComponentDeclaration = ComponentDeclaration {
            bindings: &[],
            ..UART2_COMPONENT
        };
        const DUPLICATE: ComponentDeclaration = ComponentDeclaration {
            bindings: &[
                resource("endpoint").to_hw("uart2_endpoint"),
                resource("endpoint").to_hw("uart2_endpoint"),
            ],
            ..UART2_COMPONENT
        };
        const EXTRA: ComponentDeclaration = ComponentDeclaration {
            bindings: &[
                resource("endpoint").to_hw("uart2_endpoint"),
                resource("extra").to_hw("uart3_endpoint"),
            ],
            ..UART2_COMPONENT
        };
        const WRONG_NAMESPACE: ComponentDeclaration = ComponentDeclaration {
            bindings: &[resource("endpoint").to_sw("uart2_endpoint")],
            ..UART2_COMPONENT
        };
        assert!(expand(&BOARD, &app(&[], &[MISSING])).is_err());
        assert!(expand(&BOARD, &app(&[], &[DUPLICATE])).is_err());
        assert!(expand(&BOARD, &app(&[], &[EXTRA])).is_err());
        assert!(expand(&BOARD, &app(&[], &[WRONG_NAMESPACE])).is_err());
    }

    #[test]
    fn unsupported_assignment_fails_against_endpoint_capabilities() {
        const SBUS_ON_RAW_ONLY: ComponentDeclaration = ComponentDeclaration {
            id: "uart3",
            definition: &SERIAL_PORT_COMPONENT,
            configuration: ComponentConfiguration::SerialPort(SerialPortAssignment::Rc(
                RcProtocol::Sbus,
            )),
            bindings: &[resource("endpoint").to_hw("uart3_endpoint")],
        };
        let error = expand(&BOARD, &app(&[], &[SBUS_ON_RAW_ONLY])).unwrap_err();
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn generated_artifact_collision_with_standalone_task_fails() {
        const COLLIDING: TaskDeclaration = TaskDefinition::asynchronous("collision")
            .spawned_as("uart2_consumer")
            .priority(1);
        let error = expand(&BOARD, &app(&[COLLIDING], &[UART2_COMPONENT])).unwrap_err();
        assert!(error.to_string().contains("collides"));
    }

    #[test]
    fn exposed_output_is_bindable_but_private_state_is_not() {
        const HEARTBEAT_DEF: TaskDefinition =
            TaskDefinition::asynchronous("heartbeat").with_shared(&[rc_input_snapshot("input")]);
        const HEARTBEAT: TaskDeclaration = HEARTBEAT_DEF
            .spawned_as("heartbeat")
            .priority(1)
            .with_shared(&[resource("input").to_sw("uart2_rc_input")]);
        let expanded = expand(&BOARD, &app(&[HEARTBEAT], &[UART2_COMPONENT])).unwrap();
        resolve::resolve(&BOARD, &expanded).unwrap();

        const SNOOP_DEF: TaskDefinition =
            TaskDefinition::asynchronous("snoop").with_local(&[serial_consumer_state("state")]);
        const SNOOP: TaskDeclaration = SNOOP_DEF
            .spawned_as("snoop")
            .priority(1)
            .with_local(&[resource("state").to_sw("uart2_consumer_state")]);
        let expanded = expand(&BOARD, &app(&[SNOOP], &[UART2_COMPONENT])).unwrap();
        let error = resolve::resolve(&BOARD, &expanded).unwrap_err();
        assert!(error.to_string().contains("private resource"));
    }
}
