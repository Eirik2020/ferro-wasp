//! The adapter against a hand-written DPS310 register model, driven through
//! the real `uf-dps3xx` driver. Expected compensated values come from the
//! datasheet's compensation formula evaluated here in f64, an independent
//! oracle; no worked numeric example from the datasheet was at hand, so
//! they are inferred, not quoted.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::vec::Vec;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};
use ferrowasp_io_contracts::time::TimestampMicros;
use ferrowasp_sensor_traits::barometer::{BaroSample, Barometer};
use ferrowasp_sensor_traits::class::{Bringup, Clock, Sensor, SensorError, TimeoutStage};
use uf_dps3xx::{Config, I2cAddress, Oversampling, Rate};

use super::{Dps310, Dps310Timing, PRODUCT_ID};

const ADDRESS: u8 = 0x77;
const START_US: u64 = 1_000_000;
/// The scale factor for single oversampling, the default configuration.
const SCALE_X1: f64 = 524_288.0;

const MEAS_CFG: usize = 0x08;
const PROD_ID: usize = 0x0d;
const COEFFICIENTS: usize = 0x10;

#[derive(Clone, Copy)]
struct Coefficients {
    c0: i32,
    c1: i32,
    c00: i32,
    c10: i32,
    c01: i32,
    c11: i32,
    c20: i32,
    c21: i32,
    c30: i32,
}

/// Coefficients of the size a DPS310 carries, chosen so typical raw values
/// give about 101.3 kPa and 25 °C.
const TYPICAL: Coefficients = Coefficients {
    c0: 204,
    c1: -261,
    c00: 80_469,
    c10: -54_769,
    c01: -2_770,
    c11: 1_265,
    c20: -12_000,
    c21: 134,
    c30: -1_146,
};

/// Coefficients that make a raw zero read back as exactly `c00` pascals
/// and `c0 / 2` degrees.
fn offsets_only(c00: i32, c0: i32) -> Coefficients {
    Coefficients {
        c0,
        c1: 0,
        c00,
        c10: 0,
        c01: 0,
        c11: 0,
        c20: 0,
        c21: 0,
        c30: 0,
    }
}

/// The coefficient registers' layout: 12-bit c0 and c1, 20-bit c00 and
/// c10, then five 16-bit values, all big-endian two's complement.
fn pack(c: Coefficients) -> [u8; 18] {
    let bits = |value: i32, width: u32| (value as u32) & ((1u32 << width) - 1);
    let (c0, c1, c00, c10) = (
        bits(c.c0, 12),
        bits(c.c1, 12),
        bits(c.c00, 20),
        bits(c.c10, 20),
    );
    let mut out = [0u8; 18];
    out[0] = (c0 >> 4) as u8;
    out[1] = (((c0 & 0x0f) << 4) | (c1 >> 8)) as u8;
    out[2] = (c1 & 0xff) as u8;
    out[3] = (c00 >> 12) as u8;
    out[4] = ((c00 >> 4) & 0xff) as u8;
    out[5] = (((c00 & 0x0f) << 4) | (c10 >> 16)) as u8;
    out[6] = ((c10 >> 8) & 0xff) as u8;
    out[7] = (c10 & 0xff) as u8;
    for (index, value) in [c.c01, c.c11, c.c20, c.c21, c.c30].into_iter().enumerate() {
        let [high, low] = (bits(value, 16) as u16).to_be_bytes();
        out[8 + 2 * index] = high;
        out[9 + 2 * index] = low;
    }
    out
}

/// The datasheet's compensation, in f64: pascals and degrees Celsius.
fn compensate(c: Coefficients, raw_pressure: i32, raw_temperature: i32) -> (f64, f64) {
    let p = f64::from(raw_pressure) / SCALE_X1;
    let t = f64::from(raw_temperature) / SCALE_X1;
    let f = f64::from;
    let pressure = f(c.c00)
        + p * (f(c.c10) + p * (f(c.c20) + p * f(c.c30)))
        + t * f(c.c01)
        + t * p * (f(c.c11) + p * f(c.c21));
    (pressure, f(c.c0) * 0.5 + f(c.c1) * t)
}

/// The DPS310 as the driver sees it: a register file, with the status bits
/// of `MEAS_CFG` modelled.
struct Device {
    registers: [u8; 256],
    mode: u8,
    sensor_ready: bool,
    coefficients_ready: bool,
    /// Whether a single temperature measurement, asked for in bring-up,
    /// completes.
    temperature_completes: bool,
    pressure_ready: bool,
    temperature_ready: bool,
    transactions: usize,
    fail_at: Option<usize>,
    us_per_transaction: u64,
}

impl Device {
    fn read(&mut self, register: usize) -> u8 {
        match register {
            MEAS_CFG => {
                (u8::from(self.coefficients_ready) << 7)
                    | (u8::from(self.sensor_ready) << 6)
                    | (u8::from(self.temperature_ready) << 5)
                    | (u8::from(self.pressure_ready) << 4)
                    | self.mode
            }
            0x00..=0x02 => {
                self.pressure_ready = false;
                self.registers[register]
            }
            0x03..=0x05 => {
                self.temperature_ready = false;
                self.registers[register]
            }
            _ => self.registers[register],
        }
    }

    fn write(&mut self, register: usize, value: u8) {
        if register == MEAS_CFG {
            self.mode = value & 0x07;
            if self.mode == 0b010 && self.temperature_completes {
                self.temperature_ready = true;
            }
        } else {
            self.registers[register] = value;
        }
    }
}

struct Shared {
    device: RefCell<Device>,
    now_us: Cell<u64>,
}

#[derive(Clone)]
struct Rig(Rc<Shared>);

impl Rig {
    fn new(coefficients: Coefficients) -> Self {
        let mut registers = [0u8; 256];
        registers[PROD_ID] = PRODUCT_ID;
        registers[COEFFICIENTS..COEFFICIENTS + 18].copy_from_slice(&pack(coefficients));
        Self(Rc::new(Shared {
            device: RefCell::new(Device {
                registers,
                mode: 0,
                sensor_ready: true,
                coefficients_ready: true,
                temperature_completes: true,
                pressure_ready: false,
                temperature_ready: false,
                transactions: 0,
                fail_at: None,
                us_per_transaction: 100,
            }),
            now_us: Cell::new(START_US),
        }))
    }

    fn device(&self) -> std::cell::RefMut<'_, Device> {
        self.0.device.borrow_mut()
    }

    fn now(&self) -> u64 {
        self.0.now_us.get()
    }

    fn advance(&self, us: u64) {
        self.0.now_us.set(self.now().saturating_add(us));
    }

    fn set_now(&self, us: u64) {
        self.0.now_us.set(us);
    }

    /// The part completes a background measurement of both quantities.
    fn publish(&self, raw_pressure: i32, raw_temperature: i32) {
        let mut device = self.device();
        let p = (raw_pressure as u32 & 0x00ff_ffff).to_be_bytes();
        let t = (raw_temperature as u32 & 0x00ff_ffff).to_be_bytes();
        device.registers[0..3].copy_from_slice(&p[1..]);
        device.registers[3..6].copy_from_slice(&t[1..]);
        device.pressure_ready = true;
        device.temperature_ready = true;
    }

    fn adapter(&self) -> Dps310<FakeI2c, FakeClock> {
        self.adapter_with(
            Config::default(),
            Dps310Timing::for_config(&Config::default()),
        )
    }

    fn adapter_with(&self, config: Config, timing: Dps310Timing) -> Dps310<FakeI2c, FakeClock> {
        Dps310::new(
            FakeI2c(self.clone()),
            I2cAddress::Secondary,
            config,
            timing,
            FakeClock(self.clone()),
        )
        .unwrap()
    }
}

#[derive(Debug)]
struct FakeError;

impl embedded_hal::i2c::Error for FakeError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

struct FakeI2c(Rig);

impl ErrorType for FakeI2c {
    type Error = FakeError;
}

impl I2c for FakeI2c {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), FakeError> {
        let rig = &self.0;
        let mut device = rig.device();
        let index = device.transactions;
        device.transactions += 1;
        rig.advance(device.us_per_transaction);
        if device.fail_at == Some(index) || address != ADDRESS {
            return Err(FakeError);
        }
        let mut pointer = 0usize;
        for operation in operations {
            match operation {
                Operation::Write(bytes) => {
                    if let Some((register, data)) = bytes.split_first() {
                        pointer = usize::from(*register);
                        for (offset, value) in data.iter().enumerate() {
                            device.write(pointer + offset, *value);
                        }
                    }
                }
                Operation::Read(buffer) => {
                    for (offset, slot) in buffer.iter_mut().enumerate() {
                        *slot = device.read(pointer + offset);
                    }
                }
            }
        }
        Ok(())
    }
}

struct FakeClock(Rig);

impl Clock for FakeClock {
    fn now(&mut self) -> TimestampMicros {
        TimestampMicros(self.0.now())
    }
}

/// Bring the adapter up as its task would: call `start`, wait until each
/// `retry_at`, and stop at the first error. Bounded, so a bring-up that
/// never ends fails the test instead of hanging it.
fn bring_up<B: Barometer>(rig: &Rig, adapter: &mut B) -> Result<(), SensorError> {
    for _ in 0..1_000 {
        match adapter.start()? {
            Bringup::Ready => return Ok(()),
            Bringup::Pending { retry_at } => {
                if rig.now() < retry_at.0 {
                    rig.set_now(retry_at.0);
                }
            }
        }
    }
    panic!("bring-up neither finished nor failed in 1000 steps");
}

fn within(actual: i64, expected: f64, tolerance: f64) -> bool {
    (actual as f64 - expected).abs() <= tolerance
}

#[test]
fn a_good_sample_is_compensated_stamped_and_numbered() {
    let rig = Rig::new(TYPICAL);
    let mut adapter = rig.adapter();
    bring_up(&rig, &mut adapter).unwrap();
    assert_eq!(
        rig.device().mode,
        0b111,
        "running means continuous background measurement"
    );

    for (seq, (raw_pressure, raw_temperature)) in [(-230_000, 154_700), (-240_000, 150_000)]
        .into_iter()
        .enumerate()
    {
        rig.publish(raw_pressure, raw_temperature);
        let sample = adapter.read().unwrap();
        let (pascals, celsius) = compensate(TYPICAL, raw_pressure, raw_temperature);
        assert_eq!(sample.seq, seq as u32 + 1);
        assert_eq!(
            sample.at,
            TimestampMicros(rig.now()),
            "stamped after its read"
        );
        // The driver computes in f32; 0.1 Pa and 0.01 °C cover its rounding.
        assert!(
            within(
                i64::from(sample.value.pressure.centipascals()),
                pascals * 100.0,
                10.0
            ),
            "{} cPa against {pascals} Pa",
            sample.value.pressure.centipascals()
        );
        assert!(within(
            i64::from(sample.value.temperature.centidegrees_celsius()),
            celsius * 100.0,
            1.0
        ));
    }
}

#[test]
fn a_raw_zero_reads_back_the_offset_coefficients_exactly() {
    let rig = Rig::new(offsets_only(101_325, 50));
    let mut adapter = rig.adapter();
    bring_up(&rig, &mut adapter).unwrap();
    rig.publish(0, 0);
    let sample = adapter.read().unwrap();
    assert_eq!(sample.value.pressure.centipascals(), 10_132_500);
    assert_eq!(sample.value.temperature.centidegrees_celsius(), 2_500);
}

#[test]
fn a_product_id_other_than_the_dps310s_is_refused() {
    // 0x11 passes the driver's own check, which compares the upper nibble
    // only; 0x20 and 0x00 fail both.
    for found in [0x11, 0x20, 0x00, 0xff] {
        let rig = Rig::new(TYPICAL);
        rig.device().registers[PROD_ID] = found;
        let mut adapter = rig.adapter();
        let mismatch = Err(SensorError::IdentityMismatch { found });
        assert_eq!(adapter.start(), mismatch);
        assert_eq!(adapter.start(), mismatch, "a failed bring-up stays failed");
        assert_eq!(adapter.read(), Err(SensorError::NotStarted));
        assert_eq!(rig.device().mode, 0, "nothing was configured or started");
        assert!(adapter.release().is_some());
    }
}

#[test]
fn a_bus_error_at_any_step_is_a_bus_error_and_never_a_sample() {
    let transactions = {
        let rig = Rig::new(TYPICAL);
        let mut adapter = rig.adapter();
        bring_up(&rig, &mut adapter).unwrap();
        rig.publish(-230_000, 154_700);
        adapter.read().unwrap();
        rig.device().transactions
    };
    assert!(
        transactions > 20,
        "bring-up and one read take {transactions} transfers"
    );

    let mut in_bringup = 0;
    for fail_at in 0..transactions {
        let rig = Rig::new(TYPICAL);
        rig.device().fail_at = Some(fail_at);
        let mut adapter = rig.adapter();
        match bring_up(&rig, &mut adapter) {
            Err(error) => {
                in_bringup += 1;
                assert_eq!(error, SensorError::Bus, "transfer {fail_at}");
                assert_eq!(
                    rig.device().transactions,
                    fail_at + 1,
                    "it stopped at the failure"
                );
                assert_eq!(adapter.start(), Err(SensorError::Bus));
                assert_eq!(adapter.read(), Err(SensorError::NotStarted));
            }
            Ok(()) => {
                rig.publish(-230_000, 154_700);
                assert_eq!(adapter.read(), Err(SensorError::Bus), "transfer {fail_at}");
                // A failed read leaves the adapter running for the next one.
                rig.publish(-230_000, 154_700);
                assert_eq!(adapter.read().map(|sample| sample.seq), Ok(1));
            }
        }
    }
    assert_eq!(
        in_bringup,
        transactions - 2,
        "all but the read's two transfers are bring-up"
    );
}

#[test]
fn bring_up_that_never_completes_times_out() {
    for stage in ["sensor", "temperature", "coefficients"] {
        let rig = Rig::new(TYPICAL);
        {
            let mut device = rig.device();
            match stage {
                "sensor" => device.sensor_ready = false,
                "temperature" => device.temperature_completes = false,
                _ => device.coefficients_ready = false,
            }
        }
        let mut adapter = rig.adapter();
        assert_eq!(
            bring_up(&rig, &mut adapter),
            Err(SensorError::Timeout(TimeoutStage::Bringup)),
            "{stage}"
        );
        let elapsed = rig.now() - START_US;
        assert!(
            elapsed <= u64::from(Dps310Timing::DEFAULT_BRINGUP_BUDGET_US) + 1_000,
            "{stage}: gave up after {elapsed} us"
        );
        assert_eq!(adapter.read(), Err(SensorError::NotStarted));
        assert!(adapter.release().is_some());
    }
}

#[test]
fn the_drivers_wait_budget_ends_bring_up_before_the_clock_does() {
    // 25 ms of the driver's waits, 26 ms on the clock: a sensor never ready
    // asks for 13 ms waits, so the second wait exceeds the driver's budget
    // at about 13 ms, long before the clock's deadline.
    let rig = Rig::new(TYPICAL);
    rig.device().sensor_ready = false;
    let timing = Dps310Timing {
        bringup_budget_us: 25_999,
        ..Dps310Timing::for_config(&Config::default())
    };
    let mut adapter = rig.adapter_with(Config::default(), timing);
    assert_eq!(
        bring_up(&rig, &mut adapter),
        Err(SensorError::Timeout(TimeoutStage::Bringup))
    );
    assert!(
        rig.now() - START_US < 20_000,
        "ended at {} us",
        rig.now() - START_US
    );
}

#[test]
fn without_a_new_result_a_read_is_never_the_previous_sample() {
    let rig = Rig::new(TYPICAL);
    let mut adapter = rig.adapter();
    bring_up(&rig, &mut adapter).unwrap();
    let stall = u64::from(Dps310Timing::for_config(&Config::default()).stall_limit_us);
    assert_eq!(stall, 3_000_000, "three periods at the default 1 Hz");

    // No result yet after bring-up.
    assert_eq!(adapter.read(), Err(SensorError::NoNewSample));

    rig.publish(-230_000, 154_700);
    let first = adapter.read().unwrap();
    let read_at = rig.now();
    assert_eq!(adapter.read(), Err(SensorError::NoNewSample));
    assert_eq!(adapter.read(), Err(SensorError::NoNewSample));

    // A pressure result alone is not a sample.
    rig.device().pressure_ready = true;
    assert_eq!(adapter.read(), Err(SensorError::NoNewSample));
    rig.device().pressure_ready = false;

    rig.set_now(read_at + stall - 1_000);
    assert_eq!(adapter.read(), Err(SensorError::NoNewSample));
    rig.set_now(read_at + stall);
    assert_eq!(
        adapter.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
    assert_eq!(
        adapter.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );

    rig.publish(-240_000, 150_000);
    let second = adapter.read().unwrap();
    assert_eq!(second.seq, first.seq + 1);
    assert!(second.at > first.at);
    assert_ne!(second.value, first.value);
    assert_eq!(adapter.read(), Err(SensorError::NoNewSample));
}

#[test]
fn a_sensor_that_never_measures_after_bring_up_times_out() {
    let rig = Rig::new(TYPICAL);
    let mut adapter = rig.adapter();
    bring_up(&rig, &mut adapter).unwrap();
    rig.advance(3_000_000);
    assert_eq!(
        adapter.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
}

#[test]
fn a_value_outside_the_operating_range_is_refused() {
    let cases: [(Coefficients, &str); 4] = [
        (offsets_only(29_999, 50), "pressure below 300 hPa"),
        (offsets_only(120_001, 50), "pressure above 1200 hPa"),
        (offsets_only(101_325, -81), "temperature below -40 °C"),
        (offsets_only(101_325, 171), "temperature above 85 °C"),
    ];
    for (coefficients, case) in cases {
        let rig = Rig::new(coefficients);
        let mut adapter = rig.adapter();
        bring_up(&rig, &mut adapter).unwrap();
        rig.publish(0, 0);
        assert_eq!(adapter.read(), Err(SensorError::OutOfRange), "{case}");
        // The refused result is consumed, not offered again.
        assert_eq!(adapter.read(), Err(SensorError::NoNewSample), "{case}");
    }

    // Exactly at the limits is in range.
    for (c00, c0) in [(30_000, -80), (120_000, 170)] {
        let rig = Rig::new(offsets_only(c00, c0));
        let mut adapter = rig.adapter();
        bring_up(&rig, &mut adapter).unwrap();
        rig.publish(0, 0);
        let sample: BaroSample = adapter.read().unwrap().value;
        assert_eq!(sample.pressure.centipascals(), c00 as u32 * 100);
        assert_eq!(sample.temperature.centidegrees_celsius(), c0 * 50);
    }

    // The extreme raw values and coefficients stay finite in the driver's
    // f32 maths, and are refused rather than wrapped.
    let extreme = Coefficients {
        c0: 2_047,
        c1: 2_047,
        c00: 524_287,
        c10: 524_287,
        c01: 32_767,
        c11: 32_767,
        c20: 32_767,
        c21: 32_767,
        c30: 32_767,
    };
    for raw in [0x7f_ffff, -0x80_0000] {
        let rig = Rig::new(extreme);
        let mut adapter = rig.adapter();
        bring_up(&rig, &mut adapter).unwrap();
        rig.publish(raw, raw);
        assert_eq!(adapter.read(), Err(SensorError::OutOfRange), "raw {raw}");
    }
}

#[test]
fn a_read_that_overruns_its_budget_is_a_timeout_and_its_result_discarded() {
    let rig = Rig::new(TYPICAL);
    let mut adapter = rig.adapter();
    bring_up(&rig, &mut adapter).unwrap();
    rig.device().us_per_transaction = 2_500;
    rig.publish(-230_000, 154_700);
    assert_eq!(
        adapter.read(),
        Err(SensorError::Timeout(TimeoutStage::Read))
    );
    rig.device().us_per_transaction = 100;
    assert_eq!(adapter.read(), Err(SensorError::NoNewSample));
    rig.publish(-230_000, 154_700);
    assert_eq!(
        adapter.read().map(|sample| sample.seq),
        Ok(1),
        "a discarded result took no number"
    );
}

#[test]
fn time_near_the_end_of_the_clock_fails_closed() {
    let rig = Rig::new(TYPICAL);
    rig.set_now(u64::MAX - 20_000);
    let mut adapter = rig.adapter();
    // The deadline and every retry saturate at the end of time, where the
    // deadline counts as expired: bring-up ends, it never wraps to the past.
    assert_eq!(
        bring_up(&rig, &mut adapter),
        Err(SensorError::Timeout(TimeoutStage::Bringup))
    );
    assert_eq!(rig.now(), u64::MAX);
}

#[test]
fn a_read_before_bring_up_is_refused_and_the_bus_is_given_back() {
    let rig = Rig::new(TYPICAL);
    let mut adapter = rig.adapter();
    assert_eq!(adapter.read(), Err(SensorError::NotStarted));
    assert_eq!(rig.device().transactions, 0);
    assert!(adapter.release().is_some());

    let mut starting = rig.adapter();
    assert!(matches!(starting.start(), Ok(Bringup::Pending { .. })));
    assert_eq!(starting.read(), Err(SensorError::NotStarted));
    assert!(starting.release().is_some());

    let mut running = rig.adapter();
    bring_up(&rig, &mut running).unwrap();
    assert_eq!(running.start(), Ok(Bringup::Ready));
    assert!(running.release().is_some());
}

#[test]
fn a_configuration_the_part_cannot_run_is_refused() {
    let rig = Rig::new(TYPICAL);
    let config = Config::default()
        .pressure(Rate::Hz128, Oversampling::X128)
        .temperature(Rate::Hz128, Oversampling::X128);
    let refused = Dps310::new(
        FakeI2c(rig.clone()),
        I2cAddress::Secondary,
        config,
        Dps310Timing::for_config(&config),
        FakeClock(rig.clone()),
    );
    assert!(matches!(refused, Err(SensorError::Configuration)));
    assert_eq!(rig.device().transactions, 0);
}

#[test]
fn the_stall_limit_follows_the_slower_rate_and_saturates() {
    let fast = Config::default()
        .pressure(Rate::Hz32, Oversampling::X1)
        .temperature(Rate::Hz8, Oversampling::X1);
    assert_eq!(Dps310Timing::for_config(&fast).stall_limit_us, 375_000);
    let all: Vec<u32> = [
        Rate::Hz1,
        Rate::Hz2,
        Rate::Hz4,
        Rate::Hz8,
        Rate::Hz16,
        Rate::Hz32,
        Rate::Hz64,
        Rate::Hz128,
    ]
    .into_iter()
    .map(|rate| {
        Dps310Timing::for_config(
            &Config::default()
                .pressure(rate, Oversampling::X1)
                .temperature(rate, Oversampling::X1),
        )
        .stall_limit_us
    })
    .collect();
    assert_eq!(
        all,
        [
            3_000_000, 1_500_000, 750_000, 375_000, 189_000, 96_000, 48_000, 24_000
        ]
    );
}
