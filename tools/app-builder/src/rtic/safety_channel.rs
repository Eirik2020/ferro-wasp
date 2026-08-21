//! Authoritative, bounded safety-channel declarations.
//!
//! A safety channel has exactly one task-local producer and one task-local
//! consumer. The declaration describes topology only; the generated firmware
//! uses [`ferrowasp_core::safety_channel::SafetyChannel`] for storage and splits
//! it exactly once during RTIC initialization.

/// Authoritative message families supported by the current generator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyMessage {
    /// Complete latest valid SBUS input snapshot transferred into control.
    SbusInput,

    /// Decoded inertial sample transferred into control.
    ImuSample,

    /// Commands transferred from control to the safety-owned actuator task.
    MotorCommand,

    /// Observation-only IMU health evidence transferred into safety.
    PreArmHealth,

    /// Fresh safety-owned authority observation transferred to the actuator.
    ActuatorGuard,

    /// Actuator preparation completion or abort transferred into safety.
    ActuatorPreparation,
}

impl SafetyMessage {
    /// Stable portable identifier for this message family.
    pub const fn type_id(self) -> &'static str {
        match self {
            Self::SbusInput => "sbus_input",
            Self::ImuSample => "imu_sample",
            Self::MotorCommand => "motor_cmd",
            Self::PreArmHealth => "prearm_health",
            Self::ActuatorGuard => "actuator_guard",
            Self::ActuatorPreparation => "actuator_preparation",
        }
    }

    /// Concrete Rust message type emitted by the renderer.
    pub const fn rust_type(self) -> &'static str {
        match self {
            Self::SbusInput => "RcInputSnapshot",
            Self::ImuSample => "ImuData",
            Self::MotorCommand => "MotorCmd",
            Self::PreArmHealth => "PreArmHealthReport",
            Self::ActuatorGuard => "ActuatorGuardReport",
            Self::ActuatorPreparation => "ActuatorPreparationReport",
        }
    }

    /// Concrete Rust type required by the producer task-local slot.
    pub const fn producer_rust_type(self) -> &'static str {
        match self {
            Self::SbusInput => "SafetyProducer<'static, RcInputSnapshot>",
            Self::ImuSample => "SafetyProducer<'static, ImuData>",
            Self::MotorCommand => "SafetyProducer<'static, MotorCmd>",
            Self::PreArmHealth => "SafetyProducer<'static, PreArmHealthReport>",
            Self::ActuatorGuard => "SafetyProducer<'static, ActuatorGuardReport>",
            Self::ActuatorPreparation => "SafetyProducer<'static, ActuatorPreparationReport>",
        }
    }

    /// Concrete Rust type required by the consumer task-local slot.
    pub const fn consumer_rust_type(self) -> &'static str {
        match self {
            Self::SbusInput => "SafetyConsumer<'static, RcInputSnapshot>",
            Self::ImuSample => "SafetyConsumer<'static, ImuData>",
            Self::MotorCommand => "SafetyConsumer<'static, MotorCmd>",
            Self::PreArmHealth => "SafetyConsumer<'static, PreArmHealthReport>",
            Self::ActuatorGuard => "SafetyConsumer<'static, ActuatorGuardReport>",
            Self::ActuatorPreparation => "SafetyConsumer<'static, ActuatorPreparationReport>",
        }
    }
}

/// One exclusive single-producer, single-consumer safety path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyChannelDeclaration {
    /// Private static-storage identifier used by generated initialization.
    pub id: &'static str,

    /// Authoritative message family transferred by this channel.
    pub message: SafetyMessage,

    /// Number of messages that fit without an intervening receive.
    pub usable_capacity: usize,

    /// Concrete task-local resource identifier for the sole producer.
    pub producer: &'static str,

    /// Concrete task-local resource identifier for the sole consumer.
    pub consumer: &'static str,
}

impl SafetyChannelDeclaration {
    /// Declares a bounded authoritative channel.
    pub const fn new(
        id: &'static str,
        message: SafetyMessage,
        usable_capacity: usize,
        producer: &'static str,
        consumer: &'static str,
    ) -> Self {
        Self {
            id,
            message,
            usable_capacity,
            producer,
            consumer,
        }
    }

    /// Declares a valid-SBUS-input path into the control task.
    pub const fn sbus_input(
        id: &'static str,
        usable_capacity: usize,
        producer: &'static str,
        consumer: &'static str,
    ) -> Self {
        Self::new(
            id,
            SafetyMessage::SbusInput,
            usable_capacity,
            producer,
            consumer,
        )
    }

    /// Declares a decoded-IMU-sample path into the control task.
    pub const fn imu_samples(
        id: &'static str,
        usable_capacity: usize,
        producer: &'static str,
        consumer: &'static str,
    ) -> Self {
        Self::new(
            id,
            SafetyMessage::ImuSample,
            usable_capacity,
            producer,
            consumer,
        )
    }

    /// Declares the golden-app control-to-actuator motor-command path.
    pub const fn motor_commands(
        id: &'static str,
        usable_capacity: usize,
        producer: &'static str,
        consumer: &'static str,
    ) -> Self {
        Self::new(
            id,
            SafetyMessage::MotorCommand,
            usable_capacity,
            producer,
            consumer,
        )
    }

    /// Declares an observation-only control-health path into safety.
    pub const fn prearm_health(
        id: &'static str,
        usable_capacity: usize,
        producer: &'static str,
        consumer: &'static str,
    ) -> Self {
        Self::new(
            id,
            SafetyMessage::PreArmHealth,
            usable_capacity,
            producer,
            consumer,
        )
    }

    /// Declares the safety-master authority path into the physical actuator.
    pub const fn actuator_guard(
        id: &'static str,
        usable_capacity: usize,
        producer: &'static str,
        consumer: &'static str,
    ) -> Self {
        Self::new(
            id,
            SafetyMessage::ActuatorGuard,
            usable_capacity,
            producer,
            consumer,
        )
    }

    /// Declares an actuator preparation/fault report path into safety.
    pub const fn actuator_preparation(
        id: &'static str,
        usable_capacity: usize,
        producer: &'static str,
        consumer: &'static str,
    ) -> Self {
        Self::new(
            id,
            SafetyMessage::ActuatorPreparation,
            usable_capacity,
            producer,
            consumer,
        )
    }

    /// Backing `heapless` queue length used by `SafetyChannel<T, N>`.
    pub const fn queue_length(self) -> Option<usize> {
        self.usable_capacity.checked_add(1)
    }
}
