//! The Infineon DPS310, and the DPS368 that shares its interface, over
//! I2C: registers, the calibration coefficient page, and the part's
//! compensation formulas, for `no_std` firmware through `embedded-hal`.
//!
//! Sources: every register address, bit and constant below cites iNav's
//! `barometer_dps310.c`, which this crate follows through its own
//! configure sequence, quoted register by register. The board facts - the
//! address, and that the DPS368 runs the DPS310 driver - cite ArduPilot's
//! `TBS_LUCID_H7` hwdef: "All H7 boards have footprints for either DPS368
//! (DPS310 driver) or BMP390 (BMP388 driver) at I2C 0x76 on I2C2; only
//! one is populated per build", with `BARO DPS310 I2C:0:0x76`. ArduPilot
//! carries no DPS310 driver of its own in `libraries/AP_Baro` (only the
//! DPS280), so the driver mechanics here are *single-source* to iNav,
//! and such a fact needs checking against the part's datasheet and a
//! board. Where the SPL06-007 crate carries the same fact - the parts are
//! register clones, below - it is cited as the second source.
//!
//! The DPS310 is register-compatible with the Goertek SPL06-007: the same
//! register addresses, the same `0x10` chip ID at `0x0D` (iNav's DPS310
//! driver accepts `0x10` for the DPS310 and `0x11` for the related
//! SPL07-003; its SPL06 driver checks the same `0x10`), the same
//! 18-byte coefficient page packed the same way, the same measurement
//! command values, and the same compensation formulas and scale-factor
//! table - iNav's two decodes agree byte for byte. The ID register
//! therefore *cannot* tell a DPS310 or DPS368 from an SPL06-007; a board
//! states which part its footprint carries, and the probe reports what
//! answered. What the DPS310 adds: a soft-reset register (`0x0C`), boot
//! readiness bits in `MEAS_CFG`, a temperature-coefficient source bit at
//! `0x28` that `TMP_CFG`'s external-sensor bit must match, and a
//! fuse-bit workaround for a wrong-temperature IC revision.
//!
//! The part measures on command: a write to `MEAS_CFG` starts one
//! pressure or one temperature measurement, and the matching ready bit
//! appears in `MEAS_CFG` when it completes. The driver starts and
//! reads; it never waits, so pacing a measurement is the caller's - and
//! the first temperature measurement after `configure` changes
//! `TMP_CFG`, so iNav flushes it ("Make ONE temperature measurement and
//! flush it"); the caller must discard that first temperature result
//! the same way. Every call makes bounded bus work: one write, or one
//! write-read of at most 18 bytes.
//!
//! The crate never panics on any input, never allocates, and has no
//! unsafe code.
//!
//! Requirements: DPS310-001, DPS310-002, DPS310-003, DPS310-004,
//! DPS310-005

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's seven-bit address on the boards that carry it: ArduPilot's
/// `TBS_LUCID_H7` hwdef probes `BARO DPS310 I2C:0:0x76`. The DPS310's
/// SDO pin straps this address or its complement, as the SPL06-007's
/// address pin does (single-source: the datasheet's pin description; the
/// probe tries both strappings).
pub const ADDRESS: u8 = 0x76;

/// The chip ID register reads `0x10`: iNav
/// `DPS310_ID_REV_AND_PROD_ID (0x10)`. The SPL06-007 answers the same
/// `0x10` here and the SPL07-003 answers `0x11`
/// (`SPL07_003_CHIP_ID`), so the ID alone does not name the part - the
/// board's facts do.
pub const CHIP_ID: u8 = 0x10;

/// `PSR_B2` to `PSR_B0`, MSB first: iNav `DPS310_REG_PSR_B2 0x00`
/// through `DPS310_REG_PSR_B0 0x02`; the SPL06-007's `PRS_B2` at the
/// same addresses.
pub const REG_PRESSURE: u8 = 0x00;
/// `TMP_B2` to `TMP_B0`, MSB first: iNav `DPS310_REG_TMP_B2 0x03`
/// through `DPS310_REG_TMP_B0 0x05`.
pub const REG_TEMPERATURE: u8 = 0x03;
/// `PRS_CFG`: iNav `DPS310_REG_PRS_CFG 0x06`.
pub const REG_PRESSURE_CFG: u8 = 0x06;
/// `TMP_CFG`: iNav `DPS310_REG_TMP_CFG 0x07`.
pub const REG_TEMPERATURE_CFG: u8 = 0x07;
/// `MEAS_CFG`, the mode and status register: iNav
/// `DPS310_REG_MEAS_CFG 0x08`.
pub const REG_MEAS_CFG: u8 = 0x08;
/// `CFG_REG`, the interrupt and result-shift register: iNav
/// `DPS310_REG_CFG_REG 0x09`. Its shift bits matter only above 8x
/// oversampling, which this driver never configures.
pub const REG_CFG_REG: u8 = 0x09;
/// The soft-reset register: iNav `DPS310_REG_RESET 0x0C`, its
/// `DPS310_RESET_BIT_SOFT_RST (0x09)`. A soft reset needs the part's
/// boot time before its coefficients are ready again, and the boot
/// readiness bits below say when, so the probe never resets: it asks
/// for the part as it stands, and the caller re-probes until the part
/// is ready.
pub const REG_RESET: u8 = 0x0c;
/// The chip ID register: iNav `DPS310_REG_ID 0x0D`.
pub const REG_CHIP_ID: u8 = 0x0d;
/// The calibration page: iNav `DPS310_REG_COEF 0x10`.
pub const REG_COEFFICIENTS: u8 = 0x10;
/// The calibration page's length in bytes, 18: iNav reads
/// `coefficientLength = ... 18` for the DPS310.
pub const COEFFICIENTS_LEN: usize = 18;
/// The temperature-coefficient source register: iNav
/// `DPS310_REG_COEF_SRCE 0x28`.
pub const REG_COEF_SOURCE: u8 = 0x28;

/// `MEAS_CFG` bits 2:0, `MEAS_CTRL`, the idle command: iNav
/// `DPS310_MEAS_CFG_MEAS_IDLE (0x0)`.
pub const MEAS_IDLE: u8 = 0x0;
/// `MEAS_CFG` bits 2:0, the command for one pressure measurement: the
/// SPL06-007's `MEAS_PRESSURE (1<<0)` at the same register, which the
/// parts share (single-source against the DPS310 datasheet's mode
/// table; the register-clone note above).
pub const MEAS_PRESSURE: u8 = 1 << 0;
/// `MEAS_CFG` bits 2:0, the command for one temperature measurement:
/// iNav `DPS310_MEAS_CFG_MEAS_TEMP_SING (0x2)`.
pub const MEAS_TEMPERATURE: u8 = 1 << 1;
/// `MEAS_CFG` bits 2:0, background pressure and temperature: iNav
/// `DPS310_MEAS_CFG_MEAS_CONT (0x7)`. This driver never enters it.
pub const MEAS_BACKGROUND: u8 = 0x7;
/// `MEAS_CFG` bit 4, `PRS_RDY`: iNav `DPS310_MEAS_CFG_PRS_RDY (1<<4)`.
pub const PRESSURE_READY: u8 = 1 << 4;
/// `MEAS_CFG` bit 5, `TMP_RDY`: iNav `DPS310_MEAS_CFG_TMP_RDY (1<<5)`.
pub const TEMPERATURE_READY: u8 = 1 << 5;
/// `MEAS_CFG` bit 6, `SENSOR_RDY`, the sensor initialization is
/// complete: iNav `DPS310_MEAS_CFG_SENSOR_RDY (1<<6)`.
pub const SENSOR_READY: u8 = 1 << 6;
/// `MEAS_CFG` bit 7, `COEF_RDY`, the calibration page is available:
/// iNav `DPS310_MEAS_CFG_COEF_RDY (1<<7)`.
pub const COEFFICIENTS_READY: u8 = 1 << 7;
/// `TMP_CFG` bit 7, use the part's external temperature sensor: iNav
/// `DPS310_TMP_CFG_BIT_TMP_EXT (0x80)`. It must match the coefficient
/// source bit below, as iNav's configure copies it through.
pub const TEMPERATURE_EXTERNAL_SENSOR: u8 = 1 << 7;
/// `COEF_SRCE` bit 7, the temperature coefficients were measured on
/// the part's external sensor: iNav `DPS310_COEF_SRCE_BIT_TMP_COEF_SRCE
/// (0x80)`, which its configure reads and propagates into `TMP_CFG`'s
/// external-sensor bit.
pub const TEMPERATURE_SOURCE_EXTERNAL: u8 = 1 << 7;

/// How many times a measurement oversamples: the `PRS_CFG`/`TMP_CFG`
/// oversampling field, bits 2:0 (iNav's `PM_PRC_16`/`TMP_PRC_16` both
/// sit there), 1 to 128 as powers of two. The result-shift bits in
/// `CFG_REG` (`DPS310_CFG_REG_BIT_T_SHIFT`, `P_SHIFT`) apply only above
/// 8 times, which this driver never configures.
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
    /// The field's value in `PRS_CFG` and `TMP_CFG`, bits 2:0: iNav's
    /// `PRC_16` is `0x04` there, the fourth power of two.
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

    /// The raw-value scale factor the part's compensation divides by:
    /// iNav's `kT`/`kP` are `253952` at 16 times, and the table is the
    /// SPL06-007's own (`raw_value_scale_factor`), the parts' shared
    /// scale-factor table (Table 9 of the DPS310 datasheet, via iNav).
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

/// A two's-complement `width`-bit field widened to `i32`, as iNav's
/// `getTwosComplement` does.
const fn sign_extend(bits: u32, width: u32) -> i32 {
    if bits & (1 << (width - 1)) != 0 {
        (bits as i32) - (1 << width)
    } else {
        bits as i32
    }
}

/// The part's calibration coefficients, one page of 18 bytes at
/// `COEFFICIENTS` (iNav's `deviceConfigure`, quoted register by
/// register: `0x10 c0 [11:4]`, `0x11 c0 [3:0] + c1 [11:8]`, `0x12 c1
/// [7:0]`, and so on through `0x20 c30 [15:8] + 0x21 c30 [7:0]`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Coefficients {
    /// 12-bit.
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
    /// complement, most significant byte first - iNav's decode, quoted
    /// in the register comments above, which the SPL06-007's own page
    /// matches byte for byte.
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

    /// Compensated temperature in degrees Celsius, `c0/2 + raw/scale *
    /// c1` (iNav, citing the datasheet's compensated-temperature
    /// section), or `None` where the value is not finite.
    pub fn temperature_celsius(&self, raw: i32, oversampling: Oversampling) -> Option<f32> {
        let scaled = raw as f32 / oversampling.scale_factor() as f32;
        let value = self.c0 as f32 / 2.0 + scaled * self.c1 as f32;
        finite(value)
    }

    /// Compensated pressure in pascals, the part's formula (iNav,
    /// citing the datasheet's compensated-pressure section):
    /// `c00 + pr*(c10 + pr*(c20 + pr*c30)) + tr*(c01 + pr*(c11 +
    /// pr*c21))` with `pr` the pressure raw divided by its scale
    /// factor and `tr` the temperature raw divided by its own. `None`
    /// where not finite.
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

/// The state of `MEAS_CFG`'s status bits: what is ready (iNav's bit
/// definitions).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Status {
    pub pressure_ready: bool,
    pub temperature_ready: bool,
    /// The sensor initialization is complete: a part still booting has
    /// this and `coefficients_ready` clear, so the caller re-probes.
    pub sensor_ready: bool,
    /// The calibration page is available to read.
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

/// A DPS310, or the DPS368 that shares its interface, on an I2C bus,
/// at one address.
#[derive(Debug)]
pub struct Dps310<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Dps310<I2C> {
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

impl<I2C: I2c> Dps310<I2C> {
    /// The chip ID register, to compare against [`CHIP_ID`]: one
    /// write-read of one byte.
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

    /// The temperature-coefficient source: one write-read of one byte,
    /// `true` where the coefficients were measured on the external
    /// sensor. iNav's configure reads this and propagates it into
    /// `TMP_CFG`'s external-sensor bit, and so does `configure`.
    pub fn read_temperature_source(&mut self) -> Result<bool, I2C::Error> {
        let mut source = [0_u8; 1];
        self.i2c
            .write_read(self.address, &[REG_COEF_SOURCE], &mut source)?;
        Ok(source[0] & TEMPERATURE_SOURCE_EXTERNAL != 0)
    }

    /// The calibration page, all 18 bytes at once: one write-read.
    pub fn read_coefficients(&mut self) -> Result<Coefficients, I2C::Error> {
        let mut page = [0_u8; COEFFICIENTS_LEN];
        self.i2c
            .write_read(self.address, &[REG_COEFFICIENTS], &mut page)?;
        Ok(Coefficients::decode(&page))
    }

    /// Command the mode idle, run the part's fuse-bit workaround, and
    /// set both oversampling fields with `TMP_CFG`'s external-sensor
    /// bit matched to the coefficient source: nine single-byte writes,
    /// in iNav's order. The rate fields are written whole, which also
    /// clears a previous session's background-mode rate bits; the
    /// result-shift bits stay off, as the oversampling configured here
    /// never exceeds 8 times.
    pub fn configure(
        &mut self,
        pressure: Oversampling,
        temperature: Oversampling,
        temperature_external: bool,
    ) -> Result<(), I2C::Error> {
        // Idle first, as iNav's configure does, so a part left in a
        // previous session's background mode takes commands again.
        self.i2c.write(self.address, &[REG_MEAS_CFG, MEAS_IDLE])?;
        // "Fix IC with a fuse bit problem, which lead to a wrong
        // temperature. Should not affect ICs without this problem" -
        // iNav's five writes, verbatim.
        self.i2c.write(self.address, &[0x0e, 0xa5])?;
        self.i2c.write(self.address, &[0x0f, 0x96])?;
        self.i2c.write(self.address, &[0x62, 0x02])?;
        self.i2c.write(self.address, &[0x0e, 0x00])?;
        self.i2c.write(self.address, &[0x0f, 0x00])?;
        self.i2c
            .write(self.address, &[REG_PRESSURE_CFG, pressure.register_bits()])?;
        let external = if temperature_external {
            TEMPERATURE_EXTERNAL_SENSOR
        } else {
            0
        };
        self.i2c.write(
            self.address,
            &[REG_TEMPERATURE_CFG, temperature.register_bits() | external],
        )?;
        Ok(())
    }

    /// Start one temperature measurement: one single-byte write. The
    /// first after `configure` changes `TMP_CFG` and must be flushed -
    /// iNav does, and the caller discards that first result.
    pub fn start_temperature_measurement(&mut self) -> Result<(), I2C::Error> {
        self.i2c
            .write(self.address, &[REG_MEAS_CFG, MEAS_TEMPERATURE])
    }

    /// Start one pressure measurement: one single-byte write.
    pub fn start_pressure_measurement(&mut self) -> Result<(), I2C::Error> {
        self.i2c.write(self.address, &[REG_MEAS_CFG, MEAS_PRESSURE])
    }

    /// The last temperature measurement's raw value, the part's 24-bit
    /// two's complement, MSB first, sign extended: one write-read of
    /// three bytes (iNav reads pressure and temperature together from
    /// `PSR_B2`; the registers sit contiguously, so the per-register
    /// reads here see the same bytes).
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
/// sign-extension iNav's `getTwosComplement` applies to a raw value.
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
    use std::vec;

    /// One transaction the mock saw: a write of `bytes` to `register`,
    /// or a read of `len` bytes from it.
    #[derive(Clone, Debug, PartialEq)]
    enum Transaction {
        Write(u8, std::vec::Vec<u8>),
        Read(u8, usize),
    }

    /// A DPS310 as a plain register file, recording every transaction.
    /// The fuse-bit workaround's `0x62` sits above the page, so the file
    /// spans it.
    struct Device {
        registers: [u8; 128],
        fail: bool,
        transactions: std::vec::Vec<Transaction>,
    }

    impl Default for Device {
        fn default() -> Self {
            let mut device = Self {
                registers: [0; 128],
                fail: false,
                transactions: std::vec::Vec::new(),
            };
            // A part that has finished booting, idle between commands.
            device.registers[REG_MEAS_CFG as usize] =
                SENSOR_READY | COEFFICIENTS_READY | PRESSURE_READY | TEMPERATURE_READY;
            device
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

    /// A calibration page with negative fields in the packed positions:
    /// `c0 = -2` (0xFFE), `c1 = -3` (0xFFD), `c00 = c10 = 25000`
    /// (0x61A8, 20-bit), `c01 = c11 = c21 = -1`, `c20 = c30 = 1`,
    /// packed by hand the way iNav's register comments quote the
    /// layout.
    const PAGE: [u8; COEFFICIENTS_LEN] = [
        0xff, 0xef, 0xfd, // c0 = 0xFFE (-2), c1 = 0xFFD (-3)
        0x06, 0x1a, 0x80, // c00 = 0x61A8 (25000), c10's top nibble 0
        0x61, 0xa8, // c10 = 0x61A8 (25000), low bytes
        0xff, 0xff, // c01 = -1
        0xff, 0xff, // c11 = -1
        0x00, 0x01, // c20 = 1
        0xff, 0xff, // c21 = -1
        0x00, 0x01, // c30 = 1
    ];

    #[test]
    fn the_page_decodes_the_packed_fields() {
        let coefficients = Coefficients::decode(&PAGE);
        assert_eq!(coefficients.c0(), -2);
        assert_eq!(coefficients.c1(), -3);
        assert_eq!(coefficients.c00(), 25_000);
        assert_eq!(coefficients.c10(), 25_000);
        assert_eq!(coefficients.c01(), -1);
        assert_eq!(coefficients.c11(), -1);
        assert_eq!(coefficients.c20(), 1);
        assert_eq!(coefficients.c21(), -1);
        assert_eq!(coefficients.c30(), 1);
    }

    #[test]
    fn compensation_matches_the_parts_formulas() {
        let coefficients = Coefficients::decode(&PAGE);
        // Temperature: c0/2 + raw/scale * c1 = -1 + 0 at raw 0.
        assert_eq!(
            coefficients.temperature_celsius(0, Oversampling::Times1),
            Some(-1.0)
        );
        // Pressure at zero raws: exactly c00.
        assert_eq!(
            coefficients.pressure_pascals(0, Oversampling::Times1, 0, Oversampling::Times1),
            Some(25_000.0)
        );
        assert!(
            Coefficients::default()
                .temperature_celsius(0, Oversampling::Times1)
                .is_some()
        );
    }

    #[test]
    fn the_status_bits_map_the_meas_cfg_byte() {
        let status = Status::from_register(SENSOR_READY | COEFFICIENTS_READY);
        assert!(status.sensor_ready && status.coefficients_ready);
        assert!(!status.pressure_ready && !status.temperature_ready);
        let booting = Status::from_register(0);
        assert!(!booting.sensor_ready && !booting.coefficients_ready);
    }

    #[test]
    fn the_scale_factors_match_the_shared_table() {
        // iNav's kT/kP at 16 times is 253952; the rest is the table the
        // SPL06-007 crate also carries.
        assert_eq!(Oversampling::Times16.scale_factor(), 253_952);
        assert_eq!(Oversampling::Times1.scale_factor(), 524_288);
        assert_eq!(Oversampling::Times1.register_bits(), 0);
        assert_eq!(Oversampling::Times16.register_bits(), 4);
    }

    #[test]
    fn the_driver_speaks_the_part_registers() {
        let device = Device::with_page(&PAGE);
        let mut part = Dps310::new(device, ADDRESS);

        assert_eq!(part.read_chip_id(), Ok(CHIP_ID));
        part.read_status().unwrap();
        assert_eq!(part.read_temperature_source(), Ok(false));
        part.read_coefficients().unwrap();
        part.configure(Oversampling::Times1, Oversampling::Times1, false)
            .unwrap();
        part.start_temperature_measurement().unwrap();
        part.read_temperature_raw().unwrap();
        part.start_pressure_measurement().unwrap();
        part.read_pressure_raw().unwrap();

        assert_eq!(
            part.i2c.transactions,
            [
                Transaction::Read(REG_CHIP_ID, 1),
                Transaction::Read(REG_MEAS_CFG, 1),
                Transaction::Read(REG_COEF_SOURCE, 1),
                Transaction::Read(REG_COEFFICIENTS, 18),
                Transaction::Write(REG_MEAS_CFG, vec![MEAS_IDLE]),
                Transaction::Write(0x0e, vec![0xa5]),
                Transaction::Write(0x0f, vec![0x96]),
                Transaction::Write(0x62, vec![0x02]),
                Transaction::Write(0x0e, vec![0x00]),
                Transaction::Write(0x0f, vec![0x00]),
                Transaction::Write(REG_PRESSURE_CFG, vec![Oversampling::Times1.register_bits()]),
                Transaction::Write(
                    REG_TEMPERATURE_CFG,
                    vec![Oversampling::Times1.register_bits()]
                ),
                Transaction::Write(REG_MEAS_CFG, vec![MEAS_TEMPERATURE]),
                Transaction::Read(REG_TEMPERATURE, 3),
                Transaction::Write(REG_MEAS_CFG, vec![MEAS_PRESSURE]),
                Transaction::Read(REG_PRESSURE, 3),
            ]
        );
    }

    #[test]
    fn configuration_writes_the_external_bit_from_the_source() {
        let mut device = Device::with_page(&PAGE);
        device.registers[REG_COEF_SOURCE as usize] = TEMPERATURE_SOURCE_EXTERNAL;
        let mut part = Dps310::new(device, ADDRESS);
        assert_eq!(part.read_temperature_source(), Ok(true));
        part.configure(Oversampling::Times1, Oversampling::Times1, true)
            .unwrap();
        assert_eq!(
            part.i2c.registers[REG_TEMPERATURE_CFG as usize],
            Oversampling::Times1.register_bits() | TEMPERATURE_EXTERNAL_SENSOR
        );
        // The idle command cleared the boot-time ready bits the mock
        // planted, and the fuse fix's registers hold their last writes.
        assert_eq!(part.i2c.registers[REG_MEAS_CFG as usize], MEAS_IDLE);
        assert_eq!(part.i2c.registers[0x62], 0x02);
    }

    #[test]
    fn the_driver_carries_its_address_and_hands_its_bus_back() {
        let part = Dps310::new(Device::default(), ADDRESS);
        assert_eq!(part.address(), ADDRESS);
        let device = part.into_inner();
        let other = Dps310::new(device, ADDRESS | 0x01);
        assert_eq!(other.address(), 0x77);
        let _ = other.into_inner();
    }

    #[test]
    fn a_bus_failure_is_reported() {
        let mut device = Device::with_page(&PAGE);
        device.fail = true;
        let mut part = Dps310::new(device, ADDRESS);
        assert!(part.read_chip_id().is_err());
        assert!(part.read_status().is_err());
        assert!(part.read_temperature_source().is_err());
        assert!(part.read_coefficients().is_err());
        assert!(
            part.configure(Oversampling::Times1, Oversampling::Times1, false)
                .is_err()
        );
        assert!(part.read_pressure_raw().is_err());
    }
}
