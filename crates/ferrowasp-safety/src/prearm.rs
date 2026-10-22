//! The control loop's IMU status, as each pre-arm check receives it over its
//! own `state` link (section 9). A check keeps the newest status and uses it
//! only while it is younger than the age limit: nothing else yet tells the
//! check that the control loop is still running, so a status that stops
//! arriving reads as no status at all.
//!
//! Requirements: SAFETY-005, SAFETY-013
//! Safety-critical.

use crate::{ArmingAbortReason, PreArmHealth};
use ferrowasp_io_contracts::link::{LinkConsumer, Stamped};
use ferrowasp_io_contracts::time::TimestampMicros;

/// How old the control loop's status may be when a pre-arm check uses it:
/// twenty control ticks at 2 kHz, ten at the 1 ms monotonic's resolution.
pub const IMU_PREARM_STATUS_MAX_AGE_US: u64 = 10_000;

/// What the control loop knows of the IMU on one tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImuPreArmStatus {
    /// The IMU's bus answered and configured at boot.
    pub transport_ready: bool,
    /// The detected IMU, as the board numbers its kinds; 0 for none.
    pub kind: u8,
    /// At least one sample has reached the control loop since boot.
    pub sample_seen: bool,
    pub bias_calibrated: bool,
    /// Stale beyond the one tick the control loop rides through.
    pub stale: bool,
}

/// A pre-arm check's end of its link from the control loop.
pub struct ImuPreArmReader<'a> {
    link: LinkConsumer<'a, ImuPreArmStatus>,
    latest: Option<Stamped<ImuPreArmStatus>>,
}

impl<'a> ImuPreArmReader<'a> {
    pub fn new(link: LinkConsumer<'a, ImuPreArmStatus>) -> Self {
        Self { link, latest: None }
    }

    /// The newest status, if one arrived within the age limit of `now`;
    /// `None` before the first, and once they stop. Never blocks.
    pub fn current(&mut self, now: TimestampMicros) -> Option<ImuPreArmStatus> {
        if let Some(newest) = self.link.newest() {
            self.latest = Some(newest);
        }
        self.latest
            .filter(|status| now.0.saturating_sub(status.at.0) <= IMU_PREARM_STATUS_MAX_AGE_US)
            .map(|status| status.value)
    }
}

/// The pre-arm health the control loop's status gives: `status` as
/// [`ImuPreArmReader::current`] returned it, refused as
/// [`ArmingAbortReason::ControlStatusStale`] when there is none. Whether the
/// detected IMU kind may fly is the board's, `kind_verified`; `force_stale`
/// is the fault-injection image's.
pub fn prearm_health(
    status: Option<ImuPreArmStatus>,
    host_connected: bool,
    kind_verified: impl FnOnce(u8) -> bool,
    force_stale: bool,
) -> Result<PreArmHealth, ArmingAbortReason> {
    let status = status.ok_or(ArmingAbortReason::ControlStatusStale)?;
    Ok(PreArmHealth {
        host_connected,
        imu_ready: status.transport_ready && kind_verified(status.kind) && status.sample_seen,
        imu_bias_calibrated: status.bias_calibrated,
        imu_fresh: !force_stale && !status.stale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_io_contracts::link::link;
    use heapless::spsc::Queue;

    fn status() -> ImuPreArmStatus {
        ImuPreArmStatus {
            transport_ready: true,
            kind: 2,
            sample_seen: true,
            bias_calibrated: true,
            stale: false,
        }
    }

    #[test]
    fn no_status_refuses_arming_and_a_status_gives_its_health() {
        assert_eq!(
            prearm_health(None, false, |_| true, false),
            Err(ArmingAbortReason::ControlStatusStale)
        );
        let health = prearm_health(Some(status()), false, |kind| kind == 2, false).unwrap();
        assert_eq!(crate::validate_prearm_health(health), Ok(()));
    }

    #[test]
    fn each_status_field_maps_to_its_refusal() {
        let refusal = |status, kind_ok: bool, force_stale| {
            crate::validate_prearm_health(
                prearm_health(Some(status), false, |_| kind_ok, force_stale).unwrap(),
            )
        };
        let unavailable = Err(ArmingAbortReason::ImuUnavailable);
        assert_eq!(
            refusal(
                ImuPreArmStatus {
                    transport_ready: false,
                    ..status()
                },
                true,
                false
            ),
            unavailable
        );
        assert_eq!(refusal(status(), false, false), unavailable);
        assert_eq!(
            refusal(
                ImuPreArmStatus {
                    sample_seen: false,
                    ..status()
                },
                true,
                false
            ),
            unavailable
        );
        assert_eq!(
            refusal(
                ImuPreArmStatus {
                    bias_calibrated: false,
                    ..status()
                },
                true,
                false
            ),
            Err(ArmingAbortReason::ImuBiasUncalibrated)
        );
        assert_eq!(
            refusal(
                ImuPreArmStatus {
                    stale: true,
                    ..status()
                },
                true,
                false
            ),
            Err(ArmingAbortReason::ImuStale)
        );
        assert_eq!(
            refusal(status(), true, true),
            Err(ArmingAbortReason::ImuStale)
        );
    }

    #[test]
    fn no_status_before_the_first_arrives() {
        let mut queue = Queue::<Stamped<ImuPreArmStatus>, 4>::new();
        let (_producer, consumer) = link(&mut queue);
        let mut reader = ImuPreArmReader::new(consumer);

        assert_eq!(reader.current(TimestampMicros(0)), None);
    }

    #[test]
    fn the_newest_status_stands_until_it_is_too_old() {
        let mut queue = Queue::<Stamped<ImuPreArmStatus>, 4>::new();
        let (mut producer, consumer) = link(&mut queue);
        let mut reader = ImuPreArmReader::new(consumer);

        producer.push(
            TimestampMicros(1_000),
            ImuPreArmStatus {
                stale: true,
                ..status()
            },
        );
        producer.push(TimestampMicros(2_000), status());

        assert_eq!(reader.current(TimestampMicros(2_500)), Some(status()));
        // Nothing new: the last one still stands, until the limit.
        assert_eq!(
            reader.current(TimestampMicros(2_000 + IMU_PREARM_STATUS_MAX_AGE_US)),
            Some(status())
        );
        assert_eq!(
            reader.current(TimestampMicros(2_001 + IMU_PREARM_STATUS_MAX_AGE_US)),
            None
        );
    }
}
