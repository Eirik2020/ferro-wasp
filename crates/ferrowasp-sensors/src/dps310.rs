//! The Infineon DPS310 barometer, through the external `uf-dps3xx` driver,
//! as a [`Barometer`](ferrowasp_sensor_traits::barometer::Barometer).
//!
//! The driver handles registers, calibration and the part's temperature
//! workaround; this adapter adds what the barometer class asks and the
//! driver does not give. It confirms the product ID in full, bounds
//! bring-up and every read by its own clock, numbers and stamps each new
//! measurement, refuses values outside the part's operating range, and
//! reports a missing result as an error, never as the last value again.
//!
//! **How it runs.** The driver is blocking I2C. The adapter is meant for a
//! low-priority task that owns it over an `embedded-hal-bus` I2C device:
//! the task calls [`Sensor::start`] until it is ready, waiting until each
//! `retry_at` in between, then calls [`Sensor::read`] about twice per
//! measurement period. Neither call waits for the sensor; each makes at
//! most a few short transfers. The adapter cannot interrupt a transfer, so
//! the bus implementation must bound each one; a call that overruns its
//! budget is reported as a timeout, and its result discarded.
//!
//! **Bring-up is bounded twice.** By the adapter's clock, which counts bus
//! time as well as waits; and by the driver's own rule, applied to the waits
//! the driver asks for, since the adapter drives the driver's `poll` and not
//! its blocking init. Each wait the driver asks for is at least 10 ms, so the
//! second bound caps bring-up at one step per 10 ms of budget, whatever the
//! clock reads. A step is never scheduled past the clock's deadline.
//!
//! **Freshness.** The driver returns a result only while the part flags both
//! a new pressure and a new temperature result, and reading the result
//! clears those flags (inferred from the DPS310 datasheet's description of
//! `PRS_RDY` and `TMP_RDY`). Between results the adapter returns
//! `NoNewSample`; past the stall limit, `Timeout(Measurement)`.
//!
//! Requirements: SENSOR-010, SENSOR-011, SENSOR-012, SENSOR-013, SENSOR-014

#[cfg(test)]
mod tests;

use embedded_hal::i2c::I2c;
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::{Deadline, TimestampMicros};
use ferrowasp_sensor_traits::barometer::{BaroLimits, BaroSample, Pressure, Temperature};
use ferrowasp_sensor_traits::class::{
    Bringup, Clock, MeasurementSequence, Sensor, SensorError, TimeoutStage, check_identity,
};
use uf_dps3xx::{Config, Dps3xx, Error as DriverError, I2cAddress, Init, Poll, Ready, Uninit};

/// The whole product ID register, revision and product nibbles both
/// (DPS310 datasheet, recalled and not checked against a copy: 0x10). The
/// driver compares only the upper nibble.
pub const PRODUCT_ID: u8 = 0x10;

/// The DPS310's operating range (datasheet, recalled and not checked
/// against a copy): 300 to 1200 hPa, and -40 to 85 °C.
pub const LIMITS: BaroLimits = BaroLimits {
    pressure_min: Pressure::from_centipascals(3_000_000),
    pressure_max: Pressure::from_centipascals(12_000_000),
    temperature_min: Temperature::from_centidegrees_celsius(-4_000),
    temperature_max: Temperature::from_centidegrees_celsius(8_500),
};

/// The adapter's time limits, in microseconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dps310Timing {
    /// From the first `start` to running. Also given to the driver as its
    /// own init timeout, in whole milliseconds.
    pub bringup_budget_us: u32,
    /// One `read`, from before its first transfer to after its last.
    pub read_budget_us: u32,
    /// The longest time without a new measurement before `read` reports a
    /// timeout instead of no new sample.
    pub stall_limit_us: u32,
}

impl Dps310Timing {
    /// The part flags its trim and its coefficients ready within tens of
    /// milliseconds of power-up (inferred); this leaves several times that.
    pub const DEFAULT_BRINGUP_BUDGET_US: u32 = 250_000;
    /// A read is two transfers, eight bytes in all: about 1 ms at 100 kHz.
    pub const DEFAULT_READ_BUDGET_US: u32 = 5_000;
    /// Measurement periods without a result before the part counts as
    /// stalled: one late result is tolerated, two in a row are not.
    pub const STALL_PERIODS: u64 = 3;

    /// The defaults, with the stall limit from the configuration's slower
    /// measurement rate.
    pub const fn for_config(config: &Config) -> Self {
        let stall_us = (config.measurement_period_ms() as u64)
            .saturating_mul(1_000)
            .saturating_mul(Self::STALL_PERIODS);
        Self {
            bringup_budget_us: Self::DEFAULT_BRINGUP_BUDGET_US,
            read_budget_us: Self::DEFAULT_READ_BUDGET_US,
            stall_limit_us: if stall_us > u32::MAX as u64 {
                u32::MAX
            } else {
                stall_us as u32
            },
        }
    }
}

struct Starting<I2C> {
    init: Init<I2C>,
    deadline: Deadline,
    /// What is left of the driver's own budget for the waits it asks for.
    remaining_wait_ms: u32,
    retry_at: TimestampMicros,
}

struct Running<I2C> {
    dps: Dps3xx<I2C, Ready>,
    /// When the last valid measurement was read, or bring-up finished.
    last_new_at: TimestampMicros,
}

enum State<I2C> {
    Created(Dps3xx<I2C, Uninit>),
    Starting(Starting<I2C>),
    Running(Running<I2C>),
    /// Bring-up failed: the error is returned again, and the bus is held for
    /// `release`.
    Failed {
        i2c: Option<I2C>,
        error: SensorError,
    },
    /// Only while a transition holds the state.
    Vacant,
}

/// A DPS310 on I2C, timed by `C`.
pub struct Dps310<I2C, C> {
    state: State<I2C>,
    clock: C,
    timing: Dps310Timing,
    sequence: MeasurementSequence,
}

impl<I2C, C> Dps310<I2C, C>
where
    I2C: I2c,
    C: Clock,
{
    /// The adapter, before any transfer. `config`'s init timeout is replaced
    /// by the bring-up budget. A configuration the part cannot run is
    /// `SensorError::Configuration`; the driver drops the bus handle then.
    pub fn new(
        i2c: I2C,
        address: I2cAddress,
        config: Config,
        timing: Dps310Timing,
        clock: C,
    ) -> Result<Self, SensorError> {
        let config = config.init_timeout_ms(timing.bringup_budget_us / 1_000);
        let dps = Dps3xx::new_i2c(i2c, address, config).map_err(driver_error)?;
        Ok(Self {
            state: State::Created(dps),
            clock,
            timing,
            sequence: MeasurementSequence::new(),
        })
    }

    /// Give the bus back, in whatever state the adapter is.
    pub fn release(self) -> Option<I2C> {
        match self.state {
            State::Created(dps) => Some(dps.release()),
            State::Starting(starting) => release_init(starting.init),
            State::Running(running) => Some(running.dps.release()),
            State::Failed { i2c, .. } => i2c,
            State::Vacant => None,
        }
    }

    fn fail(&mut self, i2c: Option<I2C>, error: SensorError) -> Result<Bringup, SensorError> {
        self.state = State::Failed { i2c, error };
        Err(error)
    }

    fn begin(&mut self, mut dps: Dps3xx<I2C, Uninit>) -> Result<Bringup, SensorError> {
        let started = self.clock.now();
        let identity = dps
            .product_id()
            .map_err(driver_error)
            .and_then(|id| check_identity(PRODUCT_ID, id));
        if let Err(error) = identity {
            return self.fail(Some(dps.release()), error);
        }
        let init = match dps.init() {
            Ok(init) => init,
            Err(transition) => {
                let (error, dps) = transition.into_parts();
                return self.fail(Some(dps.release()), driver_error(error));
            }
        };
        self.step(Starting {
            init,
            deadline: Deadline::new(started, self.timing.bringup_budget_us),
            remaining_wait_ms: self.timing.bringup_budget_us / 1_000,
            retry_at: started,
        })
    }

    fn step(&mut self, mut starting: Starting<I2C>) -> Result<Bringup, SensorError> {
        let now = self.clock.now();
        if starting.deadline.is_expired_at(now) {
            return self.fail(release_init(starting.init), BRINGUP_TIMEOUT);
        }
        if now < starting.retry_at {
            let retry_at = starting.retry_at;
            self.state = State::Starting(starting);
            return Ok(Bringup::Pending { retry_at });
        }
        match starting.init.poll() {
            Ok(Poll::Pending { wait_ms }) => {
                let Some(remaining) = starting.remaining_wait_ms.checked_sub(wait_ms) else {
                    return self.fail(release_init(starting.init), BRINGUP_TIMEOUT);
                };
                let after = self.clock.now();
                if starting.deadline.is_expired_at(after) {
                    return self.fail(release_init(starting.init), BRINGUP_TIMEOUT);
                }
                // Never later than the deadline, so a caller that waits as
                // told learns of a timeout when it happens.
                let wanted = TimestampMicros(after.0.saturating_add(u64::from(wait_ms) * 1_000));
                let retry_at = wanted.min(starting.deadline.expires_at());
                starting.remaining_wait_ms = remaining;
                starting.retry_at = retry_at;
                self.state = State::Starting(starting);
                Ok(Bringup::Pending { retry_at })
            }
            Ok(Poll::Ready(mut dps)) => {
                if let Err(error) = dps.start_background() {
                    return self.fail(Some(dps.release()), driver_error(error));
                }
                let after = self.clock.now();
                if starting.deadline.is_expired_at(after) {
                    return self.fail(Some(dps.release()), BRINGUP_TIMEOUT);
                }
                self.state = State::Running(Running {
                    dps,
                    last_new_at: after,
                });
                Ok(Bringup::Ready)
            }
            Err(error) => self.fail(release_init(starting.init), driver_error(error)),
        }
    }
}

impl<I2C, C> Sensor for Dps310<I2C, C>
where
    I2C: I2c,
    C: Clock,
{
    type Sample = BaroSample;

    fn start(&mut self) -> Result<Bringup, SensorError> {
        match core::mem::replace(&mut self.state, State::Vacant) {
            State::Created(dps) => self.begin(dps),
            State::Starting(starting) => self.step(starting),
            State::Running(running) => {
                self.state = State::Running(running);
                Ok(Bringup::Ready)
            }
            State::Failed { i2c, error } => self.fail(i2c, error),
            State::Vacant => Err(SensorError::NotStarted),
        }
    }

    fn read(&mut self) -> Result<Stamped<BaroSample>, SensorError> {
        let State::Running(running) = &mut self.state else {
            return Err(SensorError::NotStarted);
        };
        let began = self.clock.now();
        let result = running.dps.try_read_sample();
        let ended = self.clock.now();
        let fresh = result.map_err(driver_error)?;
        if Deadline::new(began, self.timing.read_budget_us).is_expired_at(ended) {
            return Err(SensorError::Timeout(TimeoutStage::Read));
        }
        let Some(fresh) = fresh else {
            let stalled = Deadline::new(running.last_new_at, self.timing.stall_limit_us);
            return Err(if stalled.is_expired_at(ended) {
                SensorError::Timeout(TimeoutStage::Measurement)
            } else {
                SensorError::NoNewSample
            });
        };
        let value = LIMITS.sample_from_f32(fresh.pressure_pa, fresh.temperature_c)?;
        running.last_new_at = ended;
        Ok(Stamped {
            seq: self.sequence.advance(),
            at: ended,
            value,
        })
    }
}

const BRINGUP_TIMEOUT: SensorError = SensorError::Timeout(TimeoutStage::Bringup);

fn release_init<I2C>(init: Init<I2C>) -> Option<I2C> {
    init.cancel().map(Dps3xx::release)
}

/// The driver's errors as the class's kinds, one for one where the class
/// has one.
fn driver_error<E>(error: DriverError<E>) -> SensorError {
    match error {
        DriverError::Bus(_) => SensorError::Bus,
        DriverError::InvalidDevice { id } => SensorError::IdentityMismatch { found: id },
        DriverError::InvalidConfig { .. } => SensorError::Configuration,
        DriverError::Timeout { .. } => BRINGUP_TIMEOUT,
        DriverError::NoSampleReady => SensorError::NoNewSample,
        DriverError::InitConsumed => SensorError::NotStarted,
    }
}
