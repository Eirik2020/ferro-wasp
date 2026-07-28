//! Reusable actuator-command queue publication.

use ferrowasp_core::safety::{ActuatorCmd, MotorCmd, signals::MotorCmdWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishOutcome {
    Published,
    QueueFull,
    WakeRejected,
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
