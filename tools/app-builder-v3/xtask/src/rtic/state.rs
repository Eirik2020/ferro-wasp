//! Typed, versioned application-owned task-local state declarations.
//!
//! State metadata selects from closed initialization recipes. It cannot carry
//! Rust fragments, so every generated type and constructor remains owned and
//! reviewed by the backend.

use std::collections::{BTreeMap, BTreeSet};

/// Semantic role of persistent Foxeer control-task state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TaskStateRole {
    /// Scheduler interrupt count toward the next control step.
    ControlLoopCounter,
    /// Number of scheduler ticks per control step.
    SamplesPerControlLoop,
    /// Rate controller and mixer state.
    FlightController,
    /// Three-axis gyro-rate low-pass filter state.
    ImuRateFilter,
    /// Complementary gyro-angle estimator state.
    ImuAngleIntegrator,
    /// Physical-body to legacy rate-controller axis compatibility map.
    GyroAxisMap,
    /// Stationary startup gyro-bias calibration state.
    GyroBiasCalibrator,
    /// Last consumed IMU sequence number.
    ImuLastSequence,
    /// Consecutive stale-IMU control-step count.
    ImuStaleTicks,
    /// Last applied tuning request sequence number.
    AppliedTuningSequence,
    /// Next motor-command sequence number.
    MotorCommandSequence,
    /// Previous RC-link validity at the arming/reset boundary.
    RcLinkWasValid,
    /// Latest complete healthy RC snapshot retained between control steps.
    LatestRcInput,
    /// Last observed valid RC frame sequence.
    RcLastValidFrames,
    /// Consecutive control steps without a new healthy RC frame.
    RcStaleTicks,
    /// Previous safety-owned arming state at the reset boundary.
    ControlWasArmed,
    /// Exclusive golden RC-link and arming state machine.
    SafetyMaster,
}

/// Closed Rust type vocabulary supported by task-state recipes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStateType {
    /// Boolean state.
    Bool,
    /// Unsigned 32-bit state.
    U32,
    /// `ferrowasp_tasks` flight controller.
    FlightController,
    /// Three-axis IMU rate low-pass filter.
    ImuRateLowPassFilter,
    /// Complementary angle integrator.
    GyroAngleIntegrator,
    /// Signed axis permutation.
    FrameRotation,
    /// Gyro bias calibrator.
    GyroBiasCalibrator,
    /// Complete retained radio-control snapshot.
    RcInputSnapshot,
    /// Golden Foxeer safety-master state.
    FoxeerSafetyMaster,
}

impl TaskStateType {
    /// Rust spelling used by reusable task contracts and generated locals.
    pub const fn rust_type(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::U32 => "u32",
            Self::FlightController => "dt::FlightController",
            Self::ImuRateLowPassFilter => "dt::ImuRateLowPassFilter",
            Self::GyroAngleIntegrator => "dt::GyroAngleIntegrator",
            Self::FrameRotation => "FrameRotation",
            Self::GyroBiasCalibrator => "dt::GyroBiasCalibrator",
            Self::RcInputSnapshot => "RcInputSnapshot",
            Self::FoxeerSafetyMaster => "ferrowasp_tasks::foxeer_safety::FoxeerSafetyMaster",
        }
    }
}

/// Reviewed constructor recipe for one persistent local value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStateRecipe {
    /// Typed primitive counter or sequence initial value.
    U32(u32),
    /// Typed primitive Boolean initial value.
    Bool(bool),
    /// Current pinned Foxeer controller configuration and gains.
    FoxeerFlightControllerV1,
    /// Current pinned Foxeer three-axis gyro LPF configuration.
    FoxeerImuRateLowPassFilterV1,
    /// Zeroed complementary gyro-angle estimator.
    GyroAngleIntegratorV1,
    /// Physical-body to legacy controller compatibility map.
    BodyRateToControllerMapV1,
    /// Current pinned Foxeer stationary gyro-bias calibration policy.
    FoxeerGyroBiasCalibratorV1,
    /// Empty radio-control snapshot with no valid frame.
    EmptyRcInputSnapshotV1,
    /// Output-inhibited golden RC-link and safety-master policy.
    OutputInhibitedFoxeerSafetyMasterV1,
}

impl TaskStateRecipe {
    /// Rust type constructed by this recipe.
    pub const fn state_type(self) -> TaskStateType {
        match self {
            Self::U32(_) => TaskStateType::U32,
            Self::Bool(_) => TaskStateType::Bool,
            Self::FoxeerFlightControllerV1 => TaskStateType::FlightController,
            Self::FoxeerImuRateLowPassFilterV1 => TaskStateType::ImuRateLowPassFilter,
            Self::GyroAngleIntegratorV1 => TaskStateType::GyroAngleIntegrator,
            Self::BodyRateToControllerMapV1 => TaskStateType::FrameRotation,
            Self::FoxeerGyroBiasCalibratorV1 => TaskStateType::GyroBiasCalibrator,
            Self::EmptyRcInputSnapshotV1 => TaskStateType::RcInputSnapshot,
            Self::OutputInhibitedFoxeerSafetyMasterV1 => TaskStateType::FoxeerSafetyMaster,
        }
    }
}

/// One required semantic field in a versioned task-state schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskStateRequirement {
    /// Semantic role that must appear exactly once.
    pub role: TaskStateRole,
    /// Exact closed type required for the role.
    pub state_type: TaskStateType,
}

impl TaskStateRequirement {
    /// Creates one typed schema requirement.
    pub const fn new(role: TaskStateRole, state_type: TaskStateType) -> Self {
        Self { role, state_type }
    }
}

/// Versioned set of task-local state requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskStateSchema {
    /// Stable schema family identifier.
    pub id: &'static str,
    /// Nonzero schema version.
    pub version: u16,
    /// Complete required state roles.
    pub requirements: &'static [TaskStateRequirement],
}

impl TaskStateSchema {
    /// Binds this schema to one resolved task and its complete field list.
    pub const fn declare(
        self,
        owner_task: &'static str,
        fields: &'static [TaskStateField],
    ) -> TaskStateDeclaration {
        TaskStateDeclaration {
            schema: self,
            owner_task,
            fields,
        }
    }
}

/// One named application-owned local-state value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskStateField {
    /// Generated RTIC local-resource identifier.
    pub id: &'static str,
    /// Semantic role fulfilled by this field.
    pub role: TaskStateRole,
    /// Reviewed backend constructor recipe.
    pub recipe: TaskStateRecipe,
}

impl TaskStateField {
    /// Creates one typed state field.
    pub const fn new(id: &'static str, role: TaskStateRole, recipe: TaskStateRecipe) -> Self {
        Self { id, role, recipe }
    }
}

/// Complete application-owned local-state set for one task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskStateDeclaration {
    /// Versioned schema defining complete role coverage.
    pub schema: TaskStateSchema,
    /// Concrete resolved task with exclusive ownership of every field.
    pub owner_task: &'static str,
    /// Complete concrete field declarations.
    pub fields: &'static [TaskStateField],
}

/// Required persistent state for the golden Foxeer control task.
pub const FOXEER_CONTROL_STATE_REQUIREMENTS_V1: &[TaskStateRequirement] = &[
    TaskStateRequirement::new(TaskStateRole::ControlLoopCounter, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::SamplesPerControlLoop, TaskStateType::U32),
    TaskStateRequirement::new(
        TaskStateRole::FlightController,
        TaskStateType::FlightController,
    ),
    TaskStateRequirement::new(
        TaskStateRole::ImuRateFilter,
        TaskStateType::ImuRateLowPassFilter,
    ),
    TaskStateRequirement::new(
        TaskStateRole::ImuAngleIntegrator,
        TaskStateType::GyroAngleIntegrator,
    ),
    TaskStateRequirement::new(TaskStateRole::GyroAxisMap, TaskStateType::FrameRotation),
    TaskStateRequirement::new(
        TaskStateRole::GyroBiasCalibrator,
        TaskStateType::GyroBiasCalibrator,
    ),
    TaskStateRequirement::new(TaskStateRole::ImuLastSequence, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::ImuStaleTicks, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::AppliedTuningSequence, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::MotorCommandSequence, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::RcLinkWasValid, TaskStateType::Bool),
];

/// Version 1 of the complete golden Foxeer control-state schema.
pub const FOXEER_CONTROL_STATE_V1: TaskStateSchema = TaskStateSchema {
    id: "foxeer_control_state",
    version: 1,
    requirements: FOXEER_CONTROL_STATE_REQUIREMENTS_V1,
};

/// Required persistent state for the channel-backed Foxeer control task.
pub const FOXEER_CONTROL_STATE_REQUIREMENTS_V2: &[TaskStateRequirement] = &[
    TaskStateRequirement::new(TaskStateRole::ControlLoopCounter, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::SamplesPerControlLoop, TaskStateType::U32),
    TaskStateRequirement::new(
        TaskStateRole::FlightController,
        TaskStateType::FlightController,
    ),
    TaskStateRequirement::new(
        TaskStateRole::ImuRateFilter,
        TaskStateType::ImuRateLowPassFilter,
    ),
    TaskStateRequirement::new(
        TaskStateRole::ImuAngleIntegrator,
        TaskStateType::GyroAngleIntegrator,
    ),
    TaskStateRequirement::new(TaskStateRole::GyroAxisMap, TaskStateType::FrameRotation),
    TaskStateRequirement::new(
        TaskStateRole::GyroBiasCalibrator,
        TaskStateType::GyroBiasCalibrator,
    ),
    TaskStateRequirement::new(TaskStateRole::ImuLastSequence, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::ImuStaleTicks, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::AppliedTuningSequence, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::MotorCommandSequence, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::RcLinkWasValid, TaskStateType::Bool),
    TaskStateRequirement::new(TaskStateRole::LatestRcInput, TaskStateType::RcInputSnapshot),
    TaskStateRequirement::new(TaskStateRole::RcLastValidFrames, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::RcStaleTicks, TaskStateType::U32),
    TaskStateRequirement::new(TaskStateRole::ControlWasArmed, TaskStateType::Bool),
];

/// Version 2 adds explicit RC freshness and arming-boundary state.
pub const FOXEER_CONTROL_STATE_V2: TaskStateSchema = TaskStateSchema {
    id: "foxeer_control_state",
    version: 2,
    requirements: FOXEER_CONTROL_STATE_REQUIREMENTS_V2,
};

/// Required persistent state for the priority-16 safety master.
pub const FOXEER_SAFETY_STATE_REQUIREMENTS_V1: &[TaskStateRequirement] =
    &[TaskStateRequirement::new(
        TaskStateRole::SafetyMaster,
        TaskStateType::FoxeerSafetyMaster,
    )];

/// Version 1 of the exclusive golden safety-master state schema.
pub const FOXEER_SAFETY_STATE_V1: TaskStateSchema = TaskStateSchema {
    id: "foxeer_safety_state",
    version: 1,
    requirements: FOXEER_SAFETY_STATE_REQUIREMENTS_V1,
};

pub(crate) fn validate(declarations: &[TaskStateDeclaration]) -> Result<(), String> {
    let mut field_owners = BTreeMap::new();
    for declaration in declarations {
        validate_declaration(*declaration)?;
        for field in declaration.fields {
            if let Some(previous_owner) = field_owners.insert(field.id, declaration.owner_task) {
                return Err(format!(
                    "task-local state `{}` is owned by both `{previous_owner}` and `{}`",
                    field.id, declaration.owner_task
                ));
            }
        }
    }
    Ok(())
}

fn validate_declaration(declaration: TaskStateDeclaration) -> Result<(), String> {
    if declaration.schema.version == 0 {
        return Err(format!(
            "task-state schema `{}` must have a nonzero version",
            declaration.schema.id
        ));
    }
    let mut requirements = BTreeMap::new();
    for requirement in declaration.schema.requirements {
        if requirements
            .insert(requirement.role, requirement.state_type)
            .is_some()
        {
            return Err(format!(
                "task-state schema `{}` version {} repeats role `{:?}`",
                declaration.schema.id, declaration.schema.version, requirement.role
            ));
        }
    }

    let mut fields = BTreeMap::new();
    let mut field_ids = BTreeSet::new();
    for field in declaration.fields {
        if !field_ids.insert(field.id) {
            return Err(format!(
                "task-state owner `{}` declares field `{}` more than once",
                declaration.owner_task, field.id
            ));
        }
        if fields.insert(field.role, field).is_some() {
            return Err(format!(
                "task-state owner `{}` binds role `{:?}` more than once",
                declaration.owner_task, field.role
            ));
        }
    }

    for (role, expected_type) in &requirements {
        let field = fields.get(role).ok_or_else(|| {
            format!(
                "task-state owner `{}` is missing required role `{role:?}` from schema `{}` version {}",
                declaration.owner_task, declaration.schema.id, declaration.schema.version
            )
        })?;
        let actual_type = field.recipe.state_type();
        if actual_type != *expected_type {
            return Err(format!(
                "task-state field `{}` for role `{role:?}` requires `{expected_type:?}`, but recipe supplies `{actual_type:?}`",
                field.id
            ));
        }
    }
    if let Some(extra) = fields.keys().find(|role| !requirements.contains_key(role)) {
        return Err(format!(
            "task-state owner `{}` binds undeclared role `{extra:?}` for schema `{}` version {}",
            declaration.owner_task, declaration.schema.id, declaration.schema.version
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL_SCHEMA: TaskStateSchema = TaskStateSchema {
        id: "small",
        version: 1,
        requirements: &[
            TaskStateRequirement::new(TaskStateRole::ImuLastSequence, TaskStateType::U32),
            TaskStateRequirement::new(TaskStateRole::RcLinkWasValid, TaskStateType::Bool),
        ],
    };

    #[test]
    fn missing_required_state_is_rejected() {
        const DECLARATION: TaskStateDeclaration = SMALL_SCHEMA.declare(
            "control_loop",
            &[TaskStateField::new(
                "imu_last_sequence",
                TaskStateRole::ImuLastSequence,
                TaskStateRecipe::U32(0),
            )],
        );

        assert!(
            validate(&[DECLARATION])
                .unwrap_err()
                .contains("missing required role")
        );
    }

    #[test]
    fn duplicate_semantic_state_is_rejected() {
        const DECLARATION: TaskStateDeclaration = SMALL_SCHEMA.declare(
            "control_loop",
            &[
                TaskStateField::new(
                    "imu_last_sequence",
                    TaskStateRole::ImuLastSequence,
                    TaskStateRecipe::U32(0),
                ),
                TaskStateField::new(
                    "second_sequence",
                    TaskStateRole::ImuLastSequence,
                    TaskStateRecipe::U32(0),
                ),
                TaskStateField::new(
                    "rc_link_was_valid",
                    TaskStateRole::RcLinkWasValid,
                    TaskStateRecipe::Bool(false),
                ),
            ],
        );

        assert!(validate(&[DECLARATION]).unwrap_err().contains("binds role"));
    }

    #[test]
    fn incompatible_recipe_type_is_rejected() {
        const DECLARATION: TaskStateDeclaration = SMALL_SCHEMA.declare(
            "control_loop",
            &[
                TaskStateField::new(
                    "imu_last_sequence",
                    TaskStateRole::ImuLastSequence,
                    TaskStateRecipe::Bool(false),
                ),
                TaskStateField::new(
                    "rc_link_was_valid",
                    TaskStateRole::RcLinkWasValid,
                    TaskStateRecipe::Bool(false),
                ),
            ],
        );

        assert!(
            validate(&[DECLARATION])
                .unwrap_err()
                .contains("recipe supplies")
        );
    }

    #[test]
    fn one_field_cannot_have_multiple_task_owners() {
        const FIRST: TaskStateDeclaration = SMALL_SCHEMA.declare(
            "control_loop",
            &[
                TaskStateField::new(
                    "sequence",
                    TaskStateRole::ImuLastSequence,
                    TaskStateRecipe::U32(0),
                ),
                TaskStateField::new(
                    "rc_valid_first",
                    TaskStateRole::RcLinkWasValid,
                    TaskStateRecipe::Bool(false),
                ),
            ],
        );
        const SECOND: TaskStateDeclaration = SMALL_SCHEMA.declare(
            "other_task",
            &[
                TaskStateField::new(
                    "sequence",
                    TaskStateRole::ImuLastSequence,
                    TaskStateRecipe::U32(0),
                ),
                TaskStateField::new(
                    "rc_valid_second",
                    TaskStateRole::RcLinkWasValid,
                    TaskStateRecipe::Bool(false),
                ),
            ],
        );

        assert!(
            validate(&[FIRST, SECOND])
                .unwrap_err()
                .contains("owned by both")
        );
    }
}
