//! Reusable actuator-command queue publication and inhibited consumption.

use ferrowasp_core::{
    actuator::throttle_to_u16,
    safety::{
        ActuatorAuthority, ActuatorCmd, ActuatorGuardReport, ESC_IDLE_THROTTLE,
        MOTOR_CMD_QUEUE_CAP, MotorCmd, signals::MotorCmdWriter,
        validate_active_motor_outputs_with_idle,
    },
};

use crate::foxeer_control::{MotorCommandValidationError, validate_motor_command};

/// Exact number of commands that fit in the golden motor-command queue.
pub const MOTOR_COMMAND_USABLE_CAPACITY: usize = MOTOR_CMD_QUEUE_CAP - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishOutcome {
    Published,
    QueueFull,
    WakeRejected,
}

/// Bounded fail-closed result from the output-inhibited actuator boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InhibitedActuatorOutcome {
    /// An explicit disarm request was accepted and queued commands were discarded.
    Disarmed,
    /// An arming or calibration request was refused in this inhibited build.
    ArmRequestInhibited,
    /// A valid motor command was observed but deliberately not applied.
    CommandInhibited { seq: u32 },
    /// An apply request arrived without a queued motor command.
    MissingCommand,
    /// The newest queued command exceeded the golden freshness limit.
    StaleCommand { seq: u32 },
    /// The newest queued command contained an invalid motor value.
    InvalidCommand { seq: u32 },
    /// More commands were observable than the declared bounded channel permits.
    DrainLimitExceeded,
}

/// Fail-closed reason returned by the physical actuator request adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalActuatorError {
    /// No safety-owned authority report was available.
    MissingAuthority,
    /// The newest safety-owned authority report was stale.
    StaleAuthority,
    /// The authority mode did not permit active output.
    OutputNotArmed,
    /// No control-owned motor command was available.
    MissingCommand,
    /// The newest control-owned command exceeded its freshness bound.
    StaleCommand,
    /// A command contained non-finite or out-of-range motor values.
    InvalidCommand,
    /// More commands were observed than the declared queue can contain.
    DrainLimitExceeded,
}

/// Drains a bounded guard channel and requires fresh safety-owned authority.
pub fn take_latest_guard<Receive>(
    now_us: u32,
    expected: ActuatorAuthority,
    mut receive: Receive,
) -> Result<ActuatorGuardReport, PhysicalActuatorError>
where
    Receive: FnMut() -> Option<ActuatorGuardReport>,
{
    let mut latest = None;
    for _ in 0..MOTOR_COMMAND_USABLE_CAPACITY {
        let Some(report) = receive() else {
            break;
        };
        latest = Some(report);
    }
    if receive().is_some() {
        return Err(PhysicalActuatorError::DrainLimitExceeded);
    }
    let report = latest.ok_or(PhysicalActuatorError::MissingAuthority)?;
    if !report.is_fresh(now_us) {
        return Err(PhysicalActuatorError::StaleAuthority);
    }
    if report.authority != expected {
        return Err(PhysicalActuatorError::OutputNotArmed);
    }
    Ok(report)
}

/// Validates fresh armed authority and returns one bounded physical DShot command.
pub fn take_physical_motor_command<ReceiveGuard, ReceiveCommand>(
    now_us: u32,
    now_ms: u32,
    mut receive_guard: ReceiveGuard,
    mut receive_command: ReceiveCommand,
) -> Result<[u16; 4], PhysicalActuatorError>
where
    ReceiveGuard: FnMut() -> Option<ActuatorGuardReport>,
    ReceiveCommand: FnMut() -> Option<MotorCmd>,
{
    take_latest_guard(now_us, ActuatorAuthority::Armed, &mut receive_guard)?;
    let mut latest = None;
    for _ in 0..MOTOR_COMMAND_USABLE_CAPACITY {
        let Some(command) = receive_command() else {
            break;
        };
        latest = Some(command);
    }
    if receive_command().is_some() {
        return Err(PhysicalActuatorError::DrainLimitExceeded);
    }
    let command = latest.ok_or(PhysicalActuatorError::MissingCommand)?;
    match validate_motor_command(&command, now_ms) {
        Ok(()) => {}
        Err(MotorCommandValidationError::Stale) => {
            return Err(PhysicalActuatorError::StaleCommand);
        }
        Err(MotorCommandValidationError::InvalidValues) => {
            return Err(PhysicalActuatorError::InvalidCommand);
        }
    }
    let outputs = validate_active_motor_outputs_with_idle(command.motors, ESC_IDLE_THROTTLE)
        .map_err(|_| PhysicalActuatorError::InvalidCommand)?;
    Ok(outputs.map(throttle_to_u16))
}

pub fn publish_motor_command<Stamp, Wake>(
    writer: &mut MotorCmdWriter,
    sequence: &mut u32,
    motors: [f32; 4],
    now_ms: u32,
    wake: ActuatorCmd,
    stamp: Stamp,
    wake_actuator: Wake,
) -> PublishOutcome
where
    Stamp: FnOnce(u32, u32) -> u32,
    Wake: FnOnce(ActuatorCmd) -> bool,
{
    let next_sequence = sequence.wrapping_add(1);
    let command = MotorCmd {
        motors,
        seq: next_sequence,
        issued_at_ms: stamp(now_ms, next_sequence),
    };
    if writer.enqueue(command).is_err() {
        return PublishOutcome::QueueFull;
    }
    *sequence = next_sequence;
    if wake_actuator(wake) {
        PublishOutcome::Published
    } else {
        PublishOutcome::WakeRejected
    }
}

/// Drains and validates one bounded actuator wake without granting output authority.
///
/// At most the exact usable queue capacity plus one invariant-checking receive
/// is attempted. Every result remains output-inhibited; the function returns no
/// motor values or arming capability.
pub fn handle_inhibited_actuator_wake<Receive>(
    request: ActuatorCmd,
    now_ms: u32,
    mut receive: Receive,
) -> InhibitedActuatorOutcome
where
    Receive: FnMut() -> Option<MotorCmd>,
{
    let mut latest = None;
    for _ in 0..MOTOR_COMMAND_USABLE_CAPACITY {
        let Some(command) = receive() else {
            break;
        };
        latest = Some(command);
    }
    if receive().is_some() {
        return InhibitedActuatorOutcome::DrainLimitExceeded;
    }

    match request {
        ActuatorCmd::Disarm => InhibitedActuatorOutcome::Disarmed,
        ActuatorCmd::ApplyLatestThrottle => {
            let Some(command) = latest else {
                return InhibitedActuatorOutcome::MissingCommand;
            };
            match validate_motor_command(&command, now_ms) {
                Ok(()) => InhibitedActuatorOutcome::CommandInhibited { seq: command.seq },
                Err(MotorCommandValidationError::Stale) => {
                    InhibitedActuatorOutcome::StaleCommand { seq: command.seq }
                }
                Err(MotorCommandValidationError::InvalidValues) => {
                    InhibitedActuatorOutcome::InvalidCommand { seq: command.seq }
                }
            }
        }
        _ => InhibitedActuatorOutcome::ArmRequestInhibited,
    }
}

#[cfg(test)]
mod tests {
    use ferrowasp_core::safety::{
        ActuatorAuthority, ActuatorGuardReport, ESC_MAX_THROTTLE, MOTOR_CMD_MAX_AGE_MS,
    };

    use super::*;

    fn command(seq: u32, issued_at_ms: u32) -> MotorCmd {
        MotorCmd {
            motors: [100.0, 200.0, 300.0, 400.0],
            seq,
            issued_at_ms,
        }
    }

    fn handle(
        request: ActuatorCmd,
        now_ms: u32,
        commands: impl IntoIterator<Item = MotorCmd>,
    ) -> InhibitedActuatorOutcome {
        let mut commands = commands.into_iter();
        handle_inhibited_actuator_wake(request, now_ms, || commands.next())
    }

    #[test]
    fn valid_latest_command_is_drained_but_remains_inhibited() {
        let outcome = handle(
            ActuatorCmd::ApplyLatestThrottle,
            100,
            [command(1, 90), command(2, 95), command(3, 100)],
        );

        assert_eq!(
            outcome,
            InhibitedActuatorOutcome::CommandInhibited { seq: 3 }
        );
    }

    #[test]
    fn missing_stale_and_invalid_commands_fail_closed() {
        assert_eq!(
            handle(ActuatorCmd::ApplyLatestThrottle, 100, []),
            InhibitedActuatorOutcome::MissingCommand
        );
        assert_eq!(
            handle(
                ActuatorCmd::ApplyLatestThrottle,
                100,
                [command(7, 100 - MOTOR_CMD_MAX_AGE_MS - 1)],
            ),
            InhibitedActuatorOutcome::StaleCommand { seq: 7 }
        );

        let mut invalid = command(8, 100);
        invalid.motors[2] = ESC_MAX_THROTTLE + 1.0;
        assert_eq!(
            handle(ActuatorCmd::ApplyLatestThrottle, 100, [invalid]),
            InhibitedActuatorOutcome::InvalidCommand { seq: 8 }
        );
    }

    #[test]
    fn disarm_and_arm_requests_never_return_motor_values() {
        assert_eq!(
            handle(ActuatorCmd::Disarm, 100, [command(1, 100)]),
            InhibitedActuatorOutcome::Disarmed
        );
        assert_eq!(
            handle(ActuatorCmd::EnterIdle, 100, [command(1, 100)]),
            InhibitedActuatorOutcome::ArmRequestInhibited
        );
        assert_eq!(
            handle(ActuatorCmd::Calibrate, 100, [command(1, 100)]),
            InhibitedActuatorOutcome::ArmRequestInhibited
        );
    }

    #[test]
    fn impossible_over_capacity_input_is_bounded_and_rejected() {
        assert_eq!(
            handle(
                ActuatorCmd::ApplyLatestThrottle,
                100,
                [
                    command(1, 100),
                    command(2, 100),
                    command(3, 100),
                    command(4, 100),
                ],
            ),
            InhibitedActuatorOutcome::DrainLimitExceeded
        );
    }

    #[test]
    fn physical_adapter_requires_fresh_armed_authority_and_latest_command() {
        let guard = ActuatorGuardReport {
            authority: ActuatorAuthority::Armed,
            sequence: 1,
            observed_at_us: 10_000,
        };
        let mut guards = [guard].into_iter();
        let mut commands = [command(1, 9), command(2, 10)].into_iter();
        assert_eq!(
            take_physical_motor_command(10_000, 10, || guards.next(), || commands.next()),
            Ok([100, 200, 300, 400])
        );

        let mut inhibited = [ActuatorGuardReport {
            authority: ActuatorAuthority::Inhibited,
            ..guard
        }]
        .into_iter();
        assert_eq!(
            take_latest_guard(10_000, ActuatorAuthority::Armed, || inhibited.next()),
            Err(PhysicalActuatorError::OutputNotArmed)
        );
    }
}
