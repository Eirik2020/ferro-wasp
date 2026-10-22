//! Reusable actuator-command queue publication.
//!
//! Requirements: SAFETY-001, SAFETY-006, SAFETY-007, SAFETY-008
//! Safety-critical.

use crate::{ActuatorCmd, MotorCmd, signals::MotorCmdWriter};

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
    now_ms: u64,
    wake: ActuatorCmd,
    stamp: Stamp,
    wake_actuator: Wake,
) -> PublishOutcome
where
    Stamp: FnOnce(u64, u32) -> u64,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::{self, MotorCmdQueue};

    fn leaked_queue() -> &'static mut MotorCmdQueue {
        std::boxed::Box::leak(std::boxed::Box::new(MotorCmdQueue::new()))
    }

    #[test]
    fn a_published_command_carries_the_next_sequence_and_wakes_the_actuator() {
        let (mut writer, mut reader) = signals::split_motor_cmd_queue(leaked_queue());
        let mut sequence = 7;
        let outcome = publish_motor_command(
            &mut writer,
            &mut sequence,
            [100.0; 4],
            1_000,
            ActuatorCmd::ApplyLatestThrottle,
            |now, seq| now + u64::from(seq),
            |wake| wake == ActuatorCmd::ApplyLatestThrottle,
        );
        assert_eq!(outcome, PublishOutcome::Published);
        assert_eq!(sequence, 8);
        let command = reader.take_latest().unwrap();
        assert_eq!((command.seq, command.issued_at_ms), (8, 1_008));
    }

    #[test]
    fn a_full_queue_publishes_nothing_and_keeps_the_sequence() {
        let (mut writer, _reader) = signals::split_motor_cmd_queue(leaked_queue());
        let mut sequence = 0;
        let publish = |writer: &mut _, sequence: &mut u32| {
            publish_motor_command(
                writer,
                sequence,
                [0.0; 4],
                0,
                ActuatorCmd::ApplyLatestThrottle,
                |now, _| now,
                |_| true,
            )
        };
        while publish(&mut writer, &mut sequence) == PublishOutcome::Published {}
        let full_at = sequence;
        assert_eq!(
            publish(&mut writer, &mut sequence),
            PublishOutcome::QueueFull
        );
        assert_eq!(
            sequence, full_at,
            "a refused command never advances the sequence"
        );
    }

    #[test]
    fn a_rejected_wake_is_reported_after_the_command_is_queued() {
        let (mut writer, mut reader) = signals::split_motor_cmd_queue(leaked_queue());
        let mut sequence = 0;
        let outcome = publish_motor_command(
            &mut writer,
            &mut sequence,
            [0.0; 4],
            0,
            ActuatorCmd::ApplyLatestThrottle,
            |now, _| now,
            |_| false,
        );
        assert_eq!(outcome, PublishOutcome::WakeRejected);
        assert!(
            reader.take_latest().is_some(),
            "the command still waits in the queue"
        );
    }
}
