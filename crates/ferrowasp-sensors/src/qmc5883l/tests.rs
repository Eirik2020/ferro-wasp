//! The adapter against a register-file model of the QMC5883L, driven
//! through the real `qmc5883l` driver. The model does what the part does:
//! it answers its ID, stands by after a reset, flags a field when the
//! test lets it, and the data read consumes the ready flag.

use std::cell::Cell;
use std::rc::Rc;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::TimestampMicros;
use ferrowasp_sensor_traits::class::{Bringup, Clock, Sensor, SensorError, TimeoutStage};
use ferrowasp_sensor_traits::compass::{CompassSample, Nanotesla};
use qmc5883l::{
    ADDRESS, DRDY, ID, OutputDataRate, Oversampling, Qmc5883l as Part, REG_CONF1, REG_CONF2,
    REG_DATA, REG_ID, REG_SET_RESET, REG_STATUS, Range, SET_RESET_PERIOD, SOFT_RST, STANDBY,
};

use super::{Qmc5883l, Qmc5883lTiming};

const START_US: u64 = 1_000_000;

/// The model's clock, shared with the bus so a transfer costs time.
#[derive(Clone, Default)]
struct Shared {
    now: Cell<u64>,
    per_transfer_us: Cell<u64>,
}

/// The model's controls, shared with the test that drives it.
#[derive(Clone)]
struct Knobs {
    fail: Rc<Cell<bool>>,
    /// Whether the part flags a new field.
    data_ready: Rc<Cell<bool>>,
    /// Whether the part flags the field overflowed.
    overflow: Rc<Cell<bool>>,
    /// Every register the part saw written, with its value.
    writes: Rc<std::cell::RefCell<std::vec::Vec<(u8, u8)>>>,
}

impl Knobs {
    fn new() -> Self {
        Self {
            fail: Rc::new(Cell::new(false)),
            data_ready: Rc::new(Cell::new(false)),
            overflow: Rc::new(Cell::new(false)),
            writes: Rc::new(std::cell::RefCell::new(std::vec::Vec::new())),
        }
    }
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

    fn set_per_transfer(&self, micros: u64) {
        self.shared.per_transfer_us.set(micros);
    }
}

impl Clock for Ticker {
    fn now(&mut self) -> TimestampMicros {
        TimestampMicros(self.shared.now.get())
    }
}

/// A QMC5883L as a plain register file whose status flags the test holds.
struct Device {
    shared: Rc<Shared>,
    knobs: Knobs,
    registers: [u8; 16],
}

impl Device {
    fn new(shared: &Rc<Shared>, knobs: Knobs) -> Self {
        let mut registers = [0_u8; 16];
        registers[REG_ID as usize] = ID;
        registers[REG_CONF1 as usize] = STANDBY;
        Self {
            shared: Rc::clone(shared),
            knobs,
            registers,
        }
    }

    fn with_field(&mut self, field: [i16; 3]) {
        for (component, at) in field.into_iter().zip([0, 2, 4]) {
            let [low, high] = component.to_le_bytes();
            self.registers[at] = low;
            self.registers[at + 1] = high;
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
        let register = bytes[0];
        match (register, rest) {
            (register, []) => {
                if let Some(byte) = bytes.get(1).copied() {
                    self.registers[register as usize] = byte;
                    self.knobs.writes.borrow_mut().push((register, byte));
                }
                Ok(())
            }
            (REG_STATUS, rest) => {
                let [Operation::Read(read)] = rest else {
                    return Err(ErrorKind::Other);
                };
                let Some(slot) = read.first_mut() else {
                    return Err(ErrorKind::Other);
                };
                *slot = (if self.knobs.data_ready.get() { DRDY } else { 0 })
                    | (if self.knobs.overflow.get() { 0x02 } else { 0 });
                Ok(())
            }
            (REG_DATA, rest) => {
                let [Operation::Read(read)] = rest else {
                    return Err(ErrorKind::Other);
                };
                let registers = self.registers;
                let field = &registers[REG_DATA as usize..REG_DATA as usize + 6];
                read.copy_from_slice(field);
                // Reading the data registers consumes the ready flag.
                self.knobs.data_ready.set(false);
                Ok(())
            }
            (register, rest) => {
                let [Operation::Read(read)] = rest else {
                    return Err(ErrorKind::Other);
                };
                for (at, slot) in read.iter_mut().enumerate() {
                    *slot = self.registers[register as usize + at];
                }
                Ok(())
            }
        }
    }
}

/// An adapter over a model, its clock handle, and the model's knobs. The
/// model may be reshaped before it is handed to the adapter.
fn adapter(
    model: impl FnOnce(Device) -> Device,
) -> (Qmc5883l<Device, Ticker>, Ticker, Knobs, Rc<Shared>) {
    let shared = Rc::new(Shared {
        now: Cell::new(START_US),
        per_transfer_us: Cell::new(0),
    });
    let knobs = Knobs::new();
    let device = model(Device::new(&shared, knobs.clone()));
    let compass = Qmc5883l::new(
        Part::new(device, ADDRESS),
        Ticker::new(&shared),
        Qmc5883lTiming::default(),
        Range::Gauss8,
        OutputDataRate::Hz100,
        Oversampling::Times512,
    );
    (compass, Ticker::new(&shared), knobs, shared)
}

/// Bring the adapter up: the reset writes, its wait passes, the part
/// identifies and is configured.
fn bring_up(compass: &mut Qmc5883l<Device, Ticker>, clock: &Ticker) {
    assert_eq!(
        compass.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(START_US + u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US))
        })
    );
    clock.advance(u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US));
    assert_eq!(compass.start(), Ok(Bringup::Ready));
}

#[test]
fn bring_up_resets_identifies_and_configures_the_part() {
    let (mut compass, clock, knobs, _shared) = adapter(|device| device);
    assert_eq!(compass.read(), Err(SensorError::NotStarted));
    bring_up(&mut compass, &clock);
    // The reset, the set/reset period, then the configuration the part was
    // asked for: continuous at ±8 gauss, 100 Hz, 512x oversampling.
    assert_eq!(
        &*knobs.writes.borrow(),
        &[
            (REG_CONF2, SOFT_RST),
            (REG_SET_RESET, SET_RESET_PERIOD),
            (REG_CONF1, 0x01 | 0x10 | 0x08),
        ]
    );
    // A part already running answers `start` with ready again.
    assert_eq!(compass.start(), Ok(Bringup::Ready));
}

#[test]
fn a_good_field_is_scaled_stamped_and_numbered() {
    let (mut compass, clock, knobs, _shared) = adapter(|mut device| {
        device.with_field([3_000, -6_000, 0]);
        device
    });
    bring_up(&mut compass, &clock);

    knobs.data_ready.set(true);
    let sample = compass.read().unwrap();
    assert_eq!(
        sample,
        Stamped {
            seq: 1,
            at: TimestampMicros(START_US + u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US)),
            value: CompassSample {
                x: Nanotesla::from_nanotesla(100_000),
                y: Nanotesla::from_nanotesla(-200_000),
                z: Nanotesla::from_nanotesla(0),
            },
        }
    );

    // The part flags another field, and the sample numbering follows.
    knobs.data_ready.set(true);
    let next = compass.read().unwrap();
    assert_eq!(next.seq, 2);
    assert_eq!(next.value, sample.value);

    // ±8 gauss is the configured range the part is believed within.
    assert_eq!(
        super::limits(Range::Gauss8),
        ferrowasp_sensor_traits::compass::CompassLimits {
            min: Nanotesla::from_nanotesla(-800_000),
            max: Nanotesla::from_nanotesla(800_000),
        }
    );
}

#[test]
fn without_a_new_field_a_read_is_never_the_previous_sample() {
    let (mut compass, clock, knobs, _shared) = adapter(|mut device| {
        device.with_field([3_000, -6_000, 0]);
        device
    });
    bring_up(&mut compass, &clock);

    // One good sample, then the part stops flagging fields: every read
    // refuses a value, never the previous one, and past the stall limit it
    // is a timeout.
    knobs.data_ready.set(true);
    let first = compass.read().unwrap();
    assert_eq!(first.seq, 1);
    assert_eq!(compass.read(), Err(SensorError::NoNewSample));
    assert_eq!(compass.read(), Err(SensorError::NoNewSample));
    clock.advance(u64::from(Qmc5883lTiming::DEFAULT_STALL_LIMIT_US) + 1);
    assert_eq!(
        compass.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
    assert_eq!(
        compass.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
}

#[test]
fn a_wrong_id_or_unreset_part_is_refused() {
    let (mut compass, clock, _knobs, _shared) = adapter(|mut device| {
        device.registers[REG_ID as usize] = 0x00;
        device
    });
    assert_eq!(
        compass.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(START_US + u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US))
        })
    );
    clock.advance(u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US));
    assert_eq!(
        compass.start(),
        Err(SensorError::IdentityMismatch { found: 0x00 })
    );
    assert_eq!(
        compass.start(),
        Err(SensorError::IdentityMismatch { found: 0x00 })
    );

    // A part whose control register does not answer standby after a reset
    // is not a QMC5883L behaving like one, whatever its ID.
    let (mut compass, clock, _knobs, _shared) = adapter(|mut device| {
        device.registers[REG_CONF1 as usize] = 0x2d;
        device
    });
    assert_eq!(
        compass.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(START_US + u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US))
        })
    );
    clock.advance(u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US));
    assert_eq!(
        compass.start(),
        Err(SensorError::IdentityMismatch { found: 0x2d })
    );
}

#[test]
fn bring_up_that_never_completes_times_out() {
    let (mut compass, clock, _knobs, _shared) = adapter(|device| device);
    assert_eq!(
        compass.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(START_US + u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US))
        })
    );
    // The wait for the part outlasts the whole bring-up budget.
    clock.advance(u64::from(Qmc5883lTiming::DEFAULT_BRINGUP_BUDGET_US) + 1);
    assert_eq!(
        compass.start(),
        Err(SensorError::Timeout(TimeoutStage::Bringup))
    );
    assert_eq!(
        compass.start(),
        Err(SensorError::Timeout(TimeoutStage::Bringup))
    );
}

#[test]
fn an_overflowed_or_out_of_range_field_is_refused() {
    let (mut compass, clock, knobs, _shared) = adapter(|mut device| {
        device.with_field([3_000, 3_000, 3_000]);
        device
    });
    bring_up(&mut compass, &clock);

    // The part flags a field it could not fit in its range: refused
    // without scaling it, and the ready flag is not consumed for a sample.
    knobs.data_ready.set(true);
    knobs.overflow.set(true);
    assert_eq!(compass.read(), Err(SensorError::OutOfRange));

    // A field beyond the configured range, flagged normally: refused too.
    knobs.overflow.set(false);
    let (mut compass, clock, knobs, _shared) = adapter(|mut device| {
        device.with_field([30_000, 3_000, 3_000]);
        device
    });
    bring_up(&mut compass, &clock);
    knobs.data_ready.set(true);
    assert_eq!(compass.read(), Err(SensorError::OutOfRange));
}

#[test]
fn a_read_that_overruns_its_budget_is_a_timeout_and_its_result_discarded() {
    let (mut compass, clock, knobs, _shared) = adapter(|mut device| {
        device.with_field([3_000, -6_000, 0]);
        device
    });
    bring_up(&mut compass, &clock);

    // The field is ready, but the transfers the read needs outlast its
    // budget: the sample it would return is discarded.
    knobs.data_ready.set(true);
    clock.set_per_transfer(3_000);
    assert_eq!(
        compass.read(),
        Err(SensorError::Timeout(TimeoutStage::Read))
    );

    // The next read asks the part again, so the discarded sample never
    // arrives as a later read's result.
    clock.set_per_transfer(0);
    knobs.data_ready.set(true);
    let sample = compass.read().unwrap();
    assert_eq!(sample.seq, 1);
}

#[test]
fn a_bus_error_at_any_step_is_a_bus_error_and_never_a_sample() {
    let (mut compass, clock, knobs, _shared) = adapter(|device| device);
    assert_eq!(
        compass.start(),
        Ok(Bringup::Pending {
            retry_at: TimestampMicros(START_US + u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US))
        })
    );
    // The bus dies while the part would be identifying itself.
    knobs.fail.set(true);
    clock.advance(u64::from(Qmc5883lTiming::DEFAULT_RESET_WAIT_US));
    assert_eq!(compass.start(), Err(SensorError::Bus));
    assert_eq!(compass.start(), Err(SensorError::Bus));

    // A running sensor reports a failed transfer the same way, and never
    // a sample from it: the call that failed took nothing, and the next
    // call reads the field the part still flags.
    let (mut compass, clock, knobs, _shared) = adapter(|mut device| {
        device.with_field([3_000, -6_000, 0]);
        device
    });
    bring_up(&mut compass, &clock);
    knobs.data_ready.set(true);
    knobs.fail.set(true);
    assert_eq!(compass.read(), Err(SensorError::Bus));
    knobs.fail.set(false);
    let sample = compass.read().unwrap();
    assert_eq!(sample.seq, 1);
}
