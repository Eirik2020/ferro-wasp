//! Application-composition declarations for reusable RTIC-shaped tasks.

use std::{collections::BTreeSet, error::Error, fmt};

use stm32f4xx_hal::pac::Interrupt;

use super::{
    component::ComponentDeclaration,
    platform_config::{self, PlatformConfig, SerialPort, SpiPort, SpiService},
    safety_channel::SafetyChannelDeclaration,
    state::{self, TaskStateDeclaration},
    task::{
        ConfigBinding, LocalResourceBinding, SharedResourceBinding, SpawnBinding, TaskContract,
    },
    timing::{self, InitDelayDeclaration, MonotonicDeclaration},
};
use crate::backends::stm32f4::board_declaration::{self as board_definition, BoardDeclaration};
use crate::backends::stm32f4::dshot_actuator;
use crate::input_catalog::foxeer_golden_services as golden_services;

/// Initial value for an application-owned shared resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedValue {
    /// Boolean shared state.
    Bool(bool),

    /// Unsigned 32-bit shared state.
    U32(u32),
}

impl SharedValue {
    const fn rust_type(self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::U32(_) => "u32",
        }
    }
}

/// Declares one application-owned value available through RTIC shared locking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedResourceDeclaration {
    /// Concrete field name used by the generated application.
    pub id: &'static str,

    /// Initial value constructed by RTIC initialization.
    pub initial: SharedValue,
}

impl SharedResourceDeclaration {
    /// Declares a Boolean shared resource.
    pub const fn bool(id: &'static str, initial: bool) -> Self {
        Self {
            id,
            initial: SharedValue::Bool(initial),
        }
    }

    /// Declares an unsigned 32-bit shared resource.
    pub const fn u32(id: &'static str, initial: u32) -> Self {
        Self {
            id,
            initial: SharedValue::U32(initial),
        }
    }

    const fn rust_type(self) -> &'static str {
        self.initial.rust_type()
    }
}

/// Selects how a concrete task instance enters execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskTrigger {
    /// Hardware task bound directly to an STM32F4 interrupt vector.
    Interrupt(Interrupt),

    /// Software task entered through RTIC's generated spawn API.
    Software,
}

/// Safety role assigned to one concrete task instance.
///
/// Classification belongs to application composition, because the same
/// reusable body may have different authority in different applications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskSafetyClass {
    /// Task directly owns an authoritative safety-path channel handle.
    SafetyCritical,

    /// Task can influence safety behavior but owns no authoritative handle.
    SafetyRelated,

    /// Task has no authority over a safety path.
    NonSafetyCritical,
}

/// One concrete instance of a reusable task contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskDeclaration {
    /// Concrete task identifier used by the generated RTIC application.
    pub id: &'static str,

    /// Reusable task-local contract implemented by this instance.
    pub contract: &'static TaskContract,

    /// Interrupt or software-spawn entry mechanism.
    pub trigger: TaskTrigger,

    /// Safety authority assigned to this concrete task instance.
    pub safety_class: TaskSafetyClass,

    /// RTIC task priority. Zero is invalid.
    pub priority: u8,

    /// Concrete bindings for every `cx.local` contract slot.
    pub local: &'static [LocalResourceBinding],

    /// Concrete bindings for every `cx.shared` contract slot.
    pub shared: &'static [SharedResourceBinding],

    /// Concrete values for every `cx.config` contract slot.
    pub config: &'static [ConfigBinding],

    /// Concrete destinations for every logical spawn operation.
    pub spawns: &'static [SpawnBinding],
}

impl TaskDeclaration {
    /// Creates an interrupt-bound task with no priority or contract bindings.
    pub const fn interrupt(
        id: &'static str,
        contract: &'static TaskContract,
        interrupt: Interrupt,
    ) -> Self {
        Self::new(id, contract, TaskTrigger::Interrupt(interrupt))
    }

    /// Creates a spawn-triggered software task with no priority or bindings.
    pub const fn software(id: &'static str, contract: &'static TaskContract) -> Self {
        Self::new(id, contract, TaskTrigger::Software)
    }

    const fn new(id: &'static str, contract: &'static TaskContract, trigger: TaskTrigger) -> Self {
        Self {
            id,
            contract,
            trigger,
            safety_class: TaskSafetyClass::NonSafetyCritical,
            priority: 0,
            local: &[],
            shared: &[],
            config: &[],
            spawns: &[],
        }
    }

    /// Sets the RTIC scheduling priority.
    pub const fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Assigns this task's safety role explicitly.
    pub const fn safety_class(mut self, safety_class: TaskSafetyClass) -> Self {
        self.safety_class = safety_class;
        self
    }

    /// Marks this task as an owner in an authoritative safety path.
    pub const fn safety_critical(self) -> Self {
        self.safety_class(TaskSafetyClass::SafetyCritical)
    }

    /// Supplies every task-local resource binding.
    pub const fn with_local(mut self, bindings: &'static [LocalResourceBinding]) -> Self {
        self.local = bindings;
        self
    }

    /// Supplies every task-shared resource binding.
    pub const fn with_shared(mut self, bindings: &'static [SharedResourceBinding]) -> Self {
        self.shared = bindings;
        self
    }

    /// Supplies every compile-time task configuration value.
    pub const fn with_config(mut self, bindings: &'static [ConfigBinding]) -> Self {
        self.config = bindings;
        self
    }

    /// Supplies every logical task-spawn destination.
    pub const fn with_spawns(mut self, bindings: &'static [SpawnBinding]) -> Self {
        self.spawns = bindings;
        self
    }

    /// Declares that RTIC initialization spawns this task once.
    pub const fn init_spawn(self) -> InitSpawn {
        InitSpawn { task: self.id }
    }
}

/// One software task started by the generated RTIC `init` function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitSpawn {
    /// Concrete software-task identifier to spawn.
    pub task: &'static str,
}

/// Complete authoring declaration consumed by the RTIC generator.
#[derive(Clone, Copy, Debug)]
pub struct AppComposition {
    /// Board whose physical hardware may be consumed by components and tasks.
    pub board: &'static BoardDeclaration,

    /// Function used to load service-to-port assignments during boot.
    pub platform_config: fn() -> PlatformConfig,

    /// Monotonic timer used by asynchronous RTIC task bodies.
    pub monotonic: MonotonicDeclaration,

    /// Optional board timer available to synchronous initialization code.
    pub init_delay: Option<InitDelayDeclaration>,

    /// Reusable hardware components expanded before standalone task resolution.
    pub components: &'static [ComponentDeclaration],

    /// Versioned application-owned local state assigned to resolved tasks.
    pub task_state: &'static [TaskStateDeclaration],

    /// Application-owned values shared by standalone tasks.
    pub shared_resources: &'static [SharedResourceDeclaration],

    /// Exclusive bounded channels carrying authoritative safety-path data.
    pub safety_channels: &'static [SafetyChannelDeclaration],

    /// Interrupt-bound and software-spawned task instances.
    pub tasks: &'static [TaskDeclaration],

    /// Software tasks started once during RTIC initialization.
    pub init_spawns: &'static [InitSpawn],
}

/// Structural composition-validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionError {
    message: String,
}

impl CompositionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for CompositionError {}

/// Validates task-local contracts and their concrete composition bindings.
///
/// It validates identifier shape, source metadata, complete slot coverage,
/// hardware and shared-resource bindings, priorities, init spawns, logical
/// spawn destinations, unique interrupt ownership, and exclusive task-local
/// resource ownership.
pub fn validate(composition: &AppComposition) -> Result<(), CompositionError> {
    board_definition::validate(composition.board).map_err(CompositionError::new)?;
    let platform = (composition.platform_config)();
    platform_config::validate(platform).map_err(CompositionError::new)?;
    timing::validate(composition.monotonic).map_err(CompositionError::new)?;

    let monotonic_timer = composition
        .monotonic
        .hardware_id()
        .map(|hardware_id| {
            composition.board.timer(hardware_id).ok_or_else(|| {
                CompositionError::new(format!(
                    "monotonic consumes undeclared board timer `{hardware_id}`"
                ))
            })
        })
        .transpose()?;
    let init_delay_timer = composition
        .init_delay
        .map(|declaration| {
            timing::validate_init_delay(declaration).map_err(CompositionError::new)?;
            composition
                .board
                .timer(declaration.hardware_id)
                .ok_or_else(|| {
                    CompositionError::new(format!(
                        "initialization delay consumes undeclared board timer `{}`",
                        declaration.hardware_id
                    ))
                })
        })
        .transpose()?;
    if monotonic_timer
        .zip(init_delay_timer)
        .is_some_and(|(monotonic, delay)| monotonic.peripheral == delay.peripheral)
    {
        return Err(CompositionError::new(
            "the monotonic and initialization delay cannot consume the same timer peripheral",
        ));
    }
    let mut component_ids = BTreeSet::new();
    let mut component_hardware = BTreeSet::new();
    for component in composition.components {
        validate_identifier(component.id(), "component")?;
        if !component_ids.insert(component.id()) {
            return Err(CompositionError::new(format!(
                "component `{}` is declared more than once",
                component.id()
            )));
        }
        component
            .validate(composition.board)
            .map_err(CompositionError::new)?;
        if let ComponentDeclaration::PeriodicControl(timer) = component {
            let hardware = composition
                .board
                .timer(timer.hardware_id)
                .expect("component validation resolved this periodic timer");
            if monotonic_timer.is_some_and(|monotonic| monotonic.peripheral == hardware.peripheral)
            {
                return Err(CompositionError::new(format!(
                    "periodic control `{}` conflicts with the monotonic timer peripheral",
                    timer.id
                )));
            }
            if init_delay_timer.is_some_and(|delay| delay.peripheral == hardware.peripheral) {
                return Err(CompositionError::new(format!(
                    "periodic control `{}` conflicts with the initialization delay timer peripheral",
                    timer.id
                )));
            }
        }
        let hardware_id = component.hardware_id();
        if !component_hardware.insert(hardware_id) {
            return Err(CompositionError::new(format!(
                "board hardware `{}` is consumed by more than one component",
                hardware_id
            )));
        }
        if let ComponentDeclaration::DshotActuator(actuator) = component
            && !component_hardware.insert(actuator.telemetry_hardware_id)
        {
            return Err(CompositionError::new(format!(
                "board hardware `{}` is consumed by more than one component",
                actuator.telemetry_hardware_id
            )));
        }
        if let ComponentDeclaration::GoldenServices(services) = component {
            for hardware_id in [
                services.flash_hardware_id,
                services.usb_hardware_id,
                services.watchdog_hardware_id,
            ] {
                if !component_hardware.insert(hardware_id) {
                    return Err(CompositionError::new(format!(
                        "board hardware `{hardware_id}` is consumed by more than one component"
                    )));
                }
            }
        }
    }
    if composition
        .components
        .iter()
        .any(|component| matches!(component, ComponentDeclaration::ImuEndpoint(_)))
        && composition.init_delay.is_none()
    {
        return Err(CompositionError::new(
            "SPI IMU initialization requires a general initialization delay timer",
        ));
    }

    state::validate(composition.task_state).map_err(CompositionError::new)?;
    let mut state_ids = BTreeSet::new();
    for declaration in composition.task_state {
        validate_identifier(declaration.schema.id, "task-state schema")?;
        validate_identifier(declaration.owner_task, "task-state owner")?;
        for field in declaration.fields {
            validate_identifier(field.id, "task-local state")?;
            if !state_ids.insert(field.id) {
                return Err(CompositionError::new(format!(
                    "task-local state `{}` is declared more than once",
                    field.id
                )));
            }
            if composition.board.gpio(field.id).is_some()
                || composition.board.timer(field.id).is_some()
                || composition.board.serial(field.id).is_some()
                || composition.board.spi(field.id).is_some()
                || service_local_rust_type(platform, field.id).is_some()
                || service_shared_rust_type(platform, field.id).is_some()
                || component_local_rust_type(composition, field.id).is_some()
                || component_shared_rust_type(composition, field.id).is_some()
            {
                return Err(CompositionError::new(format!(
                    "task-local state `{}` collides with a declared hardware or service resource",
                    field.id
                )));
            }
        }
    }
    for assignment in platform.serial.into_iter().flatten() {
        let endpoint_id = match assignment.port {
            SerialPort::Serial1 => "serial1",
            SerialPort::Serial2 => "serial2",
        };
        if !composition.components.iter().any(|component| {
            matches!(component, ComponentDeclaration::SerialEndpoint(endpoint) if endpoint.id == endpoint_id)
        }) {
            return Err(CompositionError::new(format!(
                "platform serial port `{:?}` has no hardware endpoint declaration `{endpoint_id}`",
                assignment.port
            )));
        }
    }
    for assignment in platform.spi.into_iter().flatten() {
        let endpoint_id = match assignment.port {
            SpiPort::Spi1 => "spi1",
        };
        let Some(declaration) =
            composition
                .components
                .iter()
                .find_map(|component| match component {
                    ComponentDeclaration::ImuEndpoint(endpoint) if endpoint.id == endpoint_id => {
                        Some(endpoint)
                    }
                    ComponentDeclaration::ImuEndpoint(_)
                    | ComponentDeclaration::DshotActuator(_)
                    | ComponentDeclaration::GoldenServices(_)
                    | ComponentDeclaration::PeriodicControl(_)
                    | ComponentDeclaration::SerialEndpoint(_) => None,
                })
        else {
            return Err(CompositionError::new(format!(
                "platform SPI port `{:?}` has no hardware endpoint declaration `{endpoint_id}`",
                assignment.port
            )));
        };
        let hardware = composition
            .board
            .spi(declaration.hardware_id)
            .expect("component validation resolved this SPI endpoint");
        match assignment.service {
            SpiService::Imu(id) if hardware.imu(id).is_none() => {
                return Err(CompositionError::new(format!(
                    "platform SPI port `{:?}` selects undeclared IMU installation {} on endpoint `{endpoint_id}`",
                    assignment.port,
                    id.get()
                )));
            }
            SpiService::Imu(_) => {}
        }
    }

    let mut shared_ids = BTreeSet::new();
    for resource in composition.shared_resources {
        validate_identifier(resource.id, "shared resource")?;
        if state_ids.contains(resource.id) {
            return Err(CompositionError::new(format!(
                "shared resource `{}` collides with task-local state",
                resource.id
            )));
        }
        if !shared_ids.insert(resource.id) {
            return Err(CompositionError::new(format!(
                "shared resource `{}` is declared more than once",
                resource.id
            )));
        }
    }
    validate_safety_channel_declarations(composition, platform, &shared_ids, &state_ids)?;

    let mut task_ids = BTreeSet::new();
    let mut interrupts = Vec::new();
    let mut local_targets = BTreeSet::new();
    for task in composition.tasks {
        validate_identifier(task.id, "task")?;
        validate_identifier(task.contract.id, "task contract")?;
        validate_identifier(task.contract.source.function, "task source function")?;
        if task.contract.source.file.is_empty() {
            return Err(CompositionError::new(format!(
                "task contract `{}` has no source file",
                task.contract.id
            )));
        }
        if task.contract.source.function != task.contract.id {
            return Err(CompositionError::new(format!(
                "task contract `{}` source function is `{}`",
                task.contract.id, task.contract.source.function
            )));
        }
        if !task_ids.insert(task.id) {
            return Err(CompositionError::new(format!(
                "task `{}` is declared more than once",
                task.id
            )));
        }
        if task.priority == 0 {
            return Err(CompositionError::new(format!(
                "task `{}` must have a nonzero priority",
                task.id
            )));
        }
        if let TaskTrigger::Interrupt(interrupt) = task.trigger {
            if interrupts.contains(&interrupt) {
                return Err(CompositionError::new(format!(
                    "interrupt `{interrupt:?}` is bound more than once"
                )));
            }
            interrupts.push(interrupt);
        }

        validate_resource_bindings(task)?;
        validate_config_bindings(task)?;
        validate_spawn_bindings(task)?;

        for binding in task.local {
            validate_identifier(binding.target(), "hardware resource")?;
            let safety_type = safety_local_rust_type(composition, binding.target());
            let valid_safety_binding =
                safety_type.is_some_and(|expected| rust_types_match(expected, binding.rust_type()));
            let valid_component_binding = component_local_rust_type(composition, binding.target())
                .is_some_and(|expected| rust_types_match(expected, binding.rust_type()));
            let state_type = task_state_local_rust_type(composition, task.id, binding.target());
            let valid_state_binding =
                state_type.is_some_and(|expected| rust_types_match(expected, binding.rust_type()));
            if safety_type.is_some() && task.safety_class != TaskSafetyClass::SafetyCritical {
                return Err(CompositionError::new(format!(
                    "task `{}` is {:?} and cannot own authoritative safety-channel resource `{}`",
                    task.id,
                    task.safety_class,
                    binding.target()
                )));
            }
            if composition.board.gpio(binding.target()).is_none()
                && service_local_rust_type(platform, binding.target()) != Some(binding.rust_type())
                && !valid_component_binding
                && !valid_safety_binding
                && !valid_state_binding
            {
                return Err(CompositionError::new(format!(
                    "task `{}` binds local slot `{}` to missing or incompatible local resource `{}`",
                    task.id,
                    binding.logical(),
                    binding.target()
                )));
            }
            if !local_targets.insert(binding.target()) {
                return Err(CompositionError::new(format!(
                    "hardware resource `{}` is owned by more than one local task binding",
                    binding.target()
                )));
            }
        }

        for binding in task.shared {
            validate_identifier(binding.target(), "shared resource binding")?;
            if safety_local_rust_type(composition, binding.target()).is_some() {
                return Err(CompositionError::new(format!(
                    "task `{}` binds authoritative safety-channel resource `{}` as RTIC shared; safety handles must be task-local",
                    task.id,
                    binding.target()
                )));
            }
            let application_type = composition
                .shared_resources
                .iter()
                .find(|resource| resource.id == binding.target())
                .map(|resource| resource.rust_type());
            let actual_type = application_type
                .or_else(|| service_shared_rust_type(platform, binding.target()))
                .or_else(|| component_shared_rust_type(composition, binding.target()))
                .ok_or_else(|| {
                    CompositionError::new(format!(
                        "task `{}` binds `{}` to missing shared resource `{}`",
                        task.id,
                        binding.logical(),
                        binding.target()
                    ))
                })?;
            if actual_type != binding.rust_type() {
                return Err(CompositionError::new(format!(
                    "task `{}` shared slot `{}` requires `{}`, but `{}` is `{}`",
                    task.id,
                    binding.logical(),
                    binding.rust_type(),
                    binding.target(),
                    actual_type
                )));
            }
        }

        for binding in task.spawns {
            validate_identifier(binding.target(), "spawn target")?;
        }
    }

    for component in composition.components {
        let ComponentDeclaration::PeriodicControl(control) = component else {
            continue;
        };
        let task_id = format!("{}_loop", control.id);
        for binding in control.local {
            validate_identifier(binding.target(), "periodic-control local resource")?;
            let safety_type = safety_local_rust_type(composition, binding.target());
            let valid_safety_binding =
                safety_type.is_some_and(|expected| rust_types_match(expected, binding.rust_type()));
            let valid_component_binding = component_local_rust_type(composition, binding.target())
                .is_some_and(|expected| rust_types_match(expected, binding.rust_type()));
            if safety_type.is_some() && control.safety_class != TaskSafetyClass::SafetyCritical {
                return Err(CompositionError::new(format!(
                    "task `{task_id}` is {:?} and cannot own authoritative safety-channel resource `{}`",
                    control.safety_class,
                    binding.target()
                )));
            }
            if !valid_safety_binding && !valid_component_binding {
                return Err(CompositionError::new(format!(
                    "task `{task_id}` binds local slot `{}` to missing or incompatible periodic-control resource `{}`",
                    binding.logical(),
                    binding.target()
                )));
            }
            if !local_targets.insert(binding.target()) {
                return Err(CompositionError::new(format!(
                    "hardware resource `{}` is owned by more than one local task binding",
                    binding.target()
                )));
            }
        }
        for binding in control.shared {
            validate_identifier(binding.target(), "periodic-control shared resource")?;
            let Some(actual_type) = component_shared_rust_type(composition, binding.target())
                .or_else(|| {
                    composition
                        .shared_resources
                        .iter()
                        .find(|resource| resource.id == binding.target())
                        .map(|resource| resource.rust_type())
                })
            else {
                return Err(CompositionError::new(format!(
                    "task `{task_id}` binds shared slot `{}` to missing resource `{}`",
                    binding.logical(),
                    binding.target()
                )));
            };
            if !rust_types_match(actual_type, binding.rust_type()) {
                return Err(CompositionError::new(format!(
                    "task `{task_id}` shared slot `{}` requires `{}`, but `{}` is `{actual_type}`",
                    binding.logical(),
                    binding.rust_type(),
                    binding.target()
                )));
            }
        }
        for binding in control.spawns {
            validate_identifier(binding.target(), "spawn target")?;
            let destination = composition
                .tasks
                .iter()
                .find(|candidate| candidate.id == binding.target())
                .ok_or_else(|| {
                    CompositionError::new(format!(
                        "task `{task_id}` spawn slot `{}` targets undeclared task `{}`",
                        binding.logical(),
                        binding.target()
                    ))
                })?;
            if !matches!(destination.trigger, TaskTrigger::Software) {
                return Err(CompositionError::new(format!(
                    "task `{task_id}` cannot spawn interrupt task `{}`",
                    destination.id
                )));
            }
        }
    }

    validate_safety_channel_owners(composition)?;

    for task in composition.tasks {
        for binding in task.spawns {
            let destination = composition
                .tasks
                .iter()
                .find(|candidate| candidate.id == binding.target())
                .ok_or_else(|| {
                    CompositionError::new(format!(
                        "task `{}` spawn slot `{}` targets undeclared task `{}`",
                        task.id,
                        binding.logical(),
                        binding.target()
                    ))
                })?;
            if !matches!(destination.trigger, TaskTrigger::Software) {
                return Err(CompositionError::new(format!(
                    "task `{}` cannot spawn interrupt task `{}`",
                    task.id, destination.id
                )));
            }
        }
    }

    let mut initial_tasks = BTreeSet::new();
    for spawn in composition.init_spawns {
        validate_identifier(spawn.task, "init spawn")?;
        if !initial_tasks.insert(spawn.task) {
            return Err(CompositionError::new(format!(
                "init spawns task `{}` more than once",
                spawn.task
            )));
        }
        let task = composition
            .tasks
            .iter()
            .find(|task| task.id == spawn.task)
            .ok_or_else(|| {
                CompositionError::new(format!(
                    "init spawns `{}` but no such task is declared",
                    spawn.task
                ))
            })?;
        if !matches!(task.trigger, TaskTrigger::Software) {
            return Err(CompositionError::new(format!(
                "init cannot spawn interrupt task `{}`",
                spawn.task
            )));
        }
    }

    Ok(())
}

fn validate_safety_channel_declarations(
    composition: &AppComposition,
    platform: PlatformConfig,
    shared_ids: &BTreeSet<&str>,
    state_ids: &BTreeSet<&str>,
) -> Result<(), CompositionError> {
    let mut resource_ids = BTreeSet::new();
    for channel in composition.safety_channels {
        validate_identifier(channel.id, "safety channel")?;
        validate_identifier(channel.producer, "safety-channel producer")?;
        validate_identifier(channel.consumer, "safety-channel consumer")?;
        if channel.usable_capacity == 0 {
            return Err(CompositionError::new(format!(
                "safety channel `{}` requires at least one usable slot",
                channel.id
            )));
        }
        if channel.queue_length().is_none() {
            return Err(CompositionError::new(format!(
                "safety channel `{}` usable capacity overflows its backing queue length",
                channel.id
            )));
        }
        for (id, kind) in [
            (channel.id, "private storage"),
            (channel.producer, "producer"),
            (channel.consumer, "consumer"),
        ] {
            if !resource_ids.insert(id) {
                return Err(CompositionError::new(format!(
                    "safety resource `{id}` is declared more than once"
                )));
            }
            if shared_ids.contains(id)
                || state_ids.contains(id)
                || composition.board.gpio(id).is_some()
                || service_local_rust_type(platform, id).is_some()
                || service_shared_rust_type(platform, id).is_some()
                || component_local_rust_type(composition, id).is_some()
                || component_shared_rust_type(composition, id).is_some()
            {
                return Err(CompositionError::new(format!(
                    "safety channel `{}` {kind} `{id}` collides with another declared resource",
                    channel.id
                )));
            }
        }
    }
    Ok(())
}

fn validate_safety_channel_owners(composition: &AppComposition) -> Result<(), CompositionError> {
    for channel in composition.safety_channels {
        let producer_owners = local_resource_owners(composition, channel.producer);
        let consumer_owners = local_resource_owners(composition, channel.consumer);
        let [producer] = producer_owners.as_slice() else {
            return Err(CompositionError::new(format!(
                "safety channel `{}` requires exactly one producer owner, found {}",
                channel.id,
                producer_owners.len()
            )));
        };
        let [consumer] = consumer_owners.as_slice() else {
            return Err(CompositionError::new(format!(
                "safety channel `{}` requires exactly one consumer owner, found {}",
                channel.id,
                consumer_owners.len()
            )));
        };
        if producer == consumer {
            return Err(CompositionError::new(format!(
                "safety channel `{}` producer and consumer must be owned by different tasks",
                channel.id
            )));
        }
    }
    Ok(())
}

fn local_resource_owners(composition: &AppComposition, resource: &str) -> Vec<String> {
    let mut owners = composition
        .tasks
        .iter()
        .filter(|task| {
            task.local
                .iter()
                .any(|binding| binding.target() == resource)
        })
        .map(|task| task.id.to_owned())
        .collect::<Vec<_>>();
    owners.extend(composition.components.iter().filter_map(|component| {
        let ComponentDeclaration::PeriodicControl(control) = component else {
            return None;
        };
        control
            .local
            .iter()
            .any(|binding| binding.target() == resource)
            .then(|| format!("{}_loop", control.id))
    }));
    owners
}

fn safety_local_rust_type(composition: &AppComposition, target: &str) -> Option<&'static str> {
    composition.safety_channels.iter().find_map(|channel| {
        if target == channel.producer {
            Some(channel.message.producer_rust_type())
        } else if target == channel.consumer {
            Some(channel.message.consumer_rust_type())
        } else {
            None
        }
    })
}

fn task_state_local_rust_type(
    composition: &AppComposition,
    owner_task: &str,
    target: &str,
) -> Option<&'static str> {
    composition.task_state.iter().find_map(|declaration| {
        (declaration.owner_task == owner_task).then_some(())?;
        declaration
            .fields
            .iter()
            .find(|field| field.id == target)
            .map(|field| field.recipe.state_type().rust_type())
    })
}

fn rust_types_match(left: &str, right: &str) -> bool {
    left.chars()
        .filter(|character| !character.is_whitespace())
        .eq(right.chars().filter(|character| !character.is_whitespace()))
}

fn service_local_rust_type(platform: PlatformConfig, target: &str) -> Option<&'static str> {
    for assignment in platform.serial.into_iter().flatten() {
        if target == assignment.service.reader_resource() {
            return Some(stringify!(UartOwnedReader<'static>));
        }
        if target == assignment.service.discontinuities_resource() {
            return Some(stringify!(UartOwnedDiscontinuities<'static>));
        }
        if assignment.service.writer_resource() == Some(target) {
            return Some(stringify!(UartOwnedWriter<'static>));
        }
    }
    None
}

fn service_shared_rust_type(platform: PlatformConfig, target: &str) -> Option<&'static str> {
    for assignment in platform.spi.into_iter().flatten() {
        if target == assignment.service.sample_resource() {
            return Some(stringify!(ImuData));
        }
    }
    None
}

fn component_local_rust_type(composition: &AppComposition, target: &str) -> Option<&'static str> {
    composition
        .components
        .iter()
        .find_map(|component| match component {
            ComponentDeclaration::DshotActuator(_) => dshot_actuator::local_rust_type(target),
            ComponentDeclaration::GoldenServices(_) => golden_services::local_rust_type(target),
            ComponentDeclaration::ImuEndpoint(_)
            | ComponentDeclaration::PeriodicControl(_)
            | ComponentDeclaration::SerialEndpoint(_) => None,
        })
}

fn component_shared_rust_type(composition: &AppComposition, target: &str) -> Option<&'static str> {
    composition
        .components
        .iter()
        .find_map(|component| match component {
            ComponentDeclaration::DshotActuator(_) => dshot_actuator::shared_rust_type(target),
            ComponentDeclaration::GoldenServices(_) => golden_services::shared_rust_type(target),
            ComponentDeclaration::ImuEndpoint(endpoint) => {
                (target == format!("{}_owner", endpoint.id)).then_some("Spi1ImuEndpointOwner")
            }
            ComponentDeclaration::PeriodicControl(_) | ComponentDeclaration::SerialEndpoint(_) => {
                None
            }
        })
}

fn validate_resource_bindings(task: &TaskDeclaration) -> Result<(), CompositionError> {
    validate_slots(
        task.id,
        "local resource",
        task.contract.local.iter().map(|requirement| requirement.id),
        task.local.iter().map(|binding| binding.logical()),
    )?;
    validate_slots(
        task.id,
        "shared resource",
        task.contract
            .shared
            .iter()
            .map(|requirement| requirement.id),
        task.shared.iter().map(|binding| binding.logical()),
    )?;

    for binding in task.local {
        let requirement = task
            .contract
            .local
            .iter()
            .find(|requirement| requirement.id == binding.logical())
            .expect("slot coverage was validated");
        if requirement.rust_type != binding.rust_type() {
            return Err(CompositionError::new(format!(
                "task `{}` local slot `{}` has inconsistent capability metadata",
                task.id,
                binding.logical()
            )));
        }
    }

    for binding in task.shared {
        let requirement = task
            .contract
            .shared
            .iter()
            .find(|requirement| requirement.id == binding.logical())
            .expect("slot coverage was validated");
        if requirement.rust_type != binding.rust_type() {
            return Err(CompositionError::new(format!(
                "task `{}` shared slot `{}` has inconsistent type metadata",
                task.id,
                binding.logical()
            )));
        }
    }

    Ok(())
}

fn validate_config_bindings(task: &TaskDeclaration) -> Result<(), CompositionError> {
    validate_slots(
        task.id,
        "configuration value",
        task.contract
            .config
            .iter()
            .map(|requirement| requirement.id),
        task.config.iter().map(|binding| binding.logical()),
    )?;

    for binding in task.config {
        let requirement = task
            .contract
            .config
            .iter()
            .find(|requirement| requirement.id == binding.logical())
            .expect("slot coverage was validated");
        if requirement.rust_type != binding.expected_type()
            || requirement.rust_type != binding.value().rust_type()
        {
            return Err(CompositionError::new(format!(
                "task `{}` configuration `{}` requires `{}`, but composition supplies `{}`",
                task.id,
                binding.logical(),
                requirement.rust_type,
                binding.value().rust_type()
            )));
        }
    }

    Ok(())
}

fn validate_spawn_bindings(task: &TaskDeclaration) -> Result<(), CompositionError> {
    validate_slots(
        task.id,
        "spawn target",
        task.contract
            .spawns
            .iter()
            .map(|requirement| requirement.id),
        task.spawns.iter().map(|binding| binding.logical()),
    )?;

    for binding in task.spawns {
        let requirement = task
            .contract
            .spawns
            .iter()
            .find(|requirement| requirement.id == binding.logical())
            .expect("slot coverage was validated");
        if requirement.arguments != binding.arguments() {
            return Err(CompositionError::new(format!(
                "task `{}` spawn slot `{}` has inconsistent argument metadata",
                task.id,
                binding.logical()
            )));
        }
    }

    Ok(())
}

fn validate_slots<'a>(
    task_id: &str,
    kind: &str,
    expected: impl Iterator<Item = &'a str>,
    actual: impl Iterator<Item = &'a str>,
) -> Result<(), CompositionError> {
    let expected = expected.collect::<BTreeSet<_>>();
    let actual_values = actual.collect::<Vec<_>>();
    let actual = actual_values.iter().copied().collect::<BTreeSet<_>>();

    if actual.len() != actual_values.len() {
        return Err(CompositionError::new(format!(
            "task `{task_id}` repeats a {kind} binding"
        )));
    }
    if let Some(missing) = expected.difference(&actual).next() {
        return Err(CompositionError::new(format!(
            "task `{task_id}` does not bind {kind} `{missing}`"
        )));
    }
    if let Some(extra) = actual.difference(&expected).next() {
        return Err(CompositionError::new(format!(
            "task `{task_id}` binds undeclared {kind} `{extra}`"
        )));
    }
    Ok(())
}

fn validate_identifier(id: &str, kind: &str) -> Result<(), CompositionError> {
    let mut characters = id.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic());
    let valid_tail =
        characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    if !valid_start || !valid_tail {
        return Err(CompositionError::new(format!(
            "{kind} ID `{id}` is not a Rust identifier"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backends::stm32f4::{
            board_declaration::{
                BoardDeclaration, HardwareEndpointDeclaration, SerialHardwareDeclaration,
            },
            dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
            gpio::{GpioHardwareDeclaration, InterruptEdge, Level},
            mcu::{ClockDeclaration, Mcu, McuDeclaration},
            periodic_control::PERIODIC_CONTROL_TIMER,
            pins::{GpioPort, PinId},
            serial::{SerialPeripheral, SerialRoute},
            tasks as stm32f4_tasks,
        },
        input_catalog::foxeer_f405_v2::{
            app_composition::{
                APP_COMPOSITION, CONTROL_TIMER, SERIAL1_ENDPOINT, SERIAL2_ENDPOINT, SPI1_ENDPOINT,
            },
            board,
        },
        rtic::platform_config::{
            ImuInstallationId, PlatformConfig, SerialAssignment, SerialPort, SerialService,
            SpiAssignment, SpiPort, SpiService,
        },
        rtic::{
            safety_channel::SafetyChannelDeclaration,
            task::{LocalSlot, ResourceRequirement, SharedSlot, TaskSource},
        },
    };
    use ferrowasp_core::{
        safety::MotorCmd,
        safety_channel::{SafetyConsumer, SafetyProducer},
    };

    fn osd_platform_config() -> PlatformConfig {
        PlatformConfig::new(
            [
                Some(SerialAssignment::new(
                    SerialPort::Serial1,
                    SerialService::MspV1Osd,
                )),
                None,
            ],
            [None],
        )
    }

    fn imu_platform_config() -> PlatformConfig {
        PlatformConfig::new(
            [None, None],
            [Some(SpiAssignment::new(
                SpiPort::Spi1,
                SpiService::Imu(ImuInstallationId::new(1)),
            ))],
        )
    }

    fn unknown_imu_platform_config() -> PlatformConfig {
        PlatformConfig::new(
            [None, None],
            [Some(SpiAssignment::new(
                SpiPort::Spi1,
                SpiService::Imu(ImuInstallationId::new(2)),
            ))],
        )
    }

    const TEST_UART4: HardwareEndpointDeclaration = HardwareEndpointDeclaration::serial(
        SerialHardwareDeclaration::new("uart4", SerialPeripheral::Uart4)
            .rx(SerialRoute::dma(
                PinId::new(GpioPort::A, 1),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream2,
                    DmaChannel::Channel4,
                ),
            ))
            .tx(SerialRoute::dma(
                PinId::new(GpioPort::A, 0),
                DmaRoute::new(
                    DmaController::Dma1,
                    DmaStream::Stream4,
                    DmaChannel::Channel4,
                ),
            )),
    );
    const TEST_BOARD: BoardDeclaration = BoardDeclaration::new(
        "composition_test_board",
        McuDeclaration::new(Mcu::Stm32f405, ClockDeclaration::hsi(16_000_000)),
        &[TEST_UART4],
    )
    .with_gpio(&[
        GpioHardwareDeclaration::output("led2", PinId::new(GpioPort::A, 5), Level::Low),
        GpioHardwareDeclaration::input("user_button", PinId::new(GpioPort::C, 13))
            .interrupt_on(InterruptEdge::Falling),
    ]);
    const TEST_ENABLED: SharedResourceDeclaration =
        SharedResourceDeclaration::bool("button_enabled", false);
    const TEST_OBSERVER: TaskDeclaration = TaskDeclaration::software(
        "observe_button_change",
        &crate::input_catalog::selected_tasks::observe_button_change::CONTRACT,
    )
    .priority(1);
    const TEST_BUTTON: TaskDeclaration = TaskDeclaration::interrupt(
        "button_exti",
        &stm32f4_tasks::button_exti::CONTRACT,
        Interrupt::EXTI15_10,
    )
    .priority(2)
    .with_local(&[stm32f4_tasks::button_exti::LOCAL.button.bind("user_button")])
    .with_shared(&[stm32f4_tasks::button_exti::SHARED
        .enabled
        .bind("button_enabled")])
    .with_config(&[stm32f4_tasks::button_exti::CONFIG.toggle_on_press.set(true)])
    .with_spawns(&[stm32f4_tasks::button_exti::SPAWNS
        .button_changed
        .bind("observe_button_change")]);

    const MOTOR_PRODUCER_REQUIREMENTS: &[ResourceRequirement] = &[ResourceRequirement::new(
        "commands",
        "SafetyProducer<'static, MotorCmd>",
    )];
    const MOTOR_CONSUMER_REQUIREMENTS: &[ResourceRequirement] = &[ResourceRequirement::new(
        "commands",
        "SafetyConsumer<'static, MotorCmd>",
    )];
    const MOTOR_PRODUCER_CONTRACT: TaskContract = TaskContract {
        id: "control_producer",
        source: TaskSource::new("test_safety_tasks.rs", "control_producer"),
        local: MOTOR_PRODUCER_REQUIREMENTS,
        shared: &[],
        config: &[],
        spawns: &[],
    };
    const MOTOR_CONSUMER_CONTRACT: TaskContract = TaskContract {
        id: "actuator_consumer",
        source: TaskSource::new("test_safety_tasks.rs", "actuator_consumer"),
        local: MOTOR_CONSUMER_REQUIREMENTS,
        shared: &[],
        config: &[],
        spawns: &[],
    };
    const MOTOR_PRODUCER_SLOT: LocalSlot<SafetyProducer<'static, MotorCmd>> =
        LocalSlot::new("commands", "SafetyProducer<'static, MotorCmd>");
    const MOTOR_CONSUMER_SLOT: LocalSlot<SafetyConsumer<'static, MotorCmd>> =
        LocalSlot::new("commands", "SafetyConsumer<'static, MotorCmd>");
    const WRONG_MOTOR_PRODUCER_REQUIREMENTS: &[ResourceRequirement] = &[ResourceRequirement::new(
        "commands",
        "SafetyProducer<'static, u32>",
    )];
    const WRONG_MOTOR_PRODUCER_CONTRACT: TaskContract = TaskContract {
        id: "wrong_control_producer",
        source: TaskSource::new("test_safety_tasks.rs", "wrong_control_producer"),
        local: WRONG_MOTOR_PRODUCER_REQUIREMENTS,
        shared: &[],
        config: &[],
        spawns: &[],
    };
    const WRONG_MOTOR_PRODUCER_SLOT: LocalSlot<SafetyProducer<'static, u32>> =
        LocalSlot::new("commands", "SafetyProducer<'static, u32>");
    const SHARED_MOTOR_PRODUCER_REQUIREMENTS: &[ResourceRequirement] = &[ResourceRequirement::new(
        "commands",
        "SafetyProducer<'static, MotorCmd>",
    )];
    const SHARED_MOTOR_PRODUCER_CONTRACT: TaskContract = TaskContract {
        id: "shared_control_producer",
        source: TaskSource::new("test_safety_tasks.rs", "shared_control_producer"),
        local: &[],
        shared: SHARED_MOTOR_PRODUCER_REQUIREMENTS,
        config: &[],
        spawns: &[],
    };
    const SHARED_MOTOR_PRODUCER_SLOT: SharedSlot<SafetyProducer<'static, MotorCmd>> =
        SharedSlot::new("commands", "SafetyProducer<'static, MotorCmd>");
    const CONTROL_TO_ACTUATOR: SafetyChannelDeclaration = SafetyChannelDeclaration::motor_commands(
        "control_to_actuator",
        3,
        "motor_cmd_producer",
        "motor_cmd_consumer",
    );
    const TEST_SAFETY_PRODUCER: TaskDeclaration =
        TaskDeclaration::software("control_producer", &MOTOR_PRODUCER_CONTRACT)
            .priority(2)
            .safety_critical()
            .with_local(&[MOTOR_PRODUCER_SLOT.bind("motor_cmd_producer")]);
    const TEST_SAFETY_CONSUMER: TaskDeclaration =
        TaskDeclaration::software("actuator_consumer", &MOTOR_CONSUMER_CONTRACT)
            .priority(3)
            .safety_critical()
            .with_local(&[MOTOR_CONSUMER_SLOT.bind("motor_cmd_consumer")]);

    const fn task_test_app(tasks: &'static [TaskDeclaration]) -> AppComposition {
        AppComposition {
            board: &TEST_BOARD,
            platform_config: super::platform_config::empty_platform_config,
            monotonic: MonotonicDeclaration::systick(1_000),
            init_delay: None,
            components: &[],
            task_state: &[],
            shared_resources: &[TEST_ENABLED],
            safety_channels: &[],
            tasks,
            init_spawns: &[],
        }
    }

    const SAFETY_TEST_APP: AppComposition = AppComposition {
        board: &TEST_BOARD,
        platform_config: super::platform_config::empty_platform_config,
        monotonic: MonotonicDeclaration::systick(1_000),
        init_delay: None,
        components: &[],
        task_state: &[],
        shared_resources: &[],
        safety_channels: &[CONTROL_TO_ACTUATOR],
        tasks: &[TEST_SAFETY_PRODUCER, TEST_SAFETY_CONSUMER],
        init_spawns: &[],
    };

    #[test]
    fn complete_example_satisfies_all_task_contracts() {
        validate(&APP_COMPOSITION).unwrap();
    }

    #[test]
    fn duplicate_interrupt_binding_is_rejected() {
        const DUPLICATE_BUTTON: TaskDeclaration = TaskDeclaration::interrupt(
            "backup_button_exti",
            &stm32f4_tasks::button_exti::CONTRACT,
            Interrupt::EXTI15_10,
        )
        .priority(2);
        const INVALID: AppComposition = AppComposition {
            tasks: &[TEST_BUTTON, DUPLICATE_BUTTON],
            ..task_test_app(&[])
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(error, "interrupt `EXTI15_10` is bound more than once");
    }

    #[test]
    fn safety_channel_requires_two_critical_task_local_owners() {
        validate(&SAFETY_TEST_APP).unwrap();

        let resolved = crate::rtic::resolve::resolve(&SAFETY_TEST_APP).unwrap();
        assert_eq!(resolved.safety_channels.len(), 1);
        assert_eq!(
            resolved.safety_channels[0].producer.owner_task,
            "control_producer"
        );
        assert_eq!(
            resolved.safety_channels[0].consumer.owner_task,
            "actuator_consumer"
        );
    }

    #[test]
    fn safety_channel_rejects_zero_usable_capacity() {
        const EMPTY_CHANNEL: SafetyChannelDeclaration = SafetyChannelDeclaration::motor_commands(
            "control_to_actuator",
            0,
            "motor_cmd_producer",
            "motor_cmd_consumer",
        );
        const INVALID: AppComposition = AppComposition {
            safety_channels: &[EMPTY_CHANNEL],
            ..SAFETY_TEST_APP
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "safety channel `control_to_actuator` requires at least one usable slot"
        );
    }

    #[test]
    fn safety_channel_rejects_a_missing_consumer_owner() {
        const INVALID: AppComposition = AppComposition {
            tasks: &[TEST_SAFETY_PRODUCER],
            ..SAFETY_TEST_APP
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "safety channel `control_to_actuator` requires exactly one consumer owner, found 0"
        );
    }

    #[test]
    fn noncritical_task_cannot_own_a_safety_handle() {
        const NONCRITICAL_PRODUCER: TaskDeclaration =
            TEST_SAFETY_PRODUCER.safety_class(TaskSafetyClass::NonSafetyCritical);
        const INVALID: AppComposition = AppComposition {
            tasks: &[NONCRITICAL_PRODUCER, TEST_SAFETY_CONSUMER],
            ..SAFETY_TEST_APP
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("cannot own authoritative safety-channel resource"));
    }

    #[test]
    fn safety_handle_cannot_be_bound_as_shared() {
        const SHARED_PRODUCER: TaskDeclaration =
            TaskDeclaration::software("shared_control_producer", &SHARED_MOTOR_PRODUCER_CONTRACT)
                .priority(2)
                .safety_critical()
                .with_shared(&[SHARED_MOTOR_PRODUCER_SLOT.bind("motor_cmd_producer")]);
        const INVALID: AppComposition = AppComposition {
            tasks: &[SHARED_PRODUCER, TEST_SAFETY_CONSUMER],
            ..SAFETY_TEST_APP
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("safety handles must be task-local"));
    }

    #[test]
    fn safety_channel_rejects_duplicated_handle_ownership() {
        const SECOND_PRODUCER: TaskDeclaration =
            TaskDeclaration::software("backup_control_producer", &MOTOR_PRODUCER_CONTRACT)
                .priority(2)
                .safety_critical()
                .with_local(&[MOTOR_PRODUCER_SLOT.bind("motor_cmd_producer")]);
        const INVALID: AppComposition = AppComposition {
            tasks: &[TEST_SAFETY_PRODUCER, SECOND_PRODUCER, TEST_SAFETY_CONSUMER],
            ..SAFETY_TEST_APP
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "hardware resource `motor_cmd_producer` is owned by more than one local task binding"
        );
    }

    #[test]
    fn safety_channel_rejects_type_incompatible_handle() {
        const WRONG_PRODUCER: TaskDeclaration =
            TaskDeclaration::software("wrong_control_producer", &WRONG_MOTOR_PRODUCER_CONTRACT)
                .priority(2)
                .safety_critical()
                .with_local(&[WRONG_MOTOR_PRODUCER_SLOT.bind("motor_cmd_producer")]);
        const INVALID: AppComposition = AppComposition {
            tasks: &[WRONG_PRODUCER, TEST_SAFETY_CONSUMER],
            ..SAFETY_TEST_APP
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("missing or incompatible local resource `motor_cmd_producer`"));
    }

    #[test]
    fn missing_task_configuration_is_rejected() {
        const INVALID_TASK: TaskDeclaration = TaskDeclaration::interrupt(
            "button_exti",
            &stm32f4_tasks::button_exti::CONTRACT,
            Interrupt::EXTI15_10,
        )
        .priority(2)
        .with_local(&[stm32f4_tasks::button_exti::LOCAL.button.bind("user_button")])
        .with_shared(&[stm32f4_tasks::button_exti::SHARED
            .enabled
            .bind("button_enabled")])
        .with_spawns(&[stm32f4_tasks::button_exti::SPAWNS
            .button_changed
            .bind("observe_button_change")]);
        const INVALID: AppComposition = AppComposition {
            tasks: &[INVALID_TASK, TEST_OBSERVER],
            ..task_test_app(&[])
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("does not bind configuration value `toggle_on_press`"));
    }

    #[test]
    fn init_cannot_spawn_an_interrupt_task() {
        const INVALID: AppComposition = AppComposition {
            tasks: &[TEST_BUTTON, TEST_OBSERVER],
            init_spawns: &[TEST_BUTTON.init_spawn()],
            ..task_test_app(&[])
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("init cannot spawn interrupt task"));
    }

    #[test]
    fn serial_endpoint_requires_hardware_declared_by_the_selected_board() {
        const BOARD_WITHOUT_UART4: BoardDeclaration = BoardDeclaration::new(
            "empty_board",
            McuDeclaration::new(Mcu::Stm32f405, ClockDeclaration::hsi(16_000_000)),
            &[],
        );
        const INVALID: AppComposition = AppComposition {
            board: &BOARD_WITHOUT_UART4,
            platform_config: osd_platform_config,
            components: &[ComponentDeclaration::serial_endpoint(SERIAL1_ENDPOINT)],
            task_state: &[],
            shared_resources: &[],
            safety_channels: &[],
            tasks: &[],
            init_spawns: &[],
            monotonic: MonotonicDeclaration::systick(1_000),
            init_delay: None,
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "serial endpoint `serial1` consumes undeclared board serial hardware `serial1`"
        );
    }

    #[test]
    fn spi_endpoint_requires_hardware_declared_by_the_selected_board() {
        const BOARD_WITHOUT_IMU: BoardDeclaration = BoardDeclaration::new(
            "board_without_imu",
            McuDeclaration::new(Mcu::Stm32f405, ClockDeclaration::hsi(16_000_000)),
            &[],
        );
        const INVALID: AppComposition = AppComposition {
            board: &BOARD_WITHOUT_IMU,
            platform_config: imu_platform_config,
            components: &[ComponentDeclaration::imu_endpoint(SPI1_ENDPOINT)],
            task_state: &[],
            shared_resources: &[],
            safety_channels: &[],
            tasks: &[],
            init_spawns: &[],
            monotonic: MonotonicDeclaration::systick(1_000),
            init_delay: None,
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "SPI endpoint `spi1` consumes undeclared board SPI hardware `spi1`"
        );
    }

    #[test]
    fn platform_config_cannot_select_an_undeclared_imu_installation() {
        const INVALID: AppComposition = AppComposition {
            platform_config: unknown_imu_platform_config,
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "platform SPI port `Spi1` selects undeclared IMU installation 2 on endpoint `spi1`"
        );
    }

    #[test]
    fn serial_endpoint_resolves_hardware_from_the_selected_board() {
        const DIFFERENT_UART4: HardwareEndpointDeclaration = HardwareEndpointDeclaration::serial(
            SerialHardwareDeclaration::new("serial1", SerialPeripheral::Usart1)
                .rx(SerialRoute::dma(
                    PinId::new(GpioPort::B, 7),
                    DmaRoute::new(
                        DmaController::Dma2,
                        DmaStream::Stream2,
                        DmaChannel::Channel4,
                    ),
                ))
                .tx(SerialRoute::dma(
                    PinId::new(GpioPort::B, 6),
                    DmaRoute::new(
                        DmaController::Dma2,
                        DmaStream::Stream7,
                        DmaChannel::Channel4,
                    ),
                )),
        );
        const DIFFERENT_BOARD: BoardDeclaration = BoardDeclaration::new(
            "different_board",
            McuDeclaration::new(Mcu::Stm32f405, ClockDeclaration::hsi(16_000_000)),
            &[DIFFERENT_UART4],
        );
        const ALTERNATE: AppComposition = AppComposition {
            board: &DIFFERENT_BOARD,
            platform_config: osd_platform_config,
            components: &[ComponentDeclaration::serial_endpoint(SERIAL1_ENDPOINT)],
            task_state: &[],
            shared_resources: &[],
            safety_channels: &[],
            tasks: &[],
            init_spawns: &[],
            monotonic: MonotonicDeclaration::systick(1_000),
            init_delay: None,
        };

        validate(&ALTERNATE).unwrap();
    }

    #[test]
    fn standalone_task_requires_declared_board_gpio() {
        const INVALID_BLINK: TaskDeclaration = TaskDeclaration::software(
            "blink_led",
            &crate::input_catalog::selected_tasks::blink_led::CONTRACT,
        )
        .priority(1)
        .with_local(&[crate::input_catalog::selected_tasks::blink_led::LOCAL
            .led
            .bind("missing_led")])
        .with_shared(&[crate::input_catalog::selected_tasks::blink_led::SHARED
            .enabled
            .bind("button_enabled")])
        .with_config(&[crate::input_catalog::selected_tasks::blink_led::CONFIG
            .interval
            .set(fugit::MillisDurationU32::millis(500))]);
        const INVALID: AppComposition = AppComposition {
            tasks: &[TEST_BUTTON, INVALID_BLINK, TEST_OBSERVER],
            init_spawns: &[INVALID_BLINK.init_spawn()],
            ..task_test_app(&[])
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains(
            "binds local slot `led` to missing or incompatible local resource `missing_led`"
        ));
    }

    #[test]
    fn logical_spawn_requires_a_declared_software_task() {
        const INVALID_BUTTON: TaskDeclaration =
            TEST_BUTTON.with_spawns(&[stm32f4_tasks::button_exti::SPAWNS
                .button_changed
                .bind("missing_observer")]);
        const INVALID: AppComposition = AppComposition {
            tasks: &[INVALID_BUTTON, TEST_OBSERVER],
            ..task_test_app(&[])
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("targets undeclared task `missing_observer`"));
    }

    #[test]
    fn zero_monotonic_frequency_is_rejected() {
        const INVALID: AppComposition = AppComposition {
            monotonic: MonotonicDeclaration::systick(0),
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(error, "monotonic tick frequency must be nonzero");
    }

    #[test]
    fn monotonic_requires_a_declared_board_timer() {
        const INVALID: AppComposition = AppComposition {
            monotonic: MonotonicDeclaration::timer("missing", 1_000_000),
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(error, "monotonic consumes undeclared board timer `missing`");
    }

    #[test]
    fn spi_imu_requires_the_general_initialization_delay() {
        const INVALID: AppComposition = AppComposition {
            init_delay: None,
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "SPI IMU initialization requires a general initialization delay timer"
        );
    }

    #[test]
    fn initialization_delay_requires_a_declared_board_timer() {
        const INVALID: AppComposition = AppComposition {
            init_delay: Some(InitDelayDeclaration::timer("missing", 1_000_000)),
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "initialization delay consumes undeclared board timer `missing`"
        );
    }

    #[test]
    fn monotonic_and_initialization_delay_cannot_share_a_timer() {
        const INVALID: AppComposition = AppComposition {
            init_delay: Some(InitDelayDeclaration::timer("tim2", 1_000_000)),
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "the monotonic and initialization delay cannot consume the same timer peripheral"
        );
    }

    #[test]
    fn periodic_control_cannot_consume_the_tim2_monotonic() {
        const TIM2_CONTROL: crate::backends::stm32f4::periodic_control::PeriodicControlDeclaration =
            PERIODIC_CONTROL_TIMER
                .declare("control", board::TIM2.id)
                .scheduler_hz(800)
                .control_hz(400)
                .interrupt_priority(14);
        const INVALID: AppComposition = AppComposition {
            components: &[
                ComponentDeclaration::serial_endpoint(SERIAL1_ENDPOINT),
                ComponentDeclaration::serial_endpoint(SERIAL2_ENDPOINT),
                ComponentDeclaration::spi_endpoint(SPI1_ENDPOINT),
                ComponentDeclaration::periodic_control(TIM2_CONTROL),
            ],
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "periodic control `control` conflicts with the monotonic timer peripheral"
        );
    }

    #[test]
    fn periodic_control_cannot_consume_the_tim5_initialization_delay() {
        const TIM5_CONTROL: crate::backends::stm32f4::periodic_control::PeriodicControlDeclaration =
            PERIODIC_CONTROL_TIMER
                .declare("control", board::TIM5.id)
                .scheduler_hz(800)
                .control_hz(400)
                .interrupt_priority(14);
        const INVALID: AppComposition = AppComposition {
            components: &[
                ComponentDeclaration::serial_endpoint(SERIAL1_ENDPOINT),
                ComponentDeclaration::serial_endpoint(SERIAL2_ENDPOINT),
                ComponentDeclaration::spi_endpoint(SPI1_ENDPOINT),
                ComponentDeclaration::periodic_control(TIM5_CONTROL),
            ],
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "periodic control `control` conflicts with the initialization delay timer peripheral"
        );
    }

    #[test]
    fn two_components_cannot_own_the_same_periodic_timer() {
        const SECOND_CONTROL:
            crate::backends::stm32f4::periodic_control::PeriodicControlDeclaration =
            PERIODIC_CONTROL_TIMER
                .declare("secondary_control", board::TIM4.id)
                .scheduler_hz(800)
                .control_hz(400)
                .interrupt_priority(14);
        const INVALID: AppComposition = AppComposition {
            components: &[
                ComponentDeclaration::serial_endpoint(SERIAL1_ENDPOINT),
                ComponentDeclaration::serial_endpoint(SERIAL2_ENDPOINT),
                ComponentDeclaration::spi_endpoint(SPI1_ENDPOINT),
                ComponentDeclaration::periodic_control(CONTROL_TIMER),
                ComponentDeclaration::periodic_control(SECOND_CONTROL),
            ],
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "board hardware `tim4` is consumed by more than one component"
        );
    }
}
