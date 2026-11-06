//! The adapter against a register-file model of the SPL06, driven through
//! the real `spl06` driver. The model runs the part's command mode: a
//! measurement command clears its ready flag, and completes it, with new
//! data, only when the test lets it. Expected compensated values are the
//! driver's own formulas, checked in the driver's tests; here the sample
//! values are chosen so they are exact.

use std::cell::Cell;
use std::rc::Rc;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::TimestampMicros;
use ferrowasp_sensor_traits::barometer::{BaroSample, Pressure, Temperature};
use ferrowasp_sensor_traits::class::{Bringup, Clock, Sensor, SensorError, TimeoutStage};
use spl06::{
    ADDRESS, CHIP_ID, COEFFICIENTS_LEN, MEAS_PRESSURE, MEAS_TEMPERATURE, Oversampling,
    PRESSURE_READY, REG_CHIP_ID, REG_COEFFICIENTS, REG_MEAS_CFG, REG_PRESSURE, REG_PRESSURE_CFG,
    REG_TEMPERATURE, REG_TEMPERATURE_CFG, Spl06 as Part, TEMPERATURE_READY,
};

use super::{LIMITS, Spl06, Spl06Timing};

const START_US: u64 = 1_000_000;

/// The model's clock, shared with the bus so a transfer costs time.
#[derive(Clone, Default)]
struct Shared {
    now: Cell<u64>,
    per_transfer_us: Cell<u64>,
}

struct Ticker {
    shared: Rc<Shared>,
}

impl Ticker {
    fn new(shared: &Rc<Shared>) -> Self {
        Self {
            shared: Rc::clone(shared),
        }
    }

    fn advance(&self, micros: u64) {
        self.shared
            .now
            .set(self.shared.now.get().saturating_add(micros));
    }
}

impl Clock for Ticker {
    fn now(&mut self) -> TimestampMicros {
        TimestampMicros(self.shared.now.get())
    }
}

/// The model's controls, shared with the test that drives it: they are
/// set from either side after the model is built.
#[derive(Clone)]
struct Knobs {
    /// Whether a measurement the adapter starts completes.
    completes: Rc<Cell<bool>>,
    /// Whether every transfer fails.
    fail: Rc<Cell<bool>>,
}

impl Knobs {
    fn new() -> Self {
        Self {
            completes: Rc::new(Cell::new(true)),
            fail: Rc::new(Cell::new(false)),
        }
    }
}

/// An SPL06 running its command mode: a measurement command clears its
/// ready flag and, when `completes`, writes new data and sets it again.
struct Device {
    shared: Rc<Shared>,
    knobs: Knobs,
    chip_id: u8,
    coefficients_ready: bool,
    page: [u8; COEFFICIENTS_LEN],
    temperature_raw: i32,
    pressure_raw: i32,
    temperature_ready: Cell<bool>,
    pressure_ready: Cell<bool>,
}

impl Device {
    fn new(shared: &Rc<Shared>, page: &[u8; COEFFICIENTS_LEN], knobs: Knobs) -> Self {
        Self {
            shared: Rc::clone(shared),
            knobs,
            chip_id: CHIP_ID,
            coefficients_ready: true,
            page: *page,
            temperature_raw: 0,
            pressure_raw: 262_144,
            temperature_ready: Cell::new(false),
            pressure_ready: Cell::new(false),
        }
    }

    fn with_chip_id(mut self, chip_id: u8) -> Self {
        self.chip_id = chip_id;
        self
    }

    fn without_coefficients_ready(mut self) -> Self {
        self.coefficients_ready = false;
        self
    }

    /// The part's data registers: a 24-bit two's-complement word, MSB
    /// first.
    fn read_data(raw: i32, bytes: &mut [u8]) {
        if let Some(slot) = bytes.first_mut() {
            *slot = ((raw as u32 >> 16) & 0xff) as u8;
        }
        if let Some(slot) = bytes.get_mut(1) {
            *slot = ((raw as u32 >> 8) & 0xff) as u8;
        }
        if let Some(slot) = bytes.get_mut(2) {
            *slot = (raw as u32 & 0xff) as u8;
        }
    }
}

impl ErrorType for Device {
    type Error = ErrorKind;
}

impl I2c for Device {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        if self.knobs.fail.get() {
            return Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address));
        }
        self.shared.now.set(
            self.shared
                .now
                .get()
                .saturating_add(self.shared.per_transfer_us.get()),
        );
        assert_eq!(address, ADDRESS);
        let (first, rest) = operations.split_at_mut(1);
        let Operation::Write(bytes) = &first[0] else {
            return Err(ErrorKind::Other);
        };
        let read_one = |rest: &mut [Operation<'_>], value: u8| -> Result<(), ErrorKind> {
            let [Operation::Read(read)] = rest else {
                return Err(ErrorKind::Other);
            };
            let Some(slot) = read.first_mut() else {
                return Err(ErrorKind::Other);
            };
            *slot = value;
            Ok(())
        };
        match (bytes[0], rest) {
            (REG_MEAS_CFG, []) => match bytes[1] {
                MEAS_TEMPERATURE => {
                    self.temperature_ready.set(false);
                    if self.knobs.completes.get() {
                        self.temperature_ready.set(true);
                    }
                }
                MEAS_PRESSURE => {
                    self.pressure_ready.set(false);
                    if self.knobs.completes.get() {
                        self.pressure_ready.set(true);
                    }
                }
                _ => {}
            },
            (REG_CHIP_ID, rest) => {
                read_one(rest, self.chip_id)?;
            }
            (REG_MEAS_CFG, rest) => {
                let status = (if self.coefficients_ready { 0xc0 } else { 0 })
                    | (if self.temperature_ready.get() {
                        TEMPERATURE_READY
                    } else {
                        0
                    })
                    | (if self.pressure_ready.get() {
                        PRESSURE_READY
                    } else {
                        0
                    });
                read_one(rest, status)?;
            }
            (REG_COEFFICIENTS, rest) => {
                let [Operation::Read(read)] = rest else {
                    return Err(ErrorKind::Other);
                };
                read.copy_from_slice(&self.page);
            }
            (REG_TEMPERATURE, rest) => {
                let [Operation::Read(read)] = rest else {
                    return Err(ErrorKind::Other);
                };
                Self::read_data(self.temperature_raw, read);
            }
            (REG_PRESSURE_CFG, []) => {}
            (REG_TEMPERATURE_CFG, []) => {}
            (REG_PRESSURE, rest) => {
                let [Operation::Read(read)] = rest else {
                    return Err(ErrorKind::Other);
                };
                Self::read_data(self.pressure_raw, read);
            }
            _ => return Err(ErrorKind::Other),
        }
        Ok(())
    }
}

/// A page whose compensation gives exactly 101 325 Pa at raw pressure
/// 262 144 and 25.0 °C at raw temperature 0, both at 1x: c0 = 50,
/// c1 = 200, c00 = 100 325, c10 = 2000, the rest zero. Packed as the part
/// packs its page.
const PLAUSIBLE: [u8; COEFFICIENTS_LEN] = [
    0x03, 0x20, 0xC8, 0x18, 0x7E, 0x50, 0x07, 0xD0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// A page whose compensation at raw zero reads below the operating range:
/// c00 = -524 288, the rest zero.
const BELOW_RANGE: [u8; COEFFICIENTS_LEN] = [
    0, 0, 0, 0x80, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// An adapter over a model with the given page, its clock handle, and the
/// model's knobs.
fn adapter(
    page: &[u8; COEFFICIENTS_LEN],
    device: impl FnOnce(Device) -> Device,
) -> (Spl06<Device, Ticker>, Ticker, Knobs) {
    let shared = Rc::new(Shared {
        now: Cell::new(START_US),
        per_transfer_us: Cell::new(0),
    });
    let knobs = Knobs::new();
    let model = device(Device::new(&shared, page, knobs.clone()));
    let baro = Spl06::new(
        Part::new(model, ADDRESS),
        Ticker::new(&shared),
        Spl06Timing::default(),
        Oversampling::Times1,
        Oversampling::Times1,
    );
    (baro, Ticker::new(&shared), knobs)
}

impl Ticker {
    fn set_per_transfer(&self, micros: u64) {
        self.shared.per_transfer_us.set(micros);
    }
}

/// Bring the adapter up: the chip ID reads, then the coefficients arrive.
fn bring_up(baro: &mut Spl06<Device, Ticker>) {
    assert_eq!(
        baro.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(START_US)
        })
    );
    assert_eq!(baro.start(), Ok(Bringup::Ready));
}

/// One full measurement cycle: three reads, the third returning the sample.
fn one_cycle(baro: &mut Spl06<Device, Ticker>) -> Result<Stamped<BaroSample>, SensorError> {
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    baro.read()
}

#[test]
fn a_good_sample_is_compensated_stamped_and_numbered() {
    let (mut baro, clock, _knobs) = adapter(&PLAUSIBLE, |device| device);
    bring_up(&mut baro);
    let sample = one_cycle(&mut baro).unwrap();
    assert_eq!(
        sample,
        Stamped {
            seq: 1,
            at: TimestampMicros(START_US),
            value: BaroSample {
                pressure: Pressure::from_pascals(101_325).unwrap(),
                temperature: Temperature::from_celsius_f32(25.0).unwrap(),
            },
        }
    );
    assert_eq!(LIMITS.check(sample.value), Ok(sample.value));

    // The next cycle numbers its sample the next in sequence.
    let next = one_cycle(&mut baro).unwrap();
    assert_eq!(next.seq, 2);
    assert_eq!(next.value, sample.value);
    assert!(clock.shared.now.get() >= START_US);
}

#[test]
fn a_read_before_bring_up_is_refused() {
    let (mut baro, _clock, _knobs) = adapter(&PLAUSIBLE, |device| device);
    assert_eq!(baro.read(), Err(SensorError::NotStarted));
    bring_up(&mut baro);
}

#[test]
fn a_chip_id_other_than_the_spl06s_is_refused() {
    let (mut baro, _clock, _knobs) = adapter(&PLAUSIBLE, |device| device.with_chip_id(0x11));
    assert_eq!(
        baro.start(),
        Err(SensorError::IdentityMismatch { found: 0x11 })
    );
    // The failure stays: every later call reports the same thing.
    assert_eq!(
        baro.start(),
        Err(SensorError::IdentityMismatch { found: 0x11 })
    );
    assert_eq!(baro.read(), Err(SensorError::NotStarted));
}

#[test]
fn bring_up_that_never_completes_times_out() {
    let (mut baro, clock, _knobs) =
        adapter(&PLAUSIBLE, |device| device.without_coefficients_ready());
    assert_eq!(
        baro.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(START_US)
        })
    );
    clock.advance(u64::from(Spl06Timing::DEFAULT_BRINGUP_BUDGET_US) + 1);
    assert_eq!(
        baro.start(),
        Err(SensorError::Timeout(TimeoutStage::Bringup))
    );
    assert_eq!(
        baro.start(),
        Err(SensorError::Timeout(TimeoutStage::Bringup))
    );
}

#[test]
fn time_near_the_end_of_the_clock_fails_closed() {
    // Bring-up that began at the end of time cannot meet its deadline, so
    // it fails closed rather than wrapping fresh.
    let shared = Rc::new(Shared {
        now: Cell::new(u64::MAX - 10),
        per_transfer_us: Cell::new(0),
    });
    let knobs = Knobs::new();
    let model = Device::new(&shared, &PLAUSIBLE, knobs).without_coefficients_ready();
    let clock = Ticker::new(&shared);
    let mut baro = Spl06::new(
        Part::new(model, ADDRESS),
        Ticker::new(&shared),
        Spl06Timing::default(),
        Oversampling::Times1,
        Oversampling::Times1,
    );
    assert_eq!(
        baro.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(u64::MAX - 10)
        })
    );
    // The deadline saturates at the end of time rather than wrapping, so
    // it is still pending just before it, and expires exactly at it.
    assert_eq!(
        baro.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(u64::MAX - 10)
        })
    );
    clock.advance(10);
    assert_eq!(
        baro.start(),
        Err(SensorError::Timeout(TimeoutStage::Bringup))
    );
}

#[test]
fn without_a_new_result_a_read_is_never_the_previous_sample() {
    let (mut baro, clock, knobs) = adapter(&PLAUSIBLE, |device| device);
    bring_up(&mut baro);
    let first = one_cycle(&mut baro).unwrap();
    assert_eq!(first.seq, 1);

    // The part stops completing measurements: every read refuses a value,
    // never the previous one, and past the stall limit it is a timeout.
    knobs.completes.set(false);
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    clock.advance(u64::from(Spl06Timing::DEFAULT_STALL_LIMIT_US) + 1);
    assert_eq!(
        baro.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
    assert_eq!(
        baro.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
}

#[test]
fn a_read_that_overruns_its_budget_is_a_timeout_and_its_result_discarded() {
    let (mut baro, clock, _knobs) = adapter(&PLAUSIBLE, |device| device);
    bring_up(&mut baro);
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));

    // The pressure is ready, but the transfers the read needs outlast its
    // budget: the sample it would return is discarded.
    clock.set_per_transfer(3_000);
    assert_eq!(baro.read(), Err(SensorError::Timeout(TimeoutStage::Read)));

    // The next read begins a new cycle, so the discarded sample never
    // arrives as a later read's result.
    clock.set_per_transfer(0);
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    let sample = baro.read().unwrap();
    assert_eq!(sample.seq, 1);
    assert_eq!(sample.at, TimestampMicros(clock.shared.now.get()));
}

#[test]
fn a_value_outside_the_operating_range_is_refused() {
    let (mut baro, _clock, _knobs) = adapter(&BELOW_RANGE, |device| device);
    bring_up(&mut baro);
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
    assert_eq!(baro.read(), Err(SensorError::OutOfRange));
    // The next cycle starts over; the refusal did not consume the sample
    // numbering.
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
}

#[test]
fn a_bus_error_at_any_step_is_a_bus_error_and_never_a_sample() {
    let (mut baro, _clock, knobs) = adapter(&PLAUSIBLE, |device| device);
    knobs.fail.set(true);
    assert_eq!(baro.start(), Err(SensorError::Bus));
    assert_eq!(baro.start(), Err(SensorError::Bus));

    // A running sensor reports a failed transfer the same way, and never
    // a sample from it.
    let (mut baro, _clock, knobs) = adapter(&PLAUSIBLE, |device| device);
    bring_up(&mut baro);
    knobs.fail.set(true);
    assert_eq!(baro.read(), Err(SensorError::Bus));
    knobs.fail.set(false);
    assert_eq!(baro.read(), Err(SensorError::NoNewSample));
}
