//! What every sensor class shares: the error kinds, the bring-up step, the
//! clock an implementation times itself by, the sequence it numbers its
//! measurements with, and how a consumer judges a measurement's age.
//!
//! The envelope is `ferrowasp_io_contracts::link::Stamped`: `at` is when the
//! sensor's implementation took the measurement, by its clock, and `seq` is
//! the sensor's own count of the measurements it returned. A measurement goes
//! onto a link with `LinkProducer::forward`, which keeps both, so a consumer
//! reads a gap in `seq` as measurements it never received.
//!
//! Requirements: SENSOR-001, SENSOR-002, SENSOR-009
//! Safety-critical.

use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::{Deadline, TimestampMicros};

/// Why a sensor returned no measurement. Each kind is distinct, so no fault
/// can pass as another, and none carries a value: an error never hands back
/// an earlier measurement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SensorError {
    /// A bus transfer failed, or delivered fewer bytes than a sample.
    Bus,
    /// The sensor has not completed a measurement since the last one
    /// returned. Normal between measurements; a consumer's age check turns a
    /// run of these into staleness.
    NoNewSample,
    /// A value outside what the part can report, or a pattern only a stuck
    /// bus produces.
    OutOfRange,
    /// An operation passed its time limit.
    Timeout(TimeoutStage),
    /// The part answered with an identity other than the one expected.
    IdentityMismatch { found: u8 },
    /// The configuration asked of the part is one it cannot run.
    Configuration,
    /// The sensor is not running: bring-up has not finished, or it failed.
    NotStarted,
}

/// Which time limit a [`SensorError::Timeout`] passed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutStage {
    /// Bring-up did not finish within its budget.
    Bringup,
    /// One read took longer than its budget, so its result's timing is not
    /// known well enough to use.
    Read,
    /// No new measurement arrived within the sensor's stall limit.
    Measurement,
}

/// One step of a sensor's bring-up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bringup {
    /// Call `start` again at or after `retry_at`; calling earlier does nothing.
    Pending { retry_at: TimestampMicros },
    /// The sensor is measuring; `read` may be called.
    Ready,
}

/// The time an implementation reads to stamp, bound and pace itself.
pub trait Clock {
    fn now(&mut self) -> TimestampMicros;
}

/// A sensor read by polling: bring it up step by step, then read it.
///
/// Neither call waits for the sensor. `start` returns when the next step
/// needs time to pass, and `read` returns `SensorError::NoNewSample` when no
/// new measurement is ready, so a caller paces both and never blocks on
/// them. Each call still makes bus transfers, which take as long as the bus
/// implementation lets them.
pub trait Sensor {
    type Sample;

    /// Advance bring-up by one step.
    fn start(&mut self) -> Result<Bringup, SensorError>;

    /// The measurement completed since the last one returned, stamped with
    /// when it was taken and the next sequence number; otherwise the reason
    /// there is none.
    fn read(&mut self) -> Result<Stamped<Self::Sample>, SensorError>;
}

/// The sequence a sensor numbers its measurements with: 1 first, one more
/// for each, and 0 skipped when it wraps, so 0 means nothing measured yet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MeasurementSequence {
    last: u32,
}

impl MeasurementSequence {
    pub const fn new() -> Self {
        Self { last: 0 }
    }

    /// The number for the next measurement.
    pub fn advance(&mut self) -> u32 {
        self.last = match self.last.wrapping_add(1) {
            0 => 1,
            next => next,
        };
        self.last
    }

    /// The number the last measurement took; 0 before any.
    pub const fn last(self) -> u32 {
        self.last
    }
}

/// How a consumer judges a measurement it holds, at its own time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Freshness {
    /// Younger than the limit.
    Fresh,
    /// As old as the limit or older: not to be used.
    Stale,
    /// Stamped after the consumer's time, which no true measurement can be:
    /// a clock or stamping fault, and not to be used either.
    Future,
}

impl Freshness {
    /// The age limit is a `Deadline` from the measurement's stamp, so a
    /// measurement is stale exactly when that deadline has expired.
    pub const fn judge(
        measured_at: TimestampMicros,
        now: TimestampMicros,
        max_age_us: u32,
    ) -> Self {
        if measured_at.0 > now.0 {
            Self::Future
        } else if Deadline::new(measured_at, max_age_us).is_expired_at(now) {
            Self::Stale
        } else {
            Self::Fresh
        }
    }

    pub const fn of<T>(measurement: &Stamped<T>, now: TimestampMicros, max_age_us: u32) -> Self {
        Self::judge(measurement.at, now, max_age_us)
    }

    pub const fn is_fresh(self) -> bool {
        matches!(self, Self::Fresh)
    }
}

/// Compare the identity a part reports with the one its driver expects.
pub const fn check_identity(expected: u8, found: u8) -> Result<(), SensorError> {
    if found == expected {
        Ok(())
    } else {
        Err(SensorError::IdentityMismatch { found })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_measurement_is_stale_at_its_age_limit_and_future_when_stamped_ahead() {
        let at = TimestampMicros(10_000);
        assert_eq!(
            Freshness::judge(at, TimestampMicros(10_000), 500),
            Freshness::Fresh
        );
        assert_eq!(
            Freshness::judge(at, TimestampMicros(10_499), 500),
            Freshness::Fresh
        );
        assert_eq!(
            Freshness::judge(at, TimestampMicros(10_500), 500),
            Freshness::Stale
        );
        assert_eq!(
            Freshness::judge(at, TimestampMicros(9_999), 500),
            Freshness::Future
        );
        assert_eq!(
            Freshness::judge(at, TimestampMicros(10_000), 0),
            Freshness::Stale
        );

        let stamped = Stamped {
            seq: 1,
            at,
            value: (),
        };
        assert!(Freshness::of(&stamped, TimestampMicros(10_100), 500).is_fresh());
        assert!(!Freshness::of(&stamped, TimestampMicros(11_000), 500).is_fresh());
    }

    #[test]
    fn a_measurement_stamped_near_the_end_of_time_never_wraps_fresh() {
        let at = TimestampMicros(u64::MAX - 10);
        assert_eq!(
            Freshness::judge(at, TimestampMicros(u64::MAX - 5), 500),
            Freshness::Fresh
        );
        assert_eq!(
            Freshness::judge(at, TimestampMicros(u64::MAX), 500),
            Freshness::Stale
        );
        assert_eq!(
            Freshness::judge(TimestampMicros(0), TimestampMicros(u64::MAX), u32::MAX),
            Freshness::Stale
        );
    }

    #[test]
    fn measurement_sequences_start_at_one_and_skip_zero_when_they_wrap() {
        let mut sequence = MeasurementSequence::new();
        assert_eq!(sequence.last(), 0);
        assert_eq!(sequence.advance(), 1);
        assert_eq!(sequence.advance(), 2);
        assert_eq!(sequence.last(), 2);

        let mut wrapping = MeasurementSequence { last: u32::MAX - 1 };
        assert_eq!(wrapping.advance(), u32::MAX);
        assert_eq!(wrapping.advance(), 1);
    }

    #[test]
    fn an_identity_other_than_the_expected_one_is_a_mismatch() {
        assert_eq!(check_identity(0x10, 0x10), Ok(()));
        assert_eq!(
            check_identity(0x10, 0x11),
            Err(SensorError::IdentityMismatch { found: 0x11 })
        );
        assert_eq!(
            check_identity(0x47, 0x00),
            Err(SensorError::IdentityMismatch { found: 0x00 })
        );
    }
}
