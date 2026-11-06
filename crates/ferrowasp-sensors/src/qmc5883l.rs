//! The QST QMC5883L compass, through the external `qmc5883l` driver, as
//! a [`Compass`](ferrowasp_sensor_traits::compass::Compass).
//!
//! The driver handles registers and scaling; this adapter adds what the
//! compass class asks and the driver does not give. It confirms the part's
//! ID and that it stands by after a reset, bounds bring-up and every read
//! by its own clock, numbers and stamps each new field, refuses a field
//! the part flagged overflowed or one outside the configured range, and
//! reports a missing result as an error, never as the last value again.
//!
//! **How it runs.** The driver is blocking I2C over a part in continuous
//! measurement: bring-up resets the part, waits the reset takes, then
//! checks its identity and its standby state, and configures it. A read
//! asks the part's status and takes the field only when the part flags a
//! new one, so a value the part did not flag new can never pass as a
//! fresh measurement (inferred from the part's datasheet: reading the
//! data registers consumes the ready flag). Neither call waits for the
//! sensor; each makes at most two short transfers. A call that overruns
//! its budget is reported as a timeout, and its result discarded.
//!
//! Requirements: SENSOR-021, SENSOR-022, SENSOR-023, SENSOR-024, SENSOR-025

#[cfg(test)]
mod tests;

use embedded_hal::i2c::I2c;
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::{Deadline, TimestampMicros};
use ferrowasp_sensor_traits::class::{
    Bringup, Clock, MeasurementSequence, Sensor, SensorError, TimeoutStage, check_identity,
};
use ferrowasp_sensor_traits::compass::{CompassLimits, CompassSample, Nanotesla};
use qmc5883l::{ID, OutputDataRate, Oversampling, Qmc5883l as Part, Range, STANDBY};

/// The adapter's time limits, in microseconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Qmc5883lTiming {
    /// From the first `start` to running.
    pub bringup_budget_us: u32,
    /// The reset's own wait, before the part answers its ID (iNav's
    /// `deviceDetect` delays 30 ms after writing the reset).
    pub reset_wait_us: u32,
    /// One `read`, from before its first transfer to after its last.
    pub read_budget_us: u32,
    /// The longest time without a new field before `read` reports a
    /// timeout instead of no new sample.
    pub stall_limit_us: u32,
}

impl Qmc5883lTiming {
    pub const DEFAULT_BRINGUP_BUDGET_US: u32 = 250_000;
    /// iNav's `deviceDetect` delays 30 ms after the reset.
    pub const DEFAULT_RESET_WAIT_US: u32 = 30_000;
    pub const DEFAULT_READ_BUDGET_US: u32 = 5_000;
    /// The part's slowest configured rate, 10 Hz, gives a field every
    /// 100 ms (inferred); this leaves many of them.
    pub const DEFAULT_STALL_LIMIT_US: u32 = 1_000_000;

    pub const fn default() -> Self {
        Self {
            bringup_budget_us: Self::DEFAULT_BRINGUP_BUDGET_US,
            reset_wait_us: Self::DEFAULT_RESET_WAIT_US,
            read_budget_us: Self::DEFAULT_READ_BUDGET_US,
            stall_limit_us: Self::DEFAULT_STALL_LIMIT_US,
        }
    }
}

impl Default for Qmc5883lTiming {
    fn default() -> Self {
        Self::default()
    }
}

/// The limits a part at the given range can be believed within: its
/// measuring range, inclusive, in nanotesla.
pub const fn limits(range: Range) -> CompassLimits {
    let gauss = match range {
        Range::Gauss2 => 200_000,
        Range::Gauss8 => 800_000,
    };
    CompassLimits {
        min: Nanotesla::from_nanotesla(-gauss),
        max: Nanotesla::from_nanotesla(gauss),
    }
}

enum State<I2C> {
    Created(Part<I2C>),
    /// The soft reset is written; the part has not yet been asked its ID.
    Resetting {
        part: Part<I2C>,
        deadline: Deadline,
        retry_at: TimestampMicros,
    },
    Running(Running<I2C>),
    /// Bring-up failed; the error is kept so every later call reports it.
    Failed(Part<I2C>, SensorError),
    Vacant,
}

/// A part that is measuring: its limits and when it last gave a field.
struct Running<I2C> {
    part: Part<I2C>,
    limits: CompassLimits,
    last_new_at: TimestampMicros,
}

/// A QMC5883L as a [`Compass`], timed by the caller's clock.
pub struct Qmc5883l<I2C, C> {
    state: State<I2C>,
    clock: C,
    timing: Qmc5883lTiming,
    sequence: MeasurementSequence,
    range: Range,
    rate: OutputDataRate,
    oversampling: Oversampling,
}

impl<I2C, C> Qmc5883l<I2C, C> {
    /// The part at one address, configured at the given range, rate and
    /// oversampling, timed by the caller's clock.
    pub const fn new(
        part: Part<I2C>,
        clock: C,
        timing: Qmc5883lTiming,
        range: Range,
        rate: OutputDataRate,
        oversampling: Oversampling,
    ) -> Self {
        Self {
            state: State::Created(part),
            clock,
            timing,
            sequence: MeasurementSequence::new(),
            range,
            rate,
            oversampling,
        }
    }
}

const BRINGUP_TIMEOUT: SensorError = SensorError::Timeout(TimeoutStage::Bringup);

impl<I2C, C> Qmc5883l<I2C, C>
where
    I2C: I2c,
    C: Clock,
{
    /// Latch a bring-up failure, keeping the part so every later call
    /// reports the same error.
    fn fail(&mut self, part: Part<I2C>, error: SensorError) -> Result<Bringup, SensorError> {
        self.state = State::Failed(part, error);
        Err(error)
    }

    /// One bring-up step: reset the part and wait the reset takes.
    fn begin(&mut self, mut part: Part<I2C>) -> Result<Bringup, SensorError> {
        let now = self.clock.now();
        if part.soft_reset().is_err() {
            return self.fail(part, SensorError::Bus);
        }
        let retry_at = TimestampMicros(now.0.saturating_add(self.timing.reset_wait_us as u64));
        self.state = State::Resetting {
            part,
            deadline: Deadline::new(now, self.timing.bringup_budget_us),
            retry_at,
        };
        Ok(Bringup::Pending { retry_at })
    }

    /// One wait step: once the reset has had its time, check the part's
    /// ID and its standby state, then configure it.
    fn wait_after_reset(
        &mut self,
        mut part: Part<I2C>,
        deadline: Deadline,
        retry_at: TimestampMicros,
    ) -> Result<Bringup, SensorError> {
        let now = self.clock.now();
        if now.0 < retry_at.0 {
            self.state = State::Resetting {
                part,
                deadline,
                retry_at,
            };
            return Ok(Bringup::Pending { retry_at });
        }
        if deadline.is_expired_at(now) {
            return self.fail(part, BRINGUP_TIMEOUT);
        }
        let found = match part.read_id() {
            Ok(found) => found,
            Err(_) => return self.fail(part, SensorError::Bus),
        };
        if let Err(error) = check_identity(ID, found) {
            return self.fail(part, error);
        }
        // A part that just reset answers standby in its control register;
        // anything else there is not a QMC5883L behaving like one, and
        // the found value tells a wrongly populated board from a stuck
        // bus (iNav's `deviceDetect` checks the same pair).
        let conf1 = match part.read_conf1() {
            Ok(conf1) => conf1,
            Err(_) => return self.fail(part, SensorError::Bus),
        };
        if conf1 != STANDBY {
            return self.fail(part, SensorError::IdentityMismatch { found: conf1 });
        }
        if part.set_reset_period().is_err() {
            return self.fail(part, SensorError::Bus);
        }
        if part
            .configure(self.range, self.rate, self.oversampling)
            .is_err()
        {
            return self.fail(part, SensorError::Bus);
        }
        self.state = State::Running(Running {
            part,
            limits: limits(self.range),
            last_new_at: now,
        });
        Ok(Bringup::Ready)
    }
}

impl<I2C, C> Sensor for Qmc5883l<I2C, C>
where
    I2C: I2c,
    C: Clock,
{
    type Sample = CompassSample;

    fn start(&mut self) -> Result<Bringup, SensorError> {
        match core::mem::replace(&mut self.state, State::Vacant) {
            State::Created(part) => self.begin(part),
            State::Resetting {
                part,
                deadline,
                retry_at,
            } => self.wait_after_reset(part, deadline, retry_at),
            State::Running(running) => {
                self.state = State::Running(running);
                Ok(Bringup::Ready)
            }
            State::Failed(part, error) => {
                self.state = State::Failed(part, error);
                Err(error)
            }
            State::Vacant => Err(SensorError::NotStarted),
        }
    }

    fn read(&mut self) -> Result<Stamped<CompassSample>, SensorError> {
        let State::Running(running) = &mut self.state else {
            return Err(SensorError::NotStarted);
        };
        let began = self.clock.now();
        let status = running.part.read_status().map_err(|_| SensorError::Bus)?;
        if !status.data_ready {
            let stalled = Deadline::new(running.last_new_at, self.timing.stall_limit_us);
            let ended = self.clock.now();
            return Err(if stalled.is_expired_at(ended) {
                SensorError::Timeout(TimeoutStage::Measurement)
            } else {
                SensorError::NoNewSample
            });
        }
        // The part flagged a field, but one it could not fit in its range
        // is not a measurement to scale.
        if status.overflow {
            return Err(SensorError::OutOfRange);
        }
        let field = running.part.read_field().map_err(|_| SensorError::Bus)?;
        let ended = self.clock.now();
        if Deadline::new(began, self.timing.read_budget_us).is_expired_at(ended) {
            return Err(SensorError::Timeout(TimeoutStage::Read));
        }
        let microtesla =
            qmc5883l::field_microtesla(field, self.range).ok_or(SensorError::OutOfRange)?;
        let value = running.limits.sample_from_f32(microtesla)?;
        running.last_new_at = ended;
        Ok(Stamped {
            seq: self.sequence.advance(),
            at: ended,
            value,
        })
    }
}
