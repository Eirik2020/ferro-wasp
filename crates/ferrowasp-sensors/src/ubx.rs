//! A u-blox receiver's `NAV-PVT` solutions, through the external `ubx`
//! driver's deframer, as a [`Gnss`](ferrowasp_sensor_traits::gnss::Gnss).
//!
//! The driver deframes and decodes; this adapter adds what the GNSS class
//! asks. One `read` drains what the port has, skips frames of other
//! messages and frames that do not decode, and returns the first
//! navigation solution it finds, numbered and stamped by the adapter's
//! clock. A solution a receiver did not mark valid becomes the class's
//! no-fix, whose fields carry no position to misread; a value outside its
//! physical range is refused, not clamped. The adapter reports a missing
//! solution as an error, never as the last one again.
//!
//! **How it runs.** The port is non-blocking over `embedded-io`, and the
//! adapter holds it and the driver's deframer itself, so a read knows
//! when the stream is drained — the deframer holds nothing and the port
//! has nothing ready — rather than guessing from a reader that cannot
//! say. A burst larger than the read budget is abandoned as a timeout,
//! its remaining frames left for the next read to drain. A port that
//! reports data and then yields none is a broken stream, reported as a
//! bus error.
//!
//! Requirements: SENSOR-026, SENSOR-027, SENSOR-028

#[cfg(test)]
mod tests;

use embedded_io::{Read, ReadReady};
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::{Deadline, TimestampMicros};
use ferrowasp_sensor_traits::class::{
    Bringup, Clock, MeasurementSequence, Sensor, SensorError, TimeoutStage,
};
use ferrowasp_sensor_traits::gnss::{
    AccuracyDegrees1e5, AccuracyMillimetres, AccuracyMillimetresPerSecond, Dop, GnssFix,
    GnssSolution, Millimetres, MillimetresPerSecond, Position, ReportedFix, UtcTime, UtcValidity,
    Velocity,
};
use ubx::{Deframer, Event, FixType, NavPvt};

/// The adapter's time limits, in microseconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UbxTiming {
    /// One `read`, from before its first byte to after its last.
    pub read_budget_us: u32,
    /// The longest time without a new solution before `read` reports a
    /// timeout instead of no new sample.
    pub stall_limit_us: u32,
}

impl UbxTiming {
    /// A read drains a burst of frames, a few hundred bytes: well under
    /// ten milliseconds at 115 200 baud, and this leaves several times
    /// that.
    pub const DEFAULT_READ_BUDGET_US: u32 = 10_000;
    /// A receiver that sends NAV-PVT at 5 Hz gives a solution every
    /// 200 ms (inferred); this leaves five of them.
    pub const DEFAULT_STALL_LIMIT_US: u32 = 1_000_000;

    pub const fn default() -> Self {
        Self {
            read_budget_us: Self::DEFAULT_READ_BUDGET_US,
            stall_limit_us: Self::DEFAULT_STALL_LIMIT_US,
        }
    }
}

impl Default for UbxTiming {
    fn default() -> Self {
        Self::default()
    }
}

/// A receiver's `NAV-PVT` stream as a [`Gnss`], timed by the caller's
/// clock.
pub struct UbxNavPvt<R, const BUF: usize, const CHUNK: usize, C> {
    port: R,
    deframer: Deframer<BUF>,
    chunk: [u8; CHUNK],
    /// The first byte of `chunk` not yet pushed into the deframer.
    chunk_next: usize,
    /// How many bytes of `chunk` the last take from the port holds.
    chunk_len: usize,
    clock: C,
    timing: UbxTiming,
    sequence: MeasurementSequence,
    last_new_at: Option<TimestampMicros>,
}

impl<R, const BUF: usize, const CHUNK: usize, C> UbxNavPvt<R, BUF, CHUNK, C> {
    /// One serial port, timed by the caller's clock. `BUF` must hold a
    /// whole frame plus its 8 bytes of overhead, and `CHUNK` is the most
    /// one read takes from the port at a time.
    pub const fn new(port: R, clock: C, timing: UbxTiming) -> Self {
        Self {
            port,
            deframer: Deframer::new(),
            chunk: [0; CHUNK],
            chunk_next: 0,
            chunk_len: 0,
            clock,
            timing,
            sequence: MeasurementSequence::new(),
            last_new_at: None,
        }
    }
}

/// The fix a receiver reports, as the class names it. A value the protocol
/// does not define has no name the class carries, so it reads as no fix.
fn reported(fix: FixType) -> ReportedFix {
    match fix {
        FixType::NoFix | FixType::Other(_) => ReportedFix::NoFix,
        FixType::DeadReckoningOnly => ReportedFix::DeadReckoningOnly,
        FixType::Fix2D => ReportedFix::TwoD,
        FixType::Fix3D => ReportedFix::ThreeD,
        FixType::GnssDeadReckoning => ReportedFix::GnssDeadReckoning,
        FixType::TimeOnly => ReportedFix::TimeOnly,
    }
}

/// One `NAV-PVT` as the class's solution: the fix the receiver's own
/// flags gate, the UTC time its own validity carries, and the values the
/// class's constructors range-check. The satellites used ride along.
fn solution(pvt: &NavPvt) -> Result<GnssSolution, SensorError> {
    let fix = GnssFix::from_report(reported(pvt.fix_type), pvt.fix().is_some(), || {
        let raw = pvt.solution_unchecked();
        Ok((
            Position::new(
                raw.lat_e7_deg,
                raw.lon_e7_deg,
                Millimetres(raw.height_mm),
                Millimetres(raw.height_msl_mm),
                AccuracyMillimetres(raw.h_acc_mm),
                AccuracyMillimetres(raw.v_acc_mm),
            )?,
            Velocity::new(
                MillimetresPerSecond(raw.vel_n_mm_s),
                MillimetresPerSecond(raw.vel_e_mm_s),
                MillimetresPerSecond(raw.vel_d_mm_s),
                MillimetresPerSecond(raw.g_speed_mm_s),
                raw.head_mot_e5_deg,
                AccuracyMillimetresPerSecond(raw.s_acc_mm_s),
                AccuracyDegrees1e5(raw.head_acc_e5_deg),
            )?,
        ))
    })?;
    let utc = {
        let time = pvt.utc_unchecked();
        let validity = pvt.time_validity;
        UtcTime::from_report(
            UtcValidity {
                date: validity.date,
                time: validity.time,
                fully_resolved: validity.fully_resolved,
            },
            time.year,
            time.month,
            time.day,
            time.hour,
            time.minute,
            time.second,
            time.nano_ns,
        )?
    };
    Ok(GnssSolution {
        fix,
        satellites_used: pvt.num_sv,
        pdop: Dop(pvt.solution_unchecked().p_dop_e2),
        utc,
    })
}

/// A frame or deframing error as a solution, if it is one: a frame of
/// another message, or one that does not decode, is skipped, and the
/// deframer resynchronizes on its own.
fn take(event: Event<'_>) -> Result<Option<GnssSolution>, SensorError> {
    match event {
        Event::Frame(frame) => {
            if let Ok(pvt) = NavPvt::from_frame(&frame) {
                Ok(Some(solution(&pvt)?))
            } else {
                Ok(None)
            }
        }
        Event::Error(_) => Ok(None),
    }
}

impl<R, const BUF: usize, const CHUNK: usize, C> Sensor for UbxNavPvt<R, BUF, CHUNK, C>
where
    R: Read + ReadReady,
    C: Clock,
{
    type Sample = GnssSolution;

    fn start(&mut self) -> Result<Bringup, SensorError> {
        // The stream is live the moment the port exists; the first start
        // begins the stall clock.
        if self.last_new_at.is_none() {
            self.last_new_at = Some(self.clock.now());
        }
        Ok(Bringup::Ready)
    }

    fn read(&mut self) -> Result<Stamped<GnssSolution>, SensorError> {
        let Some(last_new_at) = self.last_new_at else {
            return Err(SensorError::NotStarted);
        };
        let began = self.clock.now();
        let mut found = None;
        'drain: loop {
            // The bytes the last chunk took still holds, then the
            // deframer's own buffer.
            while self.chunk_next < self.chunk_len {
                let byte = self.chunk[self.chunk_next];
                self.chunk_next += 1;
                if let Some(event) = self.deframer.push(byte)
                    && let Some(solved) = take(event)?
                {
                    found = Some(solved);
                    break 'drain;
                }
            }
            while let Some(event) = self.deframer.poll() {
                if let Some(solved) = take(event)? {
                    found = Some(solved);
                    break 'drain;
                }
            }
            // Nothing held: take more bytes if the port has them, and
            // stop once it does not: the stream is drained.
            if !self.port.read_ready().map_err(|_| SensorError::Bus)? {
                break;
            }
            let taken = self
                .port
                .read(&mut self.chunk)
                .map_err(|_| SensorError::Bus)?;
            if taken == 0 {
                return Err(SensorError::Bus);
            }
            self.chunk_next = 0;
            self.chunk_len = taken;
            if Deadline::new(began, self.timing.read_budget_us).is_expired_at(self.clock.now()) {
                return Err(SensorError::Timeout(TimeoutStage::Read));
            }
        }
        let ended = self.clock.now();
        if Deadline::new(began, self.timing.read_budget_us).is_expired_at(ended) {
            return Err(SensorError::Timeout(TimeoutStage::Read));
        }
        let Some(value) = found else {
            let stalled = Deadline::new(last_new_at, self.timing.stall_limit_us);
            return Err(if stalled.is_expired_at(ended) {
                SensorError::Timeout(TimeoutStage::Measurement)
            } else {
                SensorError::NoNewSample
            });
        };
        self.last_new_at = Some(ended);
        Ok(Stamped {
            seq: self.sequence.advance(),
            at: ended,
            value,
        })
    }
}
