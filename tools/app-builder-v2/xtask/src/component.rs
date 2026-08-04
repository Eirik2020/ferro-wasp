//! Generic component declarations and deterministic expansion into app resources and tasks.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use ferrowasp_io_core::serial::SerialProtocol;

use crate::{
    app::{AppDeclaration, SoftwareResourceDeclaration},
    board::BoardDeclaration,
    task::{
        HardwareInterrupt, ResourceBinding, ResourceTarget, SOFTWARE_BOOL, TaskDefinition,
        TaskParameterBinding, TaskResourceCapability, TaskSafetyClass, TaskTrigger,
    },
};

/// Architectural layer occupied by a reusable component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentLayer {
    /// Owns physical hardware initialization and exposes protocol-neutral transport.
    HardwareEndpoint,

    /// Implements application behavior using endpoint or software interfaces.
    Functional,
}

/// Configuration family accepted by a reusable component definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentConfigurationKind {
    /// No target-specific component configuration.
    None,

    /// A receive-only serial-port electrical profile.
    SerialPort,
}

/// Typed configuration supplied to one component instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentConfiguration {
    /// Component needs no target-specific configuration.
    None,

    /// Electrical profile used to configure a serial endpoint.
    SerialPort(SerialProtocol),
}

impl ComponentConfiguration {
    const fn kind(self) -> ComponentConfigurationKind {
        match self {
            Self::None => ComponentConfigurationKind::None,
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
            (Self::SerialEnabled, ComponentConfiguration::SerialPort(protocol)) => {
                protocol != SerialProtocol::Disabled
            }
            (Self::SerialEnabled, ComponentConfiguration::None) => false,
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

/// Initialization strategy for a software resource created by a component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentSoftwareResourceInitializer {
    /// Initialize the resource with this target-independent Rust expression.
    Expression(&'static str),

    /// Receive the raw parser-side output produced by serial endpoint initialization.
    SerialRxOutput,

    /// Receive the publisher returned by splitting an init-local observer channel.
    ObserverPublisher {
        /// Logical component resource ID of the backing observer channel.
        channel: &'static str,
    },

    /// Receive the reader returned by splitting an init-local observer channel.
    ObserverReader {
        /// Logical component resource ID of the backing observer channel.
        channel: &'static str,
    },

    /// Receive the producer returned by splitting an init-local safety channel.
    SafetyProducer {
        /// Logical component resource ID of the backing safety channel.
        channel: &'static str,
    },

    /// Receive the consumer returned by splitting an init-local safety channel.
    SafetyConsumer {
        /// Logical component resource ID of the backing safety channel.
        channel: &'static str,
    },
}

/// RTIC ownership class for a component-created software resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentSoftwareResourceOwnership {
    /// Exactly one task owns the resource locally.
    Local,

    /// One or more tasks access the resource through RTIC locking.
    Shared,
}

/// Generic software-resource description created by a component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentSoftwareResource {
    /// Stable portable type ID matched against task resource requirements.
    pub type_id: &'static str,

    /// Concrete Rust type emitted in the generated RTIC resource struct.
    pub rust_type: &'static str,

    /// RTIC ownership class used when the component is expanded.
    pub ownership: ComponentSoftwareResourceOwnership,

    /// Deterministic initialization strategy for the generated value.
    pub initializer: ComponentSoftwareResourceInitializer,
}

impl ComponentSoftwareResource {
    /// Returns the portable capability supplied by this resource.
    pub const fn capability(self) -> TaskResourceCapability {
        match self.initializer {
            ComponentSoftwareResourceInitializer::Expression(_)
            | ComponentSoftwareResourceInitializer::SerialRxOutput => {
                TaskResourceCapability::Software(self.type_id)
            }
            ComponentSoftwareResourceInitializer::ObserverPublisher { .. } => {
                TaskResourceCapability::ObserverPublisher(self.type_id)
            }
            ComponentSoftwareResourceInitializer::ObserverReader { .. } => {
                TaskResourceCapability::ObserverReader(self.type_id)
            }
            ComponentSoftwareResourceInitializer::SafetyProducer { .. } => {
                TaskResourceCapability::SafetyProducer(self.type_id)
            }
            ComponentSoftwareResourceInitializer::SafetyConsumer { .. } => {
                TaskResourceCapability::SafetyConsumer(self.type_id)
            }
        }
    }
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

    /// Private storage for one non-consuming latest-value observer output.
    ObserverChannel {
        /// Stable portable type ID of the observed value.
        type_id: &'static str,

        /// Concrete Rust value type stored by the channel.
        rust_type: &'static str,

        /// Logical component resource ID of the sole publisher handle.
        publisher: &'static str,

        /// Logical component resource ID of the sole reader handle.
        reader: &'static str,
    },

    /// Private storage for one authoritative bounded SPSC channel.
    SafetyChannel {
        /// Stable portable type ID of transferred messages.
        type_id: &'static str,

        /// Concrete Rust message type stored by the channel.
        rust_type: &'static str,

        /// Backing heapless queue length; usable capacity is one less.
        queue_length: usize,

        /// Logical component resource ID of the sole producer handle.
        producer: &'static str,

        /// Logical component resource ID of the sole consumer handle.
        consumer: &'static str,
    },
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

    /// Safety role of the generated concrete task.
    pub safety_class: TaskSafetyClass,

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

    /// Hardware-endpoint or functional architectural role.
    pub layer: ComponentLayer,

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

    /// Safety role retained from target composition.
    pub safety_class: TaskSafetyClass,

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

    /// Generic component-owned software resource.
    Component(ComponentSoftwareResource),
}

impl ExpandedSoftwareResourceKind {
    /// Returns the task capability supplied by this software resource.
    pub const fn capability(self) -> TaskResourceCapability {
        match self {
            Self::Bool(_) => TaskResourceCapability::Software(SOFTWARE_BOOL),
            Self::Component(resource) => resource.capability(),
        }
    }

    /// Returns the concrete Rust type rendered in an RTIC resource struct.
    pub const fn rust_type(self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Component(resource) => resource.rust_type,
        }
    }

    /// Renders the deterministic init expression for this resource.
    pub fn initial_value(self, resource_id: &str) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Component(resource) => match resource.initializer {
                ComponentSoftwareResourceInitializer::Expression(expression) => {
                    expression.to_owned()
                }
                ComponentSoftwareResourceInitializer::SerialRxOutput => resource_id.to_owned(),
                ComponentSoftwareResourceInitializer::ObserverPublisher { .. }
                | ComponentSoftwareResourceInitializer::ObserverReader { .. }
                | ComponentSoftwareResourceInitializer::SafetyProducer { .. }
                | ComponentSoftwareResourceInitializer::SafetyConsumer { .. } => {
                    resource_id.to_owned()
                }
            },
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

    /// Owning component configuration retained for backend-produced outputs.
    pub owner_configuration: Option<ComponentConfiguration>,

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
        safety_class: task.safety_class,
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
        owner_configuration: None,
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
                let declaration = ExpandedSoftwareResource {
                    id,
                    kind: ExpandedSoftwareResourceKind::Component(resource),
                    owner_component: Some(declaration.id.to_owned()),
                    owner_configuration: Some(declaration.configuration),
                    exposed: visibility == ComponentResourceVisibility::Exposed,
                };
                match resource.ownership {
                    ComponentSoftwareResourceOwnership::Local => {
                        expanded.software_local_resources.push(declaration)
                    }
                    ComponentSoftwareResourceOwnership::Shared => {
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
            safety_class: task.safety_class,
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
            (TaskResourceCapability::Software(SOFTWARE_BOOL), true),
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
            let capability = software_resource.capability();
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
        if declaration.definition.layer == ComponentLayer::Functional
            && declaration.definition.resources.iter().any(|resource| {
                matches!(
                    resource.kind,
                    ComponentResourceKind::ExternalHardware { .. }
                )
            })
        {
            bail!(
                "functional component `{}` cannot bind board hardware directly",
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
        validate_observer_resources(declaration.definition)?;
        validate_safety_channels(declaration.definition)?;
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
                        let ComponentConfiguration::SerialPort(protocol) =
                            declaration.configuration
                        else {
                            bail!(
                                "serial endpoint component `{}` has no serial profile",
                                declaration.id
                            );
                        };
                        let uart = hardware.uart_rx_dma().ok_or_else(|| {
                            anyhow::anyhow!(
                                "component `{}` endpoint `{id}` is not a UART RX DMA resource",
                                declaration.id
                            )
                        })?;
                        if protocol != SerialProtocol::Disabled && !uart.supports_profile(protocol)
                        {
                            bail!(
                                "component `{}` serial profile {protocol:?} is not supported by endpoint `{id}`",
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
        TaskResourceCapability::Software(_)
        | TaskResourceCapability::ObserverPublisher(_)
        | TaskResourceCapability::ObserverReader(_)
        | TaskResourceCapability::SafetyProducer(_)
        | TaskResourceCapability::SafetyConsumer(_) => false,
    };
    if !compatible {
        bail!(
            "component `{component_id}` resource `{resource_id}` requires {capability:?}, but board resource `{}` is incompatible",
            hardware.id()
        );
    }
    Ok(())
}

fn validate_observer_resources(definition: &ComponentDefinition) -> Result<()> {
    let channels = definition
        .resources
        .iter()
        .filter_map(|resource| match resource.kind {
            ComponentResourceKind::InitLocal(ComponentInitLocalResource::ObserverChannel {
                type_id,
                rust_type,
                publisher,
                reader,
            }) => Some((resource.id, (type_id, rust_type, publisher, reader))),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut endpoints = BTreeMap::<&str, (usize, usize)>::new();

    for resource in definition.resources {
        let ComponentResourceKind::InternalSoftware {
            resource: software,
            visibility,
        } = resource.kind
        else {
            continue;
        };
        let (channel, publisher) = match software.initializer {
            ComponentSoftwareResourceInitializer::ObserverPublisher { channel } => (channel, true),
            ComponentSoftwareResourceInitializer::ObserverReader { channel } => (channel, false),
            ComponentSoftwareResourceInitializer::Expression(_)
            | ComponentSoftwareResourceInitializer::SerialRxOutput
            | ComponentSoftwareResourceInitializer::SafetyProducer { .. }
            | ComponentSoftwareResourceInitializer::SafetyConsumer { .. } => continue,
        };
        let (channel_type_id, value_type, publisher_id, reader_id) =
            channels.get(channel).copied().ok_or_else(|| {
            anyhow::anyhow!(
                "component definition `{}` observer endpoint `{}` references missing channel `{channel}`",
                definition.id,
                resource.id
            )
        })?;
        let expected_id = if publisher { publisher_id } else { reader_id };
        if resource.id != expected_id {
            bail!(
                "component definition `{}` observer channel `{channel}` names `{expected_id}` as its {} but endpoint `{}` uses that channel",
                definition.id,
                if publisher { "publisher" } else { "reader" },
                resource.id
            );
        }
        if software.type_id != channel_type_id {
            bail!(
                "component definition `{}` observer endpoint `{}` type `{}` does not match channel `{channel}` type `{channel_type_id}`",
                definition.id,
                resource.id,
                software.type_id
            );
        }
        let expected_type = if publisher {
            format!("ObserverPublisher<'static, {value_type}>")
        } else {
            format!("ObserverReader<'static, {value_type}>")
        };
        if software.rust_type != expected_type {
            bail!(
                "component definition `{}` observer endpoint `{}` must use Rust type `{expected_type}`",
                definition.id,
                resource.id
            );
        }
        let valid_ownership = if publisher {
            software.ownership == ComponentSoftwareResourceOwnership::Local
                && visibility == ComponentResourceVisibility::Private
        } else {
            software.ownership == ComponentSoftwareResourceOwnership::Shared
                && visibility == ComponentResourceVisibility::Exposed
        };
        if !valid_ownership {
            let role = if publisher { "publisher" } else { "reader" };
            bail!(
                "component definition `{}` observer {role} `{}` has invalid ownership or visibility",
                definition.id,
                resource.id
            );
        }
        let counts = endpoints.entry(channel).or_default();
        if publisher {
            counts.0 += 1;
        } else {
            counts.1 += 1;
        }
    }

    for channel in channels.keys() {
        let (publishers, readers) = endpoints.get(channel).copied().unwrap_or_default();
        if publishers != 1 || readers != 1 {
            bail!(
                "component definition `{}` observer channel `{channel}` requires exactly one publisher and one reader, found {publishers} publisher(s) and {readers} reader(s)",
                definition.id
            );
        }
    }
    Ok(())
}

fn validate_safety_channels(definition: &ComponentDefinition) -> Result<()> {
    let channels = definition
        .resources
        .iter()
        .filter_map(|resource| match resource.kind {
            ComponentResourceKind::InitLocal(ComponentInitLocalResource::SafetyChannel {
                type_id,
                rust_type,
                queue_length,
                producer,
                consumer,
            }) => Some((
                resource.id,
                (type_id, rust_type, queue_length, producer, consumer),
            )),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut endpoints = BTreeMap::<&str, (usize, usize)>::new();

    for (channel_id, (_, _, queue_length, _, _)) in &channels {
        if *queue_length < 2 {
            bail!(
                "component definition `{}` safety channel `{channel_id}` requires a queue length of at least two",
                definition.id
            );
        }
    }

    for resource in definition.resources {
        let ComponentResourceKind::InternalSoftware {
            resource: software,
            visibility,
        } = resource.kind
        else {
            continue;
        };
        let (channel, producer) = match software.initializer {
            ComponentSoftwareResourceInitializer::SafetyProducer { channel } => (channel, true),
            ComponentSoftwareResourceInitializer::SafetyConsumer { channel } => (channel, false),
            ComponentSoftwareResourceInitializer::Expression(_)
            | ComponentSoftwareResourceInitializer::SerialRxOutput
            | ComponentSoftwareResourceInitializer::ObserverPublisher { .. }
            | ComponentSoftwareResourceInitializer::ObserverReader { .. } => continue,
        };
        let (channel_type_id, value_type, _, producer_id, consumer_id) =
            channels.get(channel).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "component definition `{}` safety endpoint `{}` references missing channel `{channel}`",
                    definition.id,
                    resource.id
                )
            })?;
        let expected_id = if producer { producer_id } else { consumer_id };
        if resource.id != expected_id {
            bail!(
                "component definition `{}` safety channel `{channel}` names `{expected_id}` as its {} but endpoint `{}` uses that channel",
                definition.id,
                if producer { "producer" } else { "consumer" },
                resource.id
            );
        }
        if software.type_id != channel_type_id {
            bail!(
                "component definition `{}` safety endpoint `{}` type `{}` does not match channel `{channel}` type `{channel_type_id}`",
                definition.id,
                resource.id,
                software.type_id
            );
        }
        let expected_type = if producer {
            format!("SafetyProducer<'static, {value_type}>")
        } else {
            format!("SafetyConsumer<'static, {value_type}>")
        };
        if software.rust_type != expected_type {
            bail!(
                "component definition `{}` safety endpoint `{}` must use Rust type `{expected_type}`",
                definition.id,
                resource.id
            );
        }
        if software.ownership != ComponentSoftwareResourceOwnership::Local {
            bail!(
                "component definition `{}` safety {} `{}` must be task-local",
                definition.id,
                if producer { "producer" } else { "consumer" },
                resource.id
            );
        }
        let expected_visibility = if producer {
            ComponentResourceVisibility::Private
        } else {
            ComponentResourceVisibility::Exposed
        };
        if visibility != expected_visibility {
            bail!(
                "component definition `{}` safety {} `{}` has invalid visibility",
                definition.id,
                if producer { "producer" } else { "consumer" },
                resource.id
            );
        }
        let counts = endpoints.entry(channel).or_default();
        if producer {
            counts.0 += 1;
        } else {
            counts.1 += 1;
        }
    }

    for channel in channels.keys() {
        let (producers, consumers) = endpoints.get(channel).copied().unwrap_or_default();
        if producers != 1 || consumers != 1 {
            bail!(
                "component definition `{}` safety channel `{channel}` requires exactly one producer and one consumer, found {producers} producer(s) and {consumers} consumer(s)",
                definition.id
            );
        }
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
        components::{COMMAND_INPUT_COMPONENT, SERIAL_PORT_COMPONENT},
        hw_resources::{DmaChannel, HardwareResource, Mcu, PinId, SerialPortId, Target, UartRxDma},
        resolve,
        task::{
            SOFTWARE_MOTOR_CMD, SOFTWARE_RC_INPUT_SNAPSHOT, TaskDeclaration, TaskDefinition,
            TaskSafetyClass, rc_input_observer_publisher, rc_input_observer_reader, resource,
            safety_consumer, safety_producer, sbus_consumer_state, serial_rx,
        },
    };

    const UART2: HardwareResource = HardwareResource::UartRxDma(
        UartRxDma::new(
            "uart2",
            SerialPortId::new(2),
            PinId::new(0, 3),
            DmaChannel::new(0, 5, 4),
        )
        .supports(&[SerialProtocol::Sbus, SerialProtocol::Raw]),
    );
    const UART3: HardwareResource = HardwareResource::UartRxDma(
        UartRxDma::new(
            "uart3",
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
        configuration: ComponentConfiguration::SerialPort(SerialProtocol::Raw),
        bindings: &[resource("endpoint").to_hw("uart2")],
    };
    const COMMAND_INPUT: ComponentDeclaration = ComponentDeclaration {
        id: "command_input",
        definition: &COMMAND_INPUT_COMPONENT,
        configuration: ComponentConfiguration::None,
        bindings: &[resource("rx").to_sw("uart2_rx")],
    };
    const SAFETY_PRODUCER_TASK: TaskDefinition = TaskDefinition::asynchronous("safety_producer")
        .with_local(&[safety_producer("output", SOFTWARE_MOTOR_CMD)]);
    const SAFETY_CONSUMER_TASK: TaskDefinition = TaskDefinition::asynchronous("safety_consumer")
        .with_local(&[safety_consumer("input", SOFTWARE_MOTOR_CMD)]);
    const SAFETY_CHANNEL_COMPONENT: ComponentDefinition = ComponentDefinition {
        id: "safety_link",
        layer: ComponentLayer::Functional,
        configuration_kind: ComponentConfigurationKind::None,
        tasks: &[ComponentTask {
            definition: SAFETY_PRODUCER_TASK,
            safety_class: TaskSafetyClass::SafetyCritical,
            id: "producer_task",
            priority: 2,
            trigger: ComponentTaskTrigger::Spawned,
            parameters: &[],
            local_resources: &[ComponentTaskBinding::new("output", "producer")],
            shared_resources: &[],
            init_spawn: false,
            activation: ComponentActivation::Always,
        }],
        resources: &[
            ComponentResource {
                id: "channel",
                kind: ComponentResourceKind::InitLocal(ComponentInitLocalResource::SafetyChannel {
                    type_id: SOFTWARE_MOTOR_CMD,
                    rust_type: "MotorCmd",
                    queue_length: 4,
                    producer: "producer",
                    consumer: "consumer",
                }),
                activation: ComponentActivation::Always,
            },
            ComponentResource {
                id: "producer",
                kind: ComponentResourceKind::InternalSoftware {
                    resource: ComponentSoftwareResource {
                        type_id: SOFTWARE_MOTOR_CMD,
                        rust_type: "SafetyProducer<'static, MotorCmd>",
                        ownership: ComponentSoftwareResourceOwnership::Local,
                        initializer: ComponentSoftwareResourceInitializer::SafetyProducer {
                            channel: "channel",
                        },
                    },
                    visibility: ComponentResourceVisibility::Private,
                },
                activation: ComponentActivation::Always,
            },
            ComponentResource {
                id: "consumer",
                kind: ComponentResourceKind::InternalSoftware {
                    resource: ComponentSoftwareResource {
                        type_id: SOFTWARE_MOTOR_CMD,
                        rust_type: "SafetyConsumer<'static, MotorCmd>",
                        ownership: ComponentSoftwareResourceOwnership::Local,
                        initializer: ComponentSoftwareResourceInitializer::SafetyConsumer {
                            channel: "channel",
                        },
                    },
                    visibility: ComponentResourceVisibility::Exposed,
                },
                activation: ComponentActivation::Always,
            },
        ],
    };
    const SAFETY_CHANNEL: ComponentDeclaration = ComponentDeclaration {
        id: "control_to_actuator",
        definition: &SAFETY_CHANNEL_COMPONENT,
        configuration: ComponentConfiguration::None,
        bindings: &[],
    };
    const SAFETY_CONSUMER: TaskDeclaration = SAFETY_CONSUMER_TASK
        .spawned_as("actuator")
        .safety_class(TaskSafetyClass::SafetyCritical)
        .priority(2)
        .with_local(&[resource("input").to_sw("control_to_actuator_consumer")]);

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
    fn serial_endpoint_expands_transport_only_tasks_and_resources() {
        let expanded = expand(&BOARD, &app(&[], &[UART2_COMPONENT])).unwrap();
        let task_ids = expanded
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(task_ids.contains("uart2_dma_irq"));
        assert!(task_ids.contains("uart2_idle_irq"));
        assert!(!task_ids.contains("uart2_consumer"));
        assert!(expanded.init_spawned_task_ids.is_empty());
        assert!(expanded.software_local_resources.is_empty());
        assert!(
            expanded
                .software_shared_resources
                .iter()
                .any(|resource| resource.id == "uart2_rx" && resource.exposed)
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
            configuration: ComponentConfiguration::SerialPort(SerialProtocol::Raw),
            bindings: &[resource("endpoint").to_hw("uart3")],
        };
        let expanded = expand(&BOARD, &app(&[], &[UART2_COMPONENT, UART3_COMPONENT])).unwrap();
        assert!(expanded.tasks.iter().any(|task| task.id == "uart2_dma_irq"));
        assert!(expanded.tasks.iter().any(|task| task.id == "uart3_dma_irq"));
        assert!(
            expanded
                .software_shared_resources
                .iter()
                .any(|resource| resource.id == "uart2_rx")
        );
        assert!(
            expanded
                .software_shared_resources
                .iter()
                .any(|resource| resource.id == "uart3_rx")
        );
    }

    #[test]
    fn disabled_profile_omits_uart_work() {
        const DISABLED: ComponentDeclaration = ComponentDeclaration {
            configuration: ComponentConfiguration::SerialPort(SerialProtocol::Disabled),
            ..UART2_COMPONENT
        };
        let expanded = expand(&F401_BOARD, &app(&[], &[DISABLED])).unwrap();
        assert!(expanded.tasks.is_empty());
        assert!(expanded.init_spawned_task_ids.is_empty());
        assert!(expanded.init_local_resources.is_empty());
        assert!(expanded.software_local_resources.is_empty());
        assert!(expanded.software_shared_resources.is_empty());
        let resolved = resolve::resolve(&F401_BOARD, &expanded).unwrap();
        assert!(resolved.resources.is_empty());
        let board = crate::backend::validate(&F401_BOARD).unwrap();
        let rendered = crate::backend::render(&board, &resolved).unwrap();
        assert!(!rendered.initialization.contains("USART2"));
        assert!(!rendered.initialization.contains("DMA1"));
    }

    #[test]
    fn sbus_profile_reaches_the_backend_without_parser_ownership() {
        const SBUS: ComponentDeclaration = ComponentDeclaration {
            configuration: ComponentConfiguration::SerialPort(SerialProtocol::Sbus),
            ..UART2_COMPONENT
        };
        let expanded = expand(&F401_BOARD, &app(&[], &[SBUS, COMMAND_INPUT])).unwrap();
        let resolved = resolve::resolve(&F401_BOARD, &expanded).unwrap();
        let board = crate::backend::validate(&F401_BOARD).unwrap();
        let rendered = crate::backend::render(&board, &resolved).unwrap();
        assert!(rendered.initialization.contains("SerialProtocol::Sbus"));
    }

    #[test]
    fn command_input_observer_expands_to_private_publisher_shared_reader_and_storage() {
        const READER_DEFINITION: TaskDefinition = TaskDefinition::asynchronous("observer")
            .with_shared(&[rc_input_observer_reader("input")]);
        const READER: TaskDeclaration = READER_DEFINITION
            .spawned_as("observer")
            .priority(1)
            .with_shared(&[resource("input").to_sw("command_input_rc_input_reader")]);
        let expanded = expand(
            &F401_BOARD,
            &app(&[READER], &[UART2_COMPONENT, COMMAND_INPUT]),
        )
        .unwrap();

        let publisher = expanded
            .software_local_resources
            .iter()
            .find(|resource| resource.id == "command_input_rc_input_publisher")
            .unwrap();
        assert!(!publisher.exposed);
        assert_eq!(
            publisher.kind.capability(),
            TaskResourceCapability::ObserverPublisher(SOFTWARE_RC_INPUT_SNAPSHOT)
        );
        let reader = expanded
            .software_shared_resources
            .iter()
            .find(|resource| resource.id == "command_input_rc_input_reader")
            .unwrap();
        assert!(reader.exposed);
        assert_eq!(
            reader.kind.capability(),
            TaskResourceCapability::ObserverReader(SOFTWARE_RC_INPUT_SNAPSHOT)
        );
        assert!(expanded.init_local_resources.iter().any(|resource| {
            resource.id == "command_input_rc_input_channel"
                && matches!(
                    resource.kind,
                    ComponentInitLocalResource::ObserverChannel { .. }
                )
        }));

        let resolved = resolve::resolve(&F401_BOARD, &expanded).unwrap();
        assert_eq!(resolved.software_local_resources.len(), 2);
        assert!(resolved.software_shared_resources.iter().any(|resource| {
            resource.declaration.id == "command_input_rc_input_reader"
                && resource.task_ids == ["observer"]
        }));
    }

    #[test]
    fn safety_channel_expands_to_two_local_handles_and_private_init_storage() {
        let expanded = expand(&F401_BOARD, &app(&[SAFETY_CONSUMER], &[SAFETY_CHANNEL])).unwrap();
        assert!(expanded.software_shared_resources.is_empty());
        assert!(expanded.software_local_resources.iter().any(|resource| {
            resource.id == "control_to_actuator_producer"
                && !resource.exposed
                && resource.kind.capability()
                    == TaskResourceCapability::SafetyProducer(SOFTWARE_MOTOR_CMD)
        }));
        assert!(expanded.software_local_resources.iter().any(|resource| {
            resource.id == "control_to_actuator_consumer"
                && resource.exposed
                && resource.kind.capability()
                    == TaskResourceCapability::SafetyConsumer(SOFTWARE_MOTOR_CMD)
        }));
        assert!(expanded.init_local_resources.iter().any(|resource| {
            resource.id == "control_to_actuator_channel"
                && matches!(
                    resource.kind,
                    ComponentInitLocalResource::SafetyChannel {
                        queue_length: 4,
                        ..
                    }
                )
        }));

        let resolved = resolve::resolve(&F401_BOARD, &expanded).unwrap();
        assert_eq!(resolved.software_local_resources.len(), 2);
        assert!(resolved.software_shared_resources.is_empty());
        let board = crate::backend::validate(&F401_BOARD).unwrap();
        let rendered = crate::backend::render(&board, &resolved).unwrap();
        assert!(
            rendered
                .init_attribute
                .contains("SafetyChannel<MotorCmd, 4> = SafetyChannel::new()")
        );
        assert!(
            rendered
                .initialization
                .contains("cx.local.control_to_actuator_channel.split()")
        );
        assert!(
            rendered
                .local_struct
                .contains("control_to_actuator_producer: SafetyProducer<'static, MotorCmd>")
        );
        assert!(
            rendered
                .local_struct
                .contains("control_to_actuator_consumer: SafetyConsumer<'static, MotorCmd>")
        );
        assert!(
            rendered
                .prelude_exports
                .contains("safety_channel::{SafetyChannel, SafetyConsumer, SafetyProducer}")
        );
    }

    #[test]
    fn safety_channel_requires_both_handles_to_have_one_owner() {
        let expanded = expand(&BOARD, &app(&[], &[SAFETY_CHANNEL])).unwrap();
        let error = resolve::resolve(&BOARD, &expanded).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires exactly one owning task")
        );

        const SECOND_CONSUMER: TaskDeclaration = SAFETY_CONSUMER_TASK
            .spawned_as("backup_actuator")
            .safety_class(TaskSafetyClass::SafetyCritical)
            .priority(2)
            .with_local(&[resource("input").to_sw("control_to_actuator_consumer")]);
        let expanded = expand(
            &BOARD,
            &app(&[SAFETY_CONSUMER, SECOND_CONSUMER], &[SAFETY_CHANNEL]),
        )
        .unwrap();
        let error = resolve::resolve(&BOARD, &expanded).unwrap_err();
        assert!(error.to_string().contains("claimed locally by both"));
    }

    #[test]
    fn noncritical_task_cannot_own_a_safety_channel_handle() {
        const NONCRITICAL_CONSUMER: TaskDeclaration = SAFETY_CONSUMER_TASK
            .spawned_as("noncritical_consumer")
            .priority(2)
            .with_local(&[resource("input").to_sw("control_to_actuator_consumer")]);
        let expanded = expand(&BOARD, &app(&[NONCRITICAL_CONSUMER], &[SAFETY_CHANNEL])).unwrap();
        let error = resolve::resolve(&BOARD, &expanded).unwrap_err();
        assert!(error.to_string().contains("NonSafetyCritical"));
        assert!(error.to_string().contains("cannot own authoritative"));

        const SAFETY_RELATED_CONSUMER: TaskDeclaration = SAFETY_CONSUMER_TASK
            .spawned_as("safety_related_consumer")
            .safety_class(TaskSafetyClass::SafetyRelated)
            .priority(2)
            .with_local(&[resource("input").to_sw("control_to_actuator_consumer")]);
        let expanded = expand(&BOARD, &app(&[SAFETY_RELATED_CONSUMER], &[SAFETY_CHANNEL])).unwrap();
        let error = resolve::resolve(&BOARD, &expanded).unwrap_err();
        assert!(error.to_string().contains("SafetyRelated"));
    }

    #[test]
    fn safety_channel_handles_cannot_be_rtic_shared_resources() {
        const SHARED_CONSUMER: ComponentResource = ComponentResource {
            id: "consumer",
            kind: ComponentResourceKind::InternalSoftware {
                resource: ComponentSoftwareResource {
                    ownership: ComponentSoftwareResourceOwnership::Shared,
                    ..match SAFETY_CHANNEL_COMPONENT.resources[2].kind {
                        ComponentResourceKind::InternalSoftware { resource, .. } => resource,
                        _ => panic!("expected software resource"),
                    }
                },
                visibility: ComponentResourceVisibility::Exposed,
            },
            activation: ComponentActivation::Always,
        };
        const INVALID: ComponentDefinition = ComponentDefinition {
            resources: &[
                SAFETY_CHANNEL_COMPONENT.resources[0],
                SAFETY_CHANNEL_COMPONENT.resources[1],
                SHARED_CONSUMER,
            ],
            ..SAFETY_CHANNEL_COMPONENT
        };
        const DECLARATION: ComponentDeclaration = ComponentDeclaration {
            definition: &INVALID,
            ..SAFETY_CHANNEL
        };
        let error = expand(&BOARD, &app(&[], &[DECLARATION])).unwrap_err();
        assert!(error.to_string().contains("must be task-local"));
    }

    #[test]
    fn observer_channel_requires_exactly_one_valid_publisher_and_reader() {
        const INVALID: ComponentDefinition = ComponentDefinition {
            id: "invalid_observer",
            layer: ComponentLayer::Functional,
            configuration_kind: ComponentConfigurationKind::None,
            tasks: &[],
            resources: &[
                ComponentResource {
                    id: "snapshot_channel",
                    kind: ComponentResourceKind::InitLocal(
                        ComponentInitLocalResource::ObserverChannel {
                            type_id: SOFTWARE_RC_INPUT_SNAPSHOT,
                            rust_type: "RcInputSnapshot",
                            publisher: "snapshot_publisher",
                            reader: "snapshot_reader",
                        },
                    ),
                    activation: ComponentActivation::Always,
                },
                ComponentResource {
                    id: "snapshot_reader",
                    kind: ComponentResourceKind::InternalSoftware {
                        resource: ComponentSoftwareResource {
                            type_id: SOFTWARE_RC_INPUT_SNAPSHOT,
                            rust_type: "ObserverReader<'static, RcInputSnapshot>",
                            ownership: ComponentSoftwareResourceOwnership::Shared,
                            initializer: ComponentSoftwareResourceInitializer::ObserverReader {
                                channel: "snapshot_channel",
                            },
                        },
                        visibility: ComponentResourceVisibility::Exposed,
                    },
                    activation: ComponentActivation::Always,
                },
            ],
        };
        const DECLARATION: ComponentDeclaration = ComponentDeclaration {
            id: "invalid_observer",
            definition: &INVALID,
            configuration: ComponentConfiguration::None,
            bindings: &[],
        };

        let error = expand(&BOARD, &app(&[], &[DECLARATION])).unwrap_err();
        assert!(error.to_string().contains("exactly one publisher"));
    }

    #[test]
    fn observer_publisher_must_be_owned_by_a_producing_task() {
        const NO_PUBLISHER_TASK: ComponentDefinition = ComponentDefinition {
            id: "command_input_without_task",
            tasks: &[],
            ..COMMAND_INPUT_COMPONENT
        };
        const DECLARATION: ComponentDeclaration = ComponentDeclaration {
            id: "command_input",
            definition: &NO_PUBLISHER_TASK,
            configuration: ComponentConfiguration::None,
            bindings: &[resource("rx").to_sw("uart2_rx")],
        };
        let expanded = expand(&F401_BOARD, &app(&[], &[UART2_COMPONENT, DECLARATION])).unwrap();

        let error = resolve::resolve(&F401_BOARD, &expanded).unwrap_err();
        assert!(error.to_string().contains("not owned by a producing task"));
    }

    #[test]
    fn external_task_cannot_claim_private_observer_publisher() {
        const SNOOP_DEFINITION: TaskDefinition = TaskDefinition::asynchronous("snoop")
            .with_local(&[rc_input_observer_publisher("publisher")]);
        const SNOOP: TaskDeclaration = SNOOP_DEFINITION
            .spawned_as("snoop")
            .priority(1)
            .with_local(&[resource("publisher").to_sw("command_input_rc_input_publisher")]);
        let expanded = expand(
            &F401_BOARD,
            &app(&[SNOOP], &[UART2_COMPONENT, COMMAND_INPUT]),
        )
        .unwrap();
        let error = resolve::resolve(&F401_BOARD, &expanded).unwrap_err();
        assert!(error.to_string().contains("private resource"));
    }

    #[test]
    fn missing_duplicate_extra_and_wrong_namespace_bindings_fail() {
        const MISSING: ComponentDeclaration = ComponentDeclaration {
            bindings: &[],
            ..UART2_COMPONENT
        };
        const DUPLICATE: ComponentDeclaration = ComponentDeclaration {
            bindings: &[
                resource("endpoint").to_hw("uart2"),
                resource("endpoint").to_hw("uart2"),
            ],
            ..UART2_COMPONENT
        };
        const EXTRA: ComponentDeclaration = ComponentDeclaration {
            bindings: &[
                resource("endpoint").to_hw("uart2"),
                resource("extra").to_hw("uart3"),
            ],
            ..UART2_COMPONENT
        };
        const WRONG_NAMESPACE: ComponentDeclaration = ComponentDeclaration {
            bindings: &[resource("endpoint").to_sw("uart2")],
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
            configuration: ComponentConfiguration::SerialPort(SerialProtocol::Sbus),
            bindings: &[resource("endpoint").to_hw("uart3")],
        };
        let error = expand(&BOARD, &app(&[], &[SBUS_ON_RAW_ONLY])).unwrap_err();
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn generated_artifact_collision_with_standalone_task_fails() {
        const COLLIDING: TaskDeclaration = TaskDefinition::asynchronous("collision")
            .spawned_as("uart2_dma_irq")
            .priority(1);
        let error = expand(&BOARD, &app(&[COLLIDING], &[UART2_COMPONENT])).unwrap_err();
        assert!(error.to_string().contains("collides"));
    }

    #[test]
    fn exposed_endpoint_output_is_bindable_but_private_functional_state_is_not() {
        const READER_DEF: TaskDefinition =
            TaskDefinition::asynchronous("reader").with_shared(&[serial_rx("input")]);
        const READER: TaskDeclaration = READER_DEF
            .spawned_as("reader")
            .priority(1)
            .with_shared(&[resource("input").to_sw("uart2_rx")]);
        let expanded = expand(&BOARD, &app(&[READER], &[UART2_COMPONENT])).unwrap();
        resolve::resolve(&BOARD, &expanded).unwrap();

        const SNOOP_DEF: TaskDefinition =
            TaskDefinition::asynchronous("snoop").with_local(&[sbus_consumer_state("state")]);
        const SNOOP: TaskDeclaration = SNOOP_DEF
            .spawned_as("snoop")
            .priority(1)
            .with_local(&[resource("state").to_sw("command_input_decoder")]);
        let expanded = expand(&BOARD, &app(&[SNOOP], &[UART2_COMPONENT, COMMAND_INPUT])).unwrap();
        let error = resolve::resolve(&BOARD, &expanded).unwrap_err();
        assert!(error.to_string().contains("private resource"));
    }

    #[test]
    fn functional_components_cannot_bind_board_hardware() {
        const INVALID: ComponentDefinition = ComponentDefinition {
            id: "invalid_function",
            layer: ComponentLayer::Functional,
            configuration_kind: ComponentConfigurationKind::None,
            tasks: &[],
            resources: &[ComponentResource {
                id: "endpoint",
                kind: ComponentResourceKind::ExternalHardware {
                    capability: TaskResourceCapability::UartRxDma,
                },
                activation: ComponentActivation::Always,
            }],
        };
        const DECLARATION: ComponentDeclaration = ComponentDeclaration {
            id: "invalid",
            definition: &INVALID,
            configuration: ComponentConfiguration::None,
            bindings: &[resource("endpoint").to_hw("uart2")],
        };
        let error = expand(&BOARD, &app(&[], &[DECLARATION])).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot bind board hardware directly")
        );
    }
}
