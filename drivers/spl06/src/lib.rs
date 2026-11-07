//! The Goertek SPL06-007 barometer over I2C: registers, the calibration
//! coefficient page, and the part's compensation formulas, for `no_std`
//! firmware through `embedded-hal`.
//!
//! Sources: every register address, bit and constant below cites both
//! ArduPilot's `AP_Baro_SPL06.cpp` and iNav's `barometer_spl06.c`, which
//! agree on everything this crate carries. Where only one of them holds a
//! fact the documentation says *single-source*; where neither does,
//! *inferred*, and such a fact needs checking against the part's datasheet
//! and a board.
//!
//! The part measures on command: a write to `MEAS_CFG` starts one pressure
//! or one temperature measurement, and the matching ready bit appears in
//! `MEAS_CFG` when it completes (both sources drive it this way). The driver
//! starts and polls; it never waits, so pacing a measurement is the
//! caller's. Every call makes bounded bus work: one write, or one
//! write-read of at most 18 bytes.
//!
//! The crate never panics on any input, never allocates, and has no unsafe
//! code.
//!
//! Requirements: SPL06-001, SPL06-002, SPL06-003, SPL06-004, SPL06-005

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's seven-bit address: iNav `barometer_spl06.c` ("SPL06, address
/// 0x76"); ArduPilot's `speedybeef4v5` and `TBS_LUCID_H7` hwdefs probe
/// `BARO SPL06 I2C:0:0x76`.
pub const ADDRESS: u8 = 0x76;

/// The chip ID register reads 0x10: ArduPilot `SPL06_CHIP_ID 0x10` against
/// `SPL06_REG_CHIP_ID 0x0D`; iNav `SPL06_DEFAULT_CHIP_ID` against
/// `SPL06_CHIP_ID_REG`.
pub const CHIP_ID: u8 = 0x10;

/// `PRS_B2` to `PRS_B0`, MSB first: ArduPilot `SPL06_REG_PRESSURE_B2 0x00`
/// through `SPL06_REG_PRESSURE_B0 0x02`; iNav `SPL06_PRESSURE_START_REG`.
pub const REG_PRESSURE: u8 = 0x00;
/// `TMP_B2` to `TMP_B0`, MSB first: ArduPilot
/// `SPL06_REG_TEMPERATURE_B2 0x03` through `SPL06_REG_TEMPERATURE_B0 0x05`;
/// iNav `SPL06_TEMPERATURE_START_REG`.
pub const REG_TEMPERATURE: u8 = 0x03;
/// `PRS_CFG`: ArduPilot `SPL06_REG_PRESSURE_CFG 0x06`; iNav
/// `SPL06_PRESSURE_CFG_REG`.
pub const REG_PRESSURE_CFG: u8 = 0x06;
/// `TMP_CFG`: ArduPilot `SPL06_REG_TEMPERATURE_CFG 0x07`; iNav
/// `SPL06_TEMPERATURE_CFG_REG`.
pub const REG_TEMPERATURE_CFG: u8 = 0x07;
/// `MEAS_CFG`, the mode and status register: ArduPilot
/// `SPL06_REG_MODE_AND_STATUS 0x08`; iNav `SPL06_MODE_AND_STATUS_REG`.
pub const REG_MEAS_CFG: u8 = 0x08;
/// The chip ID register: ArduPilot `SPL06_REG_CHIP_ID 0x0D`; iNav
/// `SPL06_CHIP_ID_REG`.
pub const REG_CHIP_ID: u8 = 0x0d;
/// `COEFFRDY` through `C21`: ArduPilot `SPL06_REG_CALIB_COEFFS_START 0x10`
/// to `SPL06_REG_CALIB_COEFFS_END 0x21` (`SPL06_NUM_CALIB_COEFFS 18`); iNav
/// `SPL06_CALIB_COEFFS_START`, `SPL06_CALIB_COEFFS_LEN`.
pub const REG_COEFFICIENTS: u8 = 0x10;
/// The calibration page's length in bytes, 18: both sources above.
pub const COEFFICIENTS_LEN: usize = 18;

/// `MEAS_CFG` bit 0, the command for one pressure measurement: ArduPilot
/// `SPL06_MEAS_PRESSURE (1<<0)`; iNav `SPL06_MEAS_PRESSURE`.
pub const MEAS_PRESSURE: u8 = 1 << 0;
/// `MEAS_CFG` bit 1, the command for one temperature measurement: ArduPilot
/// `SPL06_MEAS_TEMPERATURE (1<<1)`; iNav `SPL06_MEAS_TEMPERATURE`.
pub const MEAS_TEMPERATURE: u8 = 1 << 1;
/// `MEAS_CFG` bit 4, `PRS_RDY`: ArduPilot `SPL06_MEAS_CFG_PRESSURE_RDY
/// (1<<4)`; iNav `SPL06_MEAS_CFG_PRESSURE_RDY`.
pub const PRESSURE_READY: u8 = 1 << 4;
/// `MEAS_CFG` bit 5, `TMP_RDY`: ArduPilot `SPL06_MEAS_CFG_TEMPERATURE_RDY
/// (1<<5)`; iNav `SPL06_MEAS_CFG_TEMPERATURE_RDY`.
pub const TEMPERATURE_READY: u8 = 1 << 5;
/// `MEAS_CFG` bit 6, `SENSOR_RDY`: ArduPilot `SPL06_MEAS_CFG_SENSOR_RDY
/// (1<<6)`; iNav `SPL06_MEAS_CFG_SENSOR_RDY`.
pub const SENSOR_READY: u8 = 1 << 6;
/// `MEAS_CFG` bit 7, `COEFFRDY`: ArduPilot `SPL06_MEAS_CFG_COEFFS_RDY
/// (1<<7)`; iNav `SPL06_MEAS_CFG_COEFFS_RDY`.
pub const COEFFICIENTS_READY: u8 = 1 << 7;
/// `TMP_CFG` bit 7, use the part's external temperature sensor: ArduPilot
/// `SPL06_TEMP_USE_EXT_SENSOR (1<<7)`; iNav `SPL06_TEMP_USE_EXT_SENSOR`.
/// Both set it, so the internal sensor's reading is never used here.
pub const TEMPERATURE_EXTERNAL_SENSOR: u8 = 1 << 7;

/// How many times a measurement oversamples: the `PRS_CFG`/`TMP_CFG`
/// oversampling field, the same value in both sources
/// (`spl06_samples_to_cfg_reg_value` in ArduPilot and iNav, 1 to 128 as
/// powers of two).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum Oversampling {
    #[default]
    Times1,
    Times2,
    Times4,
    Times8,
    Times16,
    Times32,
    Times64,
    Times128,
}

impl Oversampling {
    /// The field's value in `PRS_CFG` and `TMP_CFG`, bits 2:0: both sources.
    pub const fn register_bits(self) -> u8 {
        match self {
            Self::Times1 => 0,
            Self::Times2 => 1,
            Self::Times4 => 2,
            Self::Times8 => 3,
            Self::Times16 => 4,
            Self::Times32 => 5,
            Self::Times64 => 6,
            Self::Times128 => 7,
        }
    }

    /// The raw-value scale factor the part's compensation divides by: both
    /// sources' table, identical to the digit
    /// (`raw_value_scale_factor` / `spl06_raw_value_scale_factor`).
    pub const fn scale_factor(self) -> i32 {
        match self {
            Self::Times1 => 524_288,
            Self::Times2 => 1_572_864,
            Self::Times4 => 3_670_016,
            Self::Times8 => 7_864_320,
            Self::Times16 => 253_952,
            Self::Times32 => 516_096,
            Self::Times64 => 1_040_384,
            Self::Times128 => 2_088_960,
        }
    }
}

/// A two's-complement `width`-bit field widened to `i32`.
const fn sign_extend(bits: u32, width: u32) -> i32 {
    if bits & (1 << (width - 1)) != 0 {
        (bits as i32) - (1 << width)
    } else {
        bits as i32
    }
}

/// The part's calibration coefficients, one page of 18 bytes at
/// `COEFFICIENTS` (both sources' `read_calibration_coefficients` /
/// `read_calibration_coefficients`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Coefficients {
    /// 12-bit, bits 11:0 of the first byte and a half.
    c0: i16,
    /// 12-bit.
    c1: i16,
    /// 20-bit.
    c00: i32,
    /// 20-bit.
    c10: i32,
    /// 16-bit, big-endian pairs from byte 8 on.
    c01: i16,
    c11: i16,
    c20: i16,
    c21: i16,
    c30: i16,
}

impl Coefficients {
    /// Decode the calibration page as the part packs it: `c0` and `c1`
    /// 12-bit, `c00` and `c10` 20-bit, the rest 16-bit, all two's
    /// complement, most significant byte first. The layout is iNav's
    /// `read_calibration_coefficients`, quoted register by register;
    /// ArduPilot's byte reader agrees (single-source, its union is not
    /// quoted here).
    pub fn decode(bytes: &[u8; COEFFICIENTS_LEN]) -> Self {
        let word = |at: usize| u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        Self {
            c0: sign_extend(((bytes[0] as u32) << 4) | ((bytes[1] as u32) >> 4), 12) as i16,
            c1: sign_extend((((bytes[1] as u32) & 0x0f) << 8) | bytes[2] as u32, 12) as i16,
            c00: sign_extend(
                ((bytes[3] as u32) << 12) | ((bytes[4] as u32) << 4) | ((bytes[5] as u32) >> 4),
                20,
            ),
            c10: sign_extend(
                (((bytes[5] as u32) & 0x0f) << 16) | ((bytes[6] as u32) << 8) | bytes[7] as u32,
                20,
            ),
            c01: sign_extend(word(8) as u32, 16) as i16,
            c11: sign_extend(word(10) as u32, 16) as i16,
            c20: sign_extend(word(12) as u32, 16) as i16,
            c21: sign_extend(word(14) as u32, 16) as i16,
            c30: sign_extend(word(16) as u32, 16) as i16,
        }
    }

    /// Compensated temperature in degrees Celsius, `c0/2 + raw/scale * c1`
    /// (ArduPilot `_update_temperature`; iNav `spl06_compensate_temperature`),
    /// or `None` where the value is not finite.
    pub fn temperature_celsius(&self, raw: i32, oversampling: Oversampling) -> Option<f32> {
        let scaled = raw as f32 / oversampling.scale_factor() as f32;
        let value = self.c0 as f32 / 2.0 + scaled * self.c1 as f32;
        finite(value)
    }

    /// Compensated pressure in pascals, the part's formula (ArduPilot
    /// `_update_pressure`; iNav `spl06_compensate_pressure`):
    /// `c00 + pr*(c10 + pr*(c20 + pr*c30)) + tr*(c01 + pr*(c11 + pr*c21))`
    /// with `pr` the pressure raw divided by its scale factor and `tr` the
    /// temperature raw divided by its own. `None` where not finite.
    pub fn pressure_pascals(
        &self,
        pressure_raw: i32,
        pressure_oversampling: Oversampling,
        temperature_raw: i32,
        temperature_oversampling: Oversampling,
    ) -> Option<f32> {
        let pr = pressure_raw as f32 / pressure_oversampling.scale_factor() as f32;
        let tr = temperature_raw as f32 / temperature_oversampling.scale_factor() as f32;
        let pressure_cal = self.c00 as f32
            + pr * (self.c10 as f32 + pr * (self.c20 as f32 + pr * self.c30 as f32));
        let temperature_term =
            tr * (self.c01 as f32 + pr * (self.c11 as f32 + pr * self.c21 as f32));
        finite(pressure_cal + temperature_term)
    }

    pub const fn c0(&self) -> i16 {
        self.c0
    }
    pub const fn c1(&self) -> i16 {
        self.c1
    }
    pub const fn c00(&self) -> i32 {
        self.c00
    }
    pub const fn c10(&self) -> i32 {
        self.c10
    }
    pub const fn c01(&self) -> i16 {
        self.c01
    }
    pub const fn c11(&self) -> i16 {
        self.c11
    }
    pub const fn c20(&self) -> i16 {
        self.c20
    }
    pub const fn c21(&self) -> i16 {
        self.c21
    }
    pub const fn c30(&self) -> i16 {
        self.c30
    }
}

/// `None` for a NaN or infinity, the value otherwise.
fn finite(value: f32) -> Option<f32> {
    if value.is_finite() { Some(value) } else { None }
}

/// The state of `MEAS_CFG`'s status bits: what is ready, and what the part
/// is doing (both sources' bit definitions).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Status {
    pub pressure_ready: bool,
    pub temperature_ready: bool,
    pub sensor_ready: bool,
    pub coefficients_ready: bool,
}

impl Status {
    /// The `MEAS_CFG` byte, bit for bit.
    pub const fn from_register(meas_cfg: u8) -> Self {
        Self {
            pressure_ready: meas_cfg & PRESSURE_READY != 0,
            temperature_ready: meas_cfg & TEMPERATURE_READY != 0,
            sensor_ready: meas_cfg & SENSOR_READY != 0,
            coefficients_ready: meas_cfg & COEFFICIENTS_READY != 0,
        }
    }
}

/// An SPL06-007 on an I2C bus, at one address.
#[derive(Debug)]
pub struct Spl06<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Spl06<I2C> {
    pub const fn new(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    pub fn into_inner(self) -> I2C {
        self.i2c
    }

    /// The part's seven-bit address this driver talks to.
    pub const fn address(&self) -> u8 {
        self.address
    }
}

impl<I2C: I2c> Spl06<I2C> {
    /// The chip ID register, to compare against [`CHIP_ID`]: one write-read
    /// of one byte.
    pub fn read_chip_id(&mut self) -> Result<u8, I2C::Error> {
        let mut id = [0_u8; 1];
        self.i2c.write_read(self.address, &[REG_CHIP_ID], &mut id)?;
        Ok(id[0])
    }

    /// The `MEAS_CFG` status bits: one write-read of one byte.
    pub fn read_status(&mut self) -> Result<Status, I2C::Error> {
        let mut meas_cfg = [0_u8; 1];
        self.i2c
            .write_read(self.address, &[REG_MEAS_CFG], &mut meas_cfg)?;
        Ok(Status::from_register(meas_cfg[0]))
    }

    /// The calibration page, all 18 bytes at once: one write-read (both
    /// sources read the page in one transfer).
    pub fn read_coefficients(&mut self) -> Result<Coefficients, I2C::Error> {
        let mut page = [0_u8; COEFFICIENTS_LEN];
        self.i2c
            .write_read(self.address, &[REG_COEFFICIENTS], &mut page)?;
        Ok(Coefficients::decode(&page))
    }

    /// Set both oversampling fields, and the external temperature sensor
    /// bit: two single-byte writes (iNav `spl06_configure_measurements`'s
    /// two configuration writes).
    pub fn configure(
        &mut self,
        pressure: Oversampling,
        temperature: Oversampling,
    ) -> Result<(), I2C::Error> {
        self.i2c
            .write(self.address, &[REG_PRESSURE_CFG, pressure.register_bits()])?;
        self.i2c.write(
            self.address,
            &[
                REG_TEMPERATURE_CFG,
                temperature.register_bits() | TEMPERATURE_EXTERNAL_SENSOR,
            ],
        )?;
        Ok(())
    }

    /// Start one temperature measurement: one single-byte write (iNav
    /// `spl06_start_temperature_measurement`; ArduPilot's command-mode
    /// `_timer` writes the same).
    pub fn start_temperature_measurement(&mut self) -> Result<(), I2C::Error> {
        self.i2c
            .write(self.address, &[REG_MEAS_CFG, MEAS_TEMPERATURE])
    }

    /// Start one pressure measurement: one single-byte write (iNav
    /// `spl06_start_pressure_measurement`).
    pub fn start_pressure_measurement(&mut self) -> Result<(), I2C::Error> {
        self.i2c.write(self.address, &[REG_MEAS_CFG, MEAS_PRESSURE])
    }

    /// The last temperature measurement's raw value, the part's 24-bit
    /// two's complement, MSB first, sign extended: one write-read of three
    /// bytes. Both sources assemble it exactly this way.
    pub fn read_temperature_raw(&mut self) -> Result<i32, I2C::Error> {
        let mut data = [0_u8; 3];
        self.i2c
            .write_read(self.address, &[REG_TEMPERATURE], &mut data)?;
        Ok(raw24(data))
    }

    /// The last pressure measurement's raw value, assembled as the
    /// temperature's: one write-read of three bytes.
    pub fn read_pressure_raw(&mut self) -> Result<i32, I2C::Error> {
        let mut data = [0_u8; 3];
        self.i2c
            .write_read(self.address, &[REG_PRESSURE], &mut data)?;
        Ok(raw24(data))
    }
}

/// A 24-bit two's-complement triple, MSB first, widened to `i32`: the
/// sign-extension both sources apply to a raw value.
fn raw24(data: [u8; 3]) -> i32 {
    let extended = ((data[0] as i32) << 16) | ((data[1] as i32) << 8) | data[2] as i32;
    if data[0] & 0x80 != 0 {
        extended | 0xff00_0000u32 as i32
    } else {
        extended
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};

    /// One transaction the mock saw: a write of `bytes` to `register`, or a
    /// read of `len` bytes from it.
    #[derive(Clone, Debug, PartialEq)]
    enum Transaction {
        Write(u8, std::vec::Vec<u8>),
        Read(u8, usize),
    }

    /// An SPL06 as a plain register file, recording every transaction.
    struct Device {
        registers: [u8; 64],
        fail: bool,
        transactions: std::vec::Vec<Transaction>,
    }

    impl Default for Device {
        fn default() -> Self {
            Self {
                registers: [0; 64],
                fail: false,
                transactions: std::vec::Vec::new(),
            }
        }
    }

    impl Device {
        fn with_page(page: &[u8; COEFFICIENTS_LEN]) -> Self {
            let mut device = Self::default();
            device.registers
                [REG_COEFFICIENTS as usize..REG_COEFFICIENTS as usize + COEFFICIENTS_LEN]
                .copy_from_slice(page);
            device.registers[REG_CHIP_ID as usize] = CHIP_ID;
            device
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
            if self.fail {
                return Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address));
            }
            assert_eq!(address, ADDRESS, "the driver talks to the part's address");
            let (first, rest) = operations.split_at_mut(1);
            let Operation::Write(bytes) = &first[0] else {
                return Err(ErrorKind::Other);
            };
            let register = bytes[0];
            match rest {
                [] => {
                    for (at, byte) in bytes.iter().skip(1).enumerate() {
                        self.registers[register as usize + at] = *byte;
                    }
                    self.transactions
                        .push(Transaction::Write(register, bytes[1..].to_vec()));
                    Ok(())
                }
                [Operation::Read(read)] => {
                    let len = read.len();
                    for (at, slot) in read.iter_mut().enumerate() {
                        *slot = self.registers[register as usize + at];
                    }
                    self.transactions.push(Transaction::Read(register, len));
                    Ok(())
                }
                _ => Err(ErrorKind::Other),
            }
        }
    }

    fn part(page: &[u8; COEFFICIENTS_LEN]) -> Spl06<Device> {
        Spl06::new(Device::with_page(page), ADDRESS)
    }

    /// A page of every width the part packs, with awkward negative values:
    /// c0 = -100, c1 = 300, c00 = 80469, c10 = -54769, c01 = -2770,
    /// c11 = 1265, c20 = -12000, c21 = 134, c30 = -1146.
    const PAGE: [u8; COEFFICIENTS_LEN] = [
        0xF9, 0xC1, 0x2C, 0x13, 0xA5, 0x5F, 0x2A, 0x0F, 0xF5, 0x2E, 0x04, 0xF1, 0xD1, 0x20, 0x00,
        0x86, 0xFB, 0x86,
    ];

    /// A page whose compensation gives exactly 101 325 Pa and 25.0 °C for
    /// raw pressure 262 144 and raw temperature 0, both at 1x: c0 = 50,
    /// c1 = 200, c00 = 100 325, c10 = 2000, the rest zero.
    const PLAUSIBLE: [u8; COEFFICIENTS_LEN] = [
        0x03, 0x20, 0xC8, 0x18, 0x7E, 0x50, 0x07, 0xD0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    #[test]
    fn coefficient_page_decodes_as_the_part_packs_it() {
        let coefficients = Coefficients::decode(&PAGE);
        assert_eq!(coefficients.c0(), -100);
        assert_eq!(coefficients.c1(), 300);
        assert_eq!(coefficients.c00(), 80_469);
        assert_eq!(coefficients.c10(), -54_769);
        assert_eq!(coefficients.c01(), -2_770);
        assert_eq!(coefficients.c11(), 1_265);
        assert_eq!(coefficients.c20(), -12_000);
        assert_eq!(coefficients.c21(), 134);
        assert_eq!(coefficients.c30(), -1_146);

        let plausible = Coefficients::decode(&PLAUSIBLE);
        assert_eq!(plausible.c0(), 50);
        assert_eq!(plausible.c1(), 200);
        assert_eq!(plausible.c00(), 100_325);
        assert_eq!(plausible.c10(), 2_000);
        assert_eq!(plausible.c01(), 0);
        assert_eq!(plausible.c30(), 0);
    }

    #[test]
    fn compensation_matches_both_reference_implementations() {
        let plausible = Coefficients::decode(&PLAUSIBLE);
        assert_eq!(
            plausible.temperature_celsius(0, Oversampling::Times1),
            Some(25.0)
        );
        let pressure =
            plausible.pressure_pascals(262_144, Oversampling::Times1, 0, Oversampling::Times1);
        assert!(pressure.is_some_and(|pascals| (pascals - 101_325.0).abs() < 0.01));

        // The same formulas in f64, an independent oracle, over the awkward
        // page at a heavier oversampling.
        let coefficients = Coefficients::decode(&PAGE);
        let (pressure_raw, pressure_oversampling) = (2_000_000_i32, Oversampling::Times8);
        let (temperature_raw, temperature_oversampling) = (500_000_i32, Oversampling::Times8);
        let pr = f64::from(pressure_raw) / f64::from(pressure_oversampling.scale_factor());
        let tr = f64::from(temperature_raw) / f64::from(temperature_oversampling.scale_factor());
        let expected_temperature =
            f64::from(coefficients.c0()) / 2.0 + tr * f64::from(coefficients.c1());
        let expected_pressure = f64::from(coefficients.c00())
            + pr * (f64::from(coefficients.c10())
                + pr * (f64::from(coefficients.c20()) + pr * f64::from(coefficients.c30())))
            + tr * (f64::from(coefficients.c01())
                + pr * (f64::from(coefficients.c11()) + pr * f64::from(coefficients.c21())));
        let temperature = coefficients
            .temperature_celsius(temperature_raw, temperature_oversampling)
            .unwrap();
        assert!((temperature as f64 - expected_temperature).abs() < 1e-3);
        let pressure = coefficients
            .pressure_pascals(
                pressure_raw,
                pressure_oversampling,
                temperature_raw,
                temperature_oversampling,
            )
            .unwrap();
        assert!((pressure as f64 - expected_pressure).abs() < 1.0);

        // Every scale factor is the table both sources carry.
        assert_eq!(Oversampling::Times1.scale_factor(), 524_288);
        assert_eq!(Oversampling::Times2.scale_factor(), 1_572_864);
        assert_eq!(Oversampling::Times4.scale_factor(), 3_670_016);
        assert_eq!(Oversampling::Times8.scale_factor(), 7_864_320);
        assert_eq!(Oversampling::Times16.scale_factor(), 253_952);
        assert_eq!(Oversampling::Times32.scale_factor(), 516_096);
        assert_eq!(Oversampling::Times64.scale_factor(), 1_040_384);
        assert_eq!(Oversampling::Times128.scale_factor(), 2_088_960);
    }

    #[test]
    fn raw_values_read_back_signed_24_bit() {
        let mut device = Device::with_page(&PAGE);
        device.registers[REG_PRESSURE as usize..REG_PRESSURE as usize + 3]
            .copy_from_slice(&[0xFF, 0xFB, 0x2E]);
        device.registers[REG_TEMPERATURE as usize..REG_TEMPERATURE as usize + 3]
            .copy_from_slice(&[0x80, 0x00, 0x00]);
        let mut part = Spl06::new(device, ADDRESS);
        assert_eq!(part.read_pressure_raw(), Ok(-1_234));
        assert_eq!(part.read_temperature_raw(), Ok(-8_388_608));

        let mut device = part.into_inner();
        device.registers[REG_PRESSURE as usize..REG_PRESSURE as usize + 3]
            .copy_from_slice(&[0x12, 0x34, 0x56]);
        let mut part = Spl06::new(device, ADDRESS);
        assert_eq!(part.read_pressure_raw(), Ok(1_193_046));

        let mut device = part.into_inner();
        device.registers[REG_PRESSURE as usize..REG_PRESSURE as usize + 3]
            .copy_from_slice(&[0x7F, 0xFF, 0xFF]);
        let mut part = Spl06::new(device, ADDRESS);
        assert_eq!(part.read_pressure_raw(), Ok(8_388_607));
    }

    #[test]
    fn every_call_makes_one_bounded_transfer() {
        let mut part = part(&PAGE);
        part.read_chip_id().unwrap();
        assert_eq!(part.i2c.transactions, [Transaction::Read(REG_CHIP_ID, 1)]);

        part.read_status().unwrap();
        assert_eq!(part.i2c.transactions[1], Transaction::Read(REG_MEAS_CFG, 1));

        part.read_coefficients().unwrap();
        assert_eq!(
            part.i2c.transactions[2],
            Transaction::Read(REG_COEFFICIENTS, 18)
        );

        part.configure(Oversampling::Times16, Oversampling::Times8)
            .unwrap();
        assert_eq!(
            part.i2c.transactions[3],
            Transaction::Write(
                REG_PRESSURE_CFG,
                [Oversampling::Times16.register_bits()].to_vec()
            )
        );
        assert_eq!(
            part.i2c.transactions[4],
            Transaction::Write(REG_TEMPERATURE_CFG, [0x83].to_vec())
        );

        part.start_temperature_measurement().unwrap();
        assert_eq!(
            part.i2c.transactions[5],
            Transaction::Write(REG_MEAS_CFG, [MEAS_TEMPERATURE].to_vec())
        );
        part.start_pressure_measurement().unwrap();
        assert_eq!(
            part.i2c.transactions[6],
            Transaction::Write(REG_MEAS_CFG, [MEAS_PRESSURE].to_vec())
        );

        part.read_temperature_raw().unwrap();
        assert_eq!(
            part.i2c.transactions[7],
            Transaction::Read(REG_TEMPERATURE, 3)
        );
        part.read_pressure_raw().unwrap();
        assert_eq!(part.i2c.transactions[8], Transaction::Read(REG_PRESSURE, 3));
    }

    #[test]
    fn configuration_writes_the_part_registers() {
        let mut part = part(&PAGE);
        part.configure(Oversampling::Times16, Oversampling::Times8)
            .unwrap();
        assert_eq!(
            part.i2c.registers[REG_PRESSURE_CFG as usize],
            Oversampling::Times16.register_bits()
        );
        assert_eq!(
            part.i2c.registers[REG_TEMPERATURE_CFG as usize],
            Oversampling::Times8.register_bits() | TEMPERATURE_EXTERNAL_SENSOR
        );
        assert_eq!(part.read_status(), Ok(Status::default()));

        let mut device = Device::with_page(&PAGE);
        device.registers[REG_MEAS_CFG as usize] =
            PRESSURE_READY | TEMPERATURE_READY | SENSOR_READY | COEFFICIENTS_READY;
        let mut part = Spl06::new(device, ADDRESS);
        assert_eq!(
            part.read_status(),
            Ok(Status {
                pressure_ready: true,
                temperature_ready: true,
                sensor_ready: true,
                coefficients_ready: true,
            })
        );
        assert_eq!(part.read_chip_id(), Ok(CHIP_ID));
    }

    #[test]
    fn the_driver_carries_its_address_and_hands_its_bus_back() {
        let part = part(&PAGE);
        assert_eq!(part.address(), ADDRESS);
        let device = part.into_inner();
        let other = Spl06::new(device, ADDRESS | 0x01);
        assert_eq!(other.address(), 0x77);
        let _ = other.into_inner();
    }

    #[test]
    fn a_bus_failure_is_reported() {
        let mut device = Device::with_page(&PAGE);
        device.fail = true;
        let mut part = Spl06::new(device, ADDRESS);
        assert!(part.read_chip_id().is_err());
        assert!(part.read_coefficients().is_err());
        assert!(
            part.configure(Oversampling::Times1, Oversampling::Times1)
                .is_err()
        );
        assert!(part.read_pressure_raw().is_err());
    }
}
