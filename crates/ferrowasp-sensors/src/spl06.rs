//! The Goertek SPL06-007 barometer, through the external `spl06` driver, as
//! a [`Barometer`](ferrowasp_sensor_traits::barometer::Barometer).
//!
//! The driver handles registers, coefficients and compensation; this
//! adapter adds what the barometer class asks and the driver does not
//! give. It confirms the chip ID, bounds bring-up and every read by its own
//! clock, numbers and stamps each new measurement, refuses values outside
//! the part's operating range, and reports a missing result as an error,
//! never as the last value again.
//!
//! **How it runs.** The driver is blocking I2C in the part's command mode:
//! the adapter starts one temperature measurement, waits for its ready bit,
//! then one pressure measurement, and a sample exists only once both are
//! read. Every measurement is one the adapter started, so a sample can
//! never be an old value the part repeats. It is meant for a low-priority
//! task that owns it: the task calls [`Sensor::start`] until it is ready,
//! then [`Sensor::read`] about twice per measurement period. Neither call
//! waits for the sensor; each makes at most a few short transfers. The
//! adapter cannot interrupt a transfer, so the bus implementation must
//! bound each one; a call that overruns its budget is reported as a
//! timeout, and its result discarded.
//!
//! **Bring-up is bounded by the adapter's clock.** A part that never
//! reports its coefficients ready within the bring-up budget fails
//! closed, and a step is never scheduled past the clock's deadline.
//!
//! **Freshness.** Between a cycle's completed measurements the adapter
//! returns `NoNewSample`; past the stall limit,
//! `Timeout(Measurement)`. The stall limit must cover a whole cycle at
//! the configured oversampling, several times over.
//!
//! Requirements: SENSOR-016, SENSOR-017, SENSOR-018, SENSOR-019, SENSOR-020

#[cfg(test)]
mod tests;

use embedded_hal::i2c::I2c;
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::{Deadline, TimestampMicros};
use ferrowasp_sensor_traits::barometer::{BaroLimits, BaroSample, Pressure, Temperature};
use ferrowasp_sensor_traits::class::{
    Bringup, Clock, MeasurementSequence, Sensor, SensorError, TimeoutStage, check_identity,
};
use spl06::{CHIP_ID, Coefficients, Oversampling, Spl06 as Part};

/// The SPL06-007's operating range (datasheet, recalled and not checked
/// against a copy): 300 to 1200 hPa, and -40 to 85 °C.
pub const LIMITS: BaroLimits = BaroLimits {
    pressure_min: Pressure::from_centipascals(3_000_000),
    pressure_max: Pressure::from_centipascals(12_000_000),
    temperature_min: Temperature::from_centidegrees_celsius(-4_000),
    temperature_max: Temperature::from_centidegrees_celsius(8_500),
};

/// The adapter's time limits, in microseconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Spl06Timing {
    /// From the first `start` to running.
    pub bringup_budget_us: u32,
    /// One `read`, from before its first transfer to after its last.
    pub read_budget_us: u32,
    /// The longest time without a new measurement before `read` reports a
    /// timeout instead of no new sample.
    pub stall_limit_us: u32,
}

impl Spl06Timing {
    /// The part flags its coefficients ready within tens of milliseconds of
    /// power-up (inferred); this leaves several times that.
    pub const DEFAULT_BRINGUP_BUDGET_US: u32 = 250_000;
    /// A read is at most three short transfers: well under a millisecond at
    /// 100 kHz, and this leaves several times that.
    pub const DEFAULT_READ_BUDGET_US: u32 = 5_000;
    /// A whole cycle at 1x oversampling takes tens of milliseconds
    /// (inferred); this leaves many of them.
    pub const DEFAULT_STALL_LIMIT_US: u32 = 1_000_000;

    pub const fn default() -> Self {
        Self {
            bringup_budget_us: Self::DEFAULT_BRINGUP_BUDGET_US,
            read_budget_us: Self::DEFAULT_READ_BUDGET_US,
            stall_limit_us: Self::DEFAULT_STALL_LIMIT_US,
        }
    }
}

impl Default for Spl06Timing {
    fn default() -> Self {
        Self::default()
    }
}

/// The measurement cycle the adapter drives, one step per read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cycle {
    /// Start a temperature measurement.
    StartTemperature,
    /// Wait for the temperature, then start the pressure measurement.
    AwaitTemperature,
    /// Wait for the pressure, then read it and return the sample. The
    /// temperature raw value read earlier rides along here.
    AwaitPressure { temperature_raw: i32 },
}

enum State<I2C> {
    Created(Part<I2C>),
    /// The chip ID read and matched; the part has not yet flagged its
    /// coefficients ready. `deadline` bounds the whole bring-up.
    Waiting {
        part: Part<I2C>,
        deadline: Deadline,
    },
    Running(Running<I2C>),
    /// Bring-up failed; the error is kept so every later call reports it.
    Failed(Part<I2C>, SensorError),
    Vacant,
}

/// A part that is measuring: the coefficients its compensation needs and
/// where the measurement cycle stands.
struct Running<I2C> {
    part: Part<I2C>,
    coefficients: Coefficients,
    cycle: Cycle,
    last_new_at: TimestampMicros,
}

/// An SPL06-007 as a [`Barometer`], timed by the caller's clock.
pub struct Spl06<I2C, C> {
    state: State<I2C>,
    clock: C,
    timing: Spl06Timing,
    sequence: MeasurementSequence,
    pressure_oversampling: Oversampling,
    temperature_oversampling: Oversampling,
}

impl<I2C, C> Spl06<I2C, C> {
    /// The part at one address, with the oversampling both measurements run
    /// at and the timing the adapter bounds itself by.
    pub const fn new(
        part: Part<I2C>,
        clock: C,
        timing: Spl06Timing,
        pressure_oversampling: Oversampling,
        temperature_oversampling: Oversampling,
    ) -> Self {
        Self {
            state: State::Created(part),
            clock,
            timing,
            sequence: MeasurementSequence::new(),
            pressure_oversampling,
            temperature_oversampling,
        }
    }
}

const BRINGUP_TIMEOUT: SensorError = SensorError::Timeout(TimeoutStage::Bringup);

impl<I2C, C> Spl06<I2C, C>
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

    /// One bring-up step: read the chip ID and compare it with the one the
    /// part is expected to answer with.
    fn begin(&mut self, mut part: Part<I2C>) -> Result<Bringup, SensorError> {
        let now = self.clock.now();
        match part.read_chip_id() {
            Ok(found) => match check_identity(CHIP_ID, found) {
                Ok(()) => {
                    self.state = State::Waiting {
                        part,
                        deadline: Deadline::new(now, self.timing.bringup_budget_us),
                    };
                    Ok(Bringup::Pending { retry_at: now })
                }
                Err(error) => self.fail(part, error),
            },
            Err(_) => self.fail(part, SensorError::Bus),
        }
    }

    /// One wait step: the part flags its coefficients ready or it does
    /// not; once ready they are read and the measurements configured.
    fn wait_for_coefficients(
        &mut self,
        mut part: Part<I2C>,
        deadline: Deadline,
    ) -> Result<Bringup, SensorError> {
        let now = self.clock.now();
        if deadline.is_expired_at(now) {
            return self.fail(part, BRINGUP_TIMEOUT);
        }
        let status = match part.read_status() {
            Ok(status) => status,
            Err(_) => return self.fail(part, SensorError::Bus),
        };
        if !(status.coefficients_ready && status.sensor_ready) {
            self.state = State::Waiting { part, deadline };
            return Ok(Bringup::Pending { retry_at: now });
        }
        let coefficients = match part.read_coefficients() {
            Ok(coefficients) => coefficients,
            Err(_) => return self.fail(part, SensorError::Bus),
        };
        match part.configure(self.pressure_oversampling, self.temperature_oversampling) {
            Ok(()) => {
                self.state = State::Running(Running {
                    part,
                    coefficients,
                    cycle: Cycle::StartTemperature,
                    last_new_at: now,
                });
                Ok(Bringup::Ready)
            }
            Err(_) => self.fail(part, SensorError::Bus),
        }
    }
}

impl<I2C, C> Sensor for Spl06<I2C, C>
where
    I2C: I2c,
    C: Clock,
{
    type Sample = BaroSample;

    fn start(&mut self) -> Result<Bringup, SensorError> {
        match core::mem::replace(&mut self.state, State::Vacant) {
            State::Created(part) => self.begin(part),
            State::Waiting { part, deadline } => self.wait_for_coefficients(part, deadline),
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

    fn read(&mut self) -> Result<Stamped<BaroSample>, SensorError> {
        let State::Running(running) = &mut self.state else {
            return Err(SensorError::NotStarted);
        };
        let began = self.clock.now();
        let bus = |_| SensorError::Bus;
        let outcome = match running.cycle {
            Cycle::StartTemperature => {
                running.part.start_temperature_measurement().map_err(bus)?;
                running.cycle = Cycle::AwaitTemperature;
                None
            }
            Cycle::AwaitTemperature => {
                let status = running.part.read_status().map_err(bus)?;
                if !status.temperature_ready {
                    None
                } else {
                    let temperature_raw = running.part.read_temperature_raw().map_err(bus)?;
                    running.part.start_pressure_measurement().map_err(bus)?;
                    running.cycle = Cycle::AwaitPressure { temperature_raw };
                    None
                }
            }
            Cycle::AwaitPressure { temperature_raw } => {
                let status = running.part.read_status().map_err(bus)?;
                if !status.pressure_ready {
                    None
                } else {
                    let pressure_raw = running.part.read_pressure_raw().map_err(bus)?;
                    running.cycle = Cycle::StartTemperature;
                    Some((pressure_raw, temperature_raw))
                }
            }
        };
        let ended = self.clock.now();
        if Deadline::new(began, self.timing.read_budget_us).is_expired_at(ended) {
            return Err(SensorError::Timeout(TimeoutStage::Read));
        }
        let Some((pressure_raw, temperature_raw)) = outcome else {
            let stalled = Deadline::new(running.last_new_at, self.timing.stall_limit_us);
            return Err(if stalled.is_expired_at(ended) {
                SensorError::Timeout(TimeoutStage::Measurement)
            } else {
                SensorError::NoNewSample
            });
        };
        let pressure = running
            .coefficients
            .pressure_pascals(
                pressure_raw,
                self.pressure_oversampling,
                temperature_raw,
                self.temperature_oversampling,
            )
            .ok_or(SensorError::OutOfRange)?;
        let temperature = running
            .coefficients
            .temperature_celsius(temperature_raw, self.temperature_oversampling)
            .ok_or(SensorError::OutOfRange)?;
        let value = LIMITS.sample_from_f32(pressure, temperature)?;
        running.last_new_at = ended;
        Ok(Stamped {
            seq: self.sequence.advance(),
            at: ended,
            value,
        })
    }
}
