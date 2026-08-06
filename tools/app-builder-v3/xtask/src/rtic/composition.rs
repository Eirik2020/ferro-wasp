//! Application-composition declarations for reusable RTIC-shaped tasks.

use std::{collections::BTreeSet, error::Error, fmt};

use stm32f4xx_hal::pac::Interrupt;

use crate::hardware_definitions::stm32f4::board_declaration::{
    self as board_definition, BoardDeclaration,
};

use super::{
    component::ComponentDeclaration,
    task::{
        ConfigBinding, LocalResourceBinding, SharedResourceBinding, SpawnBinding, TaskContract,
    },
};

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

/// One concrete instance of a reusable task contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskDeclaration {
    /// Concrete task identifier used by the generated RTIC application.
    pub id: &'static str,

    /// Reusable task-local contract implemented by this instance.
    pub contract: &'static TaskContract,

    /// Interrupt or software-spawn entry mechanism.
    pub trigger: TaskTrigger,

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

/// Complete authoring declaration consumed by the future RTIC generator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppComposition {
    /// Board whose physical hardware may be consumed by components and tasks.
    pub board: &'static BoardDeclaration,

    /// Reusable hardware components expanded before standalone task resolution.
    pub components: &'static [ComponentDeclaration],

    /// Application-owned values shared by standalone tasks.
    pub shared_resources: &'static [SharedResourceDeclaration],

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
/// This phase intentionally does not resolve hardware IDs or logical spawn
/// destinations; those names belong to board and whole-task-graph resolution
/// in the future generator. It validates identifier shape, complete slot
/// coverage, shared-resource types, priorities, init spawns, unique interrupt
/// ownership, and exclusive task-local resource ownership.
pub fn validate(composition: &AppComposition) -> Result<(), CompositionError> {
    board_definition::validate(composition.board).map_err(CompositionError::new)?;

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
        if !component_hardware.insert(component.hardware_id()) {
            return Err(CompositionError::new(format!(
                "board serial hardware `{}` is consumed by more than one component",
                component.hardware_id()
            )));
        }
    }

    let mut shared_ids = BTreeSet::new();
    for resource in composition.shared_resources {
        validate_identifier(resource.id, "shared resource")?;
        if !shared_ids.insert(resource.id) {
            return Err(CompositionError::new(format!(
                "shared resource `{}` is declared more than once",
                resource.id
            )));
        }
    }

    let mut task_ids = BTreeSet::new();
    let mut interrupts = Vec::new();
    let mut local_targets = BTreeSet::new();
    for task in composition.tasks {
        validate_identifier(task.id, "task")?;
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
            if !local_targets.insert(binding.target()) {
                return Err(CompositionError::new(format!(
                    "hardware resource `{}` is owned by more than one local task binding",
                    binding.target()
                )));
            }
        }

        for binding in task.shared {
            validate_identifier(binding.target(), "shared resource binding")?;
            let declaration = composition
                .shared_resources
                .iter()
                .find(|resource| resource.id == binding.target())
                .ok_or_else(|| {
                    CompositionError::new(format!(
                        "task `{}` binds `{}` to missing shared resource `{}`",
                        task.id,
                        binding.logical(),
                        binding.target()
                    ))
                })?;
            if declaration.rust_type() != binding.rust_type() {
                return Err(CompositionError::new(format!(
                    "task `{}` shared slot `{}` requires `{}`, but `{}` is `{}`",
                    task.id,
                    binding.logical(),
                    binding.rust_type(),
                    binding.target(),
                    declaration.rust_type()
                )));
            }
        }

        for binding in task.spawns {
            validate_identifier(binding.target(), "spawn target")?;
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
        hardware_definitions::stm32f4::{
            board_declaration::{BoardDeclaration, SerialHardwareDeclaration},
            dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
            pins::{GpioPort, PinId},
            serial::{SerialPeripheral, SerialRoute},
            tasks as stm32f4_tasks,
        },
        target::app_composition::APP_COMPOSITION,
    };

    #[test]
    fn complete_example_satisfies_all_task_contracts() {
        validate(&APP_COMPOSITION).unwrap();
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
            shared_resources: &[SharedResourceDeclaration::bool("button_enabled", false)],
            tasks: &[INVALID_TASK],
            init_spawns: &[],
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("does not bind configuration value `toggle_on_press`"));
    }

    #[test]
    fn init_cannot_spawn_an_interrupt_task() {
        const INVALID: AppComposition = AppComposition {
            init_spawns: &[crate::target::app_composition::BUTTON_EXTI_TASK.init_spawn()],
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert!(error.contains("init cannot spawn interrupt task"));
    }

    #[test]
    fn serial_endpoint_requires_hardware_declared_by_the_selected_board() {
        const BOARD_WITHOUT_UART4: BoardDeclaration = BoardDeclaration::new("empty_board", &[]);
        const INVALID: AppComposition = AppComposition {
            board: &BOARD_WITHOUT_UART4,
            ..APP_COMPOSITION
        };

        let error = validate(&INVALID).unwrap_err().to_string();
        assert_eq!(
            error,
            "serial endpoint `osd_uart` consumes undeclared board serial hardware `uart4`"
        );
    }

    #[test]
    fn serial_endpoint_resolves_hardware_from_the_selected_board() {
        const DIFFERENT_UART4: SerialHardwareDeclaration =
            SerialHardwareDeclaration::new("uart4", SerialPeripheral::Usart1)
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
                ));
        const DIFFERENT_BOARD: BoardDeclaration =
            BoardDeclaration::new("different_board", &[DIFFERENT_UART4]);
        const ALTERNATE: AppComposition = AppComposition {
            board: &DIFFERENT_BOARD,
            ..APP_COMPOSITION
        };

        validate(&ALTERNATE).unwrap();
    }
}
