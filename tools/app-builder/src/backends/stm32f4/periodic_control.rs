//! Periodic STM32F4 control-scheduler component.
//!
//! The application selects an already-declared board timer and exact rates.
//! The backend owns no allocation policy: it only lowers that declaration to
//! the shared STM32F4 scheduler recipe and an interrupt-bound task.

use crate::{
    backends::stm32f4::{board_declaration::BoardDeclaration, tasks},
    rtic::{
        composition::TaskSafetyClass,
        task::{LocalResourceBinding, SharedResourceBinding, SpawnBinding, TaskContract},
    },
};

/// Authoring-only scheduler used to type-check the reusable interrupt body.
///
/// Generated firmware replaces this type through the resolved component
/// resource recipe with `CounterHz<TIMx>`.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct ControlScheduler {
    acknowledged_ticks: u32,
}

/// Host-side stand-in for the shared STM32F4 timer acknowledgement primitive.
pub fn acknowledge_control_tick(scheduler: &mut ControlScheduler) {
    scheduler.acknowledged_ticks = scheduler.acknowledged_ticks.wrapping_add(1);
}

#[cfg(test)]
impl ControlScheduler {
    pub(crate) const fn acknowledged_ticks(&self) -> u32 {
        self.acknowledged_ticks
    }
}

/// Semantic role of one periodic-control component resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeriodicControlResourceRole {
    /// HAL timer counter that owns the update interrupt.
    Scheduler,
    /// Number of scheduler ticks accumulated toward the next control step.
    Phase,
}

/// One private, task-local resource in the periodic-control graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeriodicControlResource {
    /// Stable semantic role used by resolution and lowering.
    pub role: PeriodicControlResourceRole,
}

impl PeriodicControlResource {
    const fn new(role: PeriodicControlResourceRole) -> Self {
        Self { role }
    }
}

/// Resources owned exclusively by the synchronous control interrupt task.
pub const PERIODIC_CONTROL_RESOURCES: &[PeriodicControlResource] = &[
    PeriodicControlResource::new(PeriodicControlResourceRole::Scheduler),
    PeriodicControlResource::new(PeriodicControlResourceRole::Phase),
];

/// Reusable periodic-control timer definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeriodicControlDefinition {
    /// Stable reusable definition identifier.
    pub id: &'static str,
    /// Synchronous timer interrupt task contract.
    pub task: &'static TaskContract,
    /// Private component resource graph.
    pub resources: &'static [PeriodicControlResource],
}

impl PeriodicControlDefinition {
    /// Declares one instance bound to explicit board timer hardware.
    pub const fn declare(
        self,
        id: &'static str,
        hardware_id: &'static str,
    ) -> PeriodicControlDeclaration {
        PeriodicControlDeclaration {
            id,
            definition: self,
            hardware_id,
            scheduler_hz: 0,
            control_hz: 0,
            interrupt_priority: 0,
            task: self.task,
            safety_class: TaskSafetyClass::NonSafetyCritical,
            local: &[],
            shared: &[],
            spawns: &[],
        }
    }
}

/// Application-selected periodic control scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeriodicControlDeclaration {
    /// Component identifier used to namespace resources and the control task.
    pub id: &'static str,
    /// Reusable task and resource definition.
    pub definition: PeriodicControlDefinition,
    /// Exact board timer selected by the application.
    pub hardware_id: &'static str,
    /// Hardware update-interrupt frequency.
    pub scheduler_hz: u32,
    /// Control-step frequency derived from scheduler ticks.
    pub control_hz: u32,
    /// RTIC priority of the timer-bound synchronous task.
    pub interrupt_priority: u8,
    /// Reusable synchronous task selected for this scheduler instance.
    pub task: &'static TaskContract,
    /// Safety classification of the expanded timer task.
    pub safety_class: TaskSafetyClass,
    /// Application bindings beyond the timer's private scheduler resources.
    pub local: &'static [LocalResourceBinding],
    /// Application shared-resource bindings consumed by the control task.
    pub shared: &'static [SharedResourceBinding],
    /// Typed software-task wake edges emitted by the control task.
    pub spawns: &'static [SpawnBinding],
}

impl PeriodicControlDeclaration {
    /// Sets the timer update frequency in hertz.
    pub const fn scheduler_hz(mut self, frequency_hz: u32) -> Self {
        self.scheduler_hz = frequency_hz;
        self
    }

    /// Sets the control-step frequency in hertz.
    pub const fn control_hz(mut self, frequency_hz: u32) -> Self {
        self.control_hz = frequency_hz;
        self
    }

    /// Sets the timer interrupt's RTIC priority.
    pub const fn interrupt_priority(mut self, priority: u8) -> Self {
        self.interrupt_priority = priority;
        self
    }

    /// Replaces the timing-only default with an application control contract.
    pub const fn task(mut self, task: &'static TaskContract) -> Self {
        self.task = task;
        self
    }

    /// Marks the expanded task as an authoritative safety-path owner.
    pub const fn safety_critical(mut self) -> Self {
        self.safety_class = TaskSafetyClass::SafetyCritical;
        self
    }

    /// Supplies application-owned local capability bindings.
    pub const fn with_local(mut self, bindings: &'static [LocalResourceBinding]) -> Self {
        self.local = bindings;
        self
    }

    /// Supplies observation and configuration shared-resource bindings.
    pub const fn with_shared(mut self, bindings: &'static [SharedResourceBinding]) -> Self {
        self.shared = bindings;
        self
    }

    /// Supplies typed software-task wake destinations.
    pub const fn with_spawns(mut self, bindings: &'static [SpawnBinding]) -> Self {
        self.spawns = bindings;
        self
    }

    /// Exact number of scheduler updates per control step after validation.
    pub const fn ticks_per_control(self) -> u32 {
        match self.scheduler_hz.checked_div(self.control_hz) {
            Some(ticks) => ticks,
            None => 0,
        }
    }
}

/// Shared STM32F4 periodic-control definition.
pub const PERIODIC_CONTROL_TIMER: PeriodicControlDefinition = PeriodicControlDefinition {
    id: "periodic_control_timer",
    task: &tasks::periodic_control_tick::CONTRACT,
    resources: PERIODIC_CONTROL_RESOURCES,
};

pub(crate) const fn resource_suffix(role: PeriodicControlResourceRole) -> &'static str {
    match role {
        PeriodicControlResourceRole::Scheduler => "scheduler",
        PeriodicControlResourceRole::Phase => "phase",
    }
}

pub(crate) fn validate(
    declaration: PeriodicControlDeclaration,
    board: &BoardDeclaration,
) -> Result<(), String> {
    if board.timer(declaration.hardware_id).is_none() {
        return Err(format!(
            "periodic control `{}` consumes undeclared board timer `{}`",
            declaration.id, declaration.hardware_id
        ));
    }
    if declaration.scheduler_hz == 0 || declaration.control_hz == 0 {
        return Err(format!(
            "periodic control `{}` scheduler and control frequencies must be nonzero",
            declaration.id
        ));
    }
    if declaration.control_hz > declaration.scheduler_hz {
        return Err(format!(
            "periodic control `{}` control frequency cannot exceed its scheduler frequency",
            declaration.id
        ));
    }
    if declaration
        .scheduler_hz
        .checked_rem(declaration.control_hz)
        .is_some_and(|remainder| remainder != 0)
    {
        return Err(format!(
            "periodic control `{}` scheduler frequency must be an integer multiple of its control frequency",
            declaration.id
        ));
    }
    if declaration.interrupt_priority == 0 {
        return Err(format!(
            "periodic control `{}` must have a nonzero interrupt priority",
            declaration.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_catalog::foxeer_f405_v2::board::{BOARD, TIM4};

    #[test]
    fn declaration_preserves_the_golden_frequency_ratio() {
        let declaration = PERIODIC_CONTROL_TIMER
            .declare("control", TIM4.id)
            .scheduler_hz(800)
            .control_hz(400)
            .interrupt_priority(14);

        validate(declaration, &BOARD).unwrap();
        assert_eq!(declaration.ticks_per_control(), 2);
    }

    #[test]
    fn declaration_rejects_non_integral_control_cadence() {
        let declaration = PERIODIC_CONTROL_TIMER
            .declare("control", TIM4.id)
            .scheduler_hz(800)
            .control_hz(333)
            .interrupt_priority(14);

        assert!(
            validate(declaration, &BOARD)
                .unwrap_err()
                .contains("integer multiple")
        );
    }

    #[test]
    fn declaration_rejects_zero_or_inverted_frequencies() {
        let zero = PERIODIC_CONTROL_TIMER
            .declare("control", TIM4.id)
            .scheduler_hz(0)
            .control_hz(400)
            .interrupt_priority(14);
        let inverted = PERIODIC_CONTROL_TIMER
            .declare("control", TIM4.id)
            .scheduler_hz(400)
            .control_hz(800)
            .interrupt_priority(14);

        assert!(validate(zero, &BOARD).unwrap_err().contains("nonzero"));
        assert!(
            validate(inverted, &BOARD)
                .unwrap_err()
                .contains("cannot exceed")
        );
    }
}
