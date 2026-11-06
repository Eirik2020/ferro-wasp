//! The QST QMC5883L compass over I2C: registers, configuration, and the
//! status the part reports, for `no_std` firmware through `embedded-hal`.
//!
//! Sources: every register address, bit and constant below cites both
//! ArduPilot's `AP_Compass_QMC5883L.cpp` and iNav's `compass_qmc5883l.c`,
//! which agree on everything this crate carries. Two places they do not
//! agree with each other or with themselves are called out where they
//! matter: ArduPilot's read loop gates on the status register's bit 2
//! (which the part defines as data-skipped, not data-ready), so this crate
//! follows iNav and gates on bit 0; and both files carry an `OSR_128`
//! define built from a hex literal that overflows a byte, so the
//! oversampling bits here are the two-bit field the part defines.
//!
//! The part measures continuously once configured, and flags a new field
//! in its status register. The driver reads and reports; it never waits,
//! so pacing reads is the caller's. Every call makes bounded bus work:
//! one write, or one write-read of at most six bytes.
//!
//! The crate never panics on any input, never allocates, and has no
//! unsafe code.
//!
//! Requirements: QMC-001, QMC-002, QMC-003, QMC-004

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's seven-bit address: iNav `QMC5883L_MAG_I2C_ADDRESS 0x0D`;
/// ArduPilot's compass bus for the part is configured at the same address.
pub const ADDRESS: u8 = 0x0d;

/// The ID register reads 0xFF: ArduPilot `QMC5883_ID_VAL 0xFF` against
/// `QMC5883L_REG_ID 0x0D`; iNav `QMC5883_ID_VAL` against
/// `QMC5883L_REG_ID`. One byte, the whole register.
pub const ID: u8 = 0xff;

/// `X LSB` through `Z MSB`, six bytes, every component least significant
/// byte first: both sources read six bytes from register 0.
pub const REG_DATA: u8 = 0x00;
/// The status register: both sources' `QMC5883L_REG_STATUS 0x06`.
pub const REG_STATUS: u8 = 0x06;
/// `CONF1`, the control register: both sources' `QMC5883L_REG_CONF1 0x09`.
pub const REG_CONF1: u8 = 0x09;
/// `CONF2`: both sources' `QMC5883L_REG_CONF2 0x0A`.
pub const REG_CONF2: u8 = 0x0a;
/// The set/reset period register: both sources write `0x0B` with 0x01
/// before configuring the part.
pub const REG_SET_RESET: u8 = 0x0b;
/// The ID register's address, distinct from the part's bus address: both
/// sources' `QMC5883L_REG_ID 0x0D`.
pub const REG_ID: u8 = 0x0d;
/// The set/reset period the part wants: both sources write this value.
pub const SET_RESET_PERIOD: u8 = 0x01;
/// `CONF2` bit 7, the soft-reset command: both sources'
/// `QMC5883L_RST 0x80`.
pub const SOFT_RST: u8 = 0x80;
/// Status bit 0, `DRDY`, a new field is ready: iNav gates its read on it;
/// the part's datasheet defines it. (ArduPilot gates on bit 2 instead,
/// which the part defines as data-skipped.)
pub const DRDY: u8 = 0x01;
/// Status bit 1, `OVL`, the field overflowed the measuring range: the
/// part's datasheet; iNav's header names it.
pub const OVL: u8 = 0x02;
/// Status bit 2, `DOR`, a field was skipped for not reading fast enough:
/// the part's datasheet.
pub const DOR: u8 = 0x04;

/// The standby value `CONF1`'s `MODE` field holds after a reset, which a
/// bring-up can use to tell a live part from a stuck bus: iNav's
/// `deviceDetect` reads `CONF1` and requires this value after a reset
/// (the part is in standby then).
pub const STANDBY: u8 = 0x00;
/// The continuous-measurement value of `CONF1`'s `MODE` field: both
/// sources' `QMC5883L_MODE_CONTINUOUS 0x01`.
pub const CONTINUOUS: u8 = 0x01;

/// The measuring range `CONF1`'s `RNG` field selects: both sources'
/// `QMC5883L_RNG_2G (0x00 << 4)` and `QMC5883L_RNG_8G (0x01 << 4)`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum Range {
    /// ±2 gauss.
    Gauss2,
    /// ±8 gauss, the range both reference drivers configure.
    #[default]
    Gauss8,
}

/// The output data rate `CONF1`'s `ODR` field selects: both sources'
/// defines, 10 to 200 Hz.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum OutputDataRate {
    Hz10,
    Hz50,
    #[default]
    Hz100,
    Hz200,
}

/// The oversampling `CONF1`'s `OSR` field selects: both sources' names,
/// 512 down to 64 samples.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum Oversampling {
    #[default]
    Times512,
    Times256,
    Times128,
    Times64,
}

impl Range {
    /// The `RNG` field's value in `CONF1`, bits 5:4: both sources.
    pub const fn register_bits(self) -> u8 {
        match self {
            Self::Gauss2 => 0 << 4,
            Self::Gauss8 => 1 << 4,
        }
    }

    /// How many raw counts one gauss is at this range. ±8 gauss is
    /// ArduPilot's measured scale (`range_scale = 1000.0f / 3000.0f`
    /// milli-gauss per count); ±2 gauss is the same analog span at a
    /// quarter of the gain (inferred, single-source: the datasheet's
    /// sensitivity table, not quoted here).
    pub const fn counts_per_gauss(self) -> i32 {
        match self {
            Self::Gauss2 => 12_000,
            Self::Gauss8 => 3_000,
        }
    }
}

impl OutputDataRate {
    /// The `ODR` field's value in `CONF1`, bits 3:2: both sources.
    pub const fn register_bits(self) -> u8 {
        match self {
            Self::Hz10 => 0 << 2,
            Self::Hz50 => 1 << 2,
            Self::Hz100 => 2 << 2,
            Self::Hz200 => 3 << 2,
        }
    }
}

impl Oversampling {
    /// The `OSR` field's value in `CONF1`, bits 7:6: the two-bit field the
    /// part defines. (Both reference files build `OSR_128` and `OSR_64`
    /// from hex literals that overflow a byte, so their values there are
    /// not usable.)
    pub const fn register_bits(self) -> u8 {
        match self {
            Self::Times512 => 0 << 6,
            Self::Times256 => 1 << 6,
            Self::Times128 => 2 << 6,
            Self::Times64 => 3 << 6,
        }
    }
}

/// What the status register reports about the last field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Status {
    /// A new field is ready, `DRDY`.
    pub data_ready: bool,
    /// The field overflowed the range, `OVL`.
    pub overflow: bool,
    /// A field was skipped for not reading fast enough, `DOR`.
    pub skipped: bool,
}

impl Status {
    /// The status register, bit for bit.
    pub const fn from_register(status: u8) -> Self {
        Self {
            data_ready: status & DRDY != 0,
            overflow: status & OVL != 0,
            skipped: status & DOR != 0,
        }
    }
}

/// A field in the part's raw units, x, y and z.
pub type Field = [i16; 3];

/// The field in microtesla, each component divided by the range's counts
/// per gauss and scaled from gauss (one gauss is 100 microtesla), or
/// `None` where a component is not finite.
pub fn field_microtesla(field: Field, range: Range) -> Option<[f32; 3]> {
    let counts = range.counts_per_gauss() as f32;
    let mut microtesla = [0.0_f32; 3];
    for (slot, component) in core::iter::zip(&mut microtesla, field) {
        let value = component as f32 / counts * 100.0;
        if !value.is_finite() {
            return None;
        }
        *slot = value;
    }
    Some(microtesla)
}

/// A QMC5883L on an I2C bus, at one address.
pub struct Qmc5883l<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Qmc5883l<I2C> {
    pub const fn new(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    pub fn into_inner(self) -> I2C {
        self.i2c
    }
}

impl<I2C: I2c> Qmc5883l<I2C> {
    /// The ID register, to compare against [`ID`]: one write-read of one
    /// byte.
    pub fn read_id(&mut self) -> Result<u8, I2C::Error> {
        let mut id = [0_u8; 1];
        self.i2c.write_read(self.address, &[REG_ID], &mut id)?;
        Ok(id[0])
    }

    /// The control register, whose value after a reset tells a live part
    /// from a stuck bus (iNav's `deviceDetect`): one write-read.
    pub fn read_conf1(&mut self) -> Result<u8, I2C::Error> {
        let mut conf1 = [0_u8; 1];
        self.i2c
            .write_read(self.address, &[REG_CONF1], &mut conf1)?;
        Ok(conf1[0])
    }

    /// Command a soft reset: one single-byte write (iNav's `deviceDetect`
    /// writes it; ArduPilot defines the same bit).
    pub fn soft_reset(&mut self) -> Result<(), I2C::Error> {
        self.i2c.write(self.address, &[REG_CONF2, SOFT_RST])
    }

    /// Set the set/reset period both reference drivers write before
    /// configuring the part: one single-byte write.
    pub fn set_reset_period(&mut self) -> Result<(), I2C::Error> {
        self.i2c
            .write(self.address, &[REG_SET_RESET, SET_RESET_PERIOD])
    }

    /// Put the part into continuous measurement at the given range, rate
    /// and oversampling: one single-byte write of `CONF1`, the same fields
    /// and values both reference drivers write.
    pub fn configure(
        &mut self,
        range: Range,
        rate: OutputDataRate,
        oversampling: Oversampling,
    ) -> Result<(), I2C::Error> {
        let conf1 = CONTINUOUS
            | range.register_bits()
            | rate.register_bits()
            | oversampling.register_bits();
        self.i2c.write(self.address, &[REG_CONF1, conf1])
    }

    /// The status bits: one write-read of one byte.
    pub fn read_status(&mut self) -> Result<Status, I2C::Error> {
        let mut status = [0_u8; 1];
        self.i2c
            .write_read(self.address, &[REG_STATUS], &mut status)?;
        Ok(Status::from_register(status[0]))
    }

    /// The last field, six bytes as the part packs them, every component
    /// least significant byte first: one write-read of six bytes (both
    /// sources' read).
    pub fn read_field(&mut self) -> Result<Field, I2C::Error> {
        let mut data = [0_u8; 6];
        self.i2c.write_read(self.address, &[REG_DATA], &mut data)?;
        Ok([
            i16::from_le_bytes([data[0], data[1]]),
            i16::from_le_bytes([data[2], data[3]]),
            i16::from_le_bytes([data[4], data[5]]),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};

    /// One transaction the mock saw.
    #[derive(Clone, Debug, PartialEq)]
    enum Transaction {
        Write(u8, std::vec::Vec<u8>),
        Read(u8, usize),
    }

    /// A QMC5883L as a plain register file, recording every transaction.
    struct Device {
        registers: [u8; 16],
        fail: bool,
        transactions: std::vec::Vec<Transaction>,
    }

    impl Device {
        fn new() -> Self {
            let mut registers = [0_u8; 16];
            registers[REG_ID as usize] = ID;
            Self {
                registers,
                fail: false,
                transactions: std::vec::Vec::new(),
            }
        }

        fn with_field(&mut self, field: Field) {
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
                    if let Some(byte) = bytes.get(1).copied() {
                        self.registers[register as usize] = byte;
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

    fn part() -> Qmc5883l<Device> {
        Qmc5883l::new(Device::new(), ADDRESS)
    }

    #[test]
    fn every_call_makes_one_bounded_transfer() {
        let mut part = part();
        part.read_id().unwrap();
        assert_eq!(part.i2c.transactions, [Transaction::Read(REG_ID, 1)]);
        part.read_conf1().unwrap();
        assert_eq!(part.i2c.transactions[1], Transaction::Read(REG_CONF1, 1));
        part.soft_reset().unwrap();
        assert_eq!(
            part.i2c.transactions[2],
            Transaction::Write(REG_CONF2, [SOFT_RST].to_vec())
        );
        part.set_reset_period().unwrap();
        assert_eq!(
            part.i2c.transactions[3],
            Transaction::Write(REG_SET_RESET, [SET_RESET_PERIOD].to_vec())
        );
        part.configure(Range::Gauss8, OutputDataRate::Hz200, Oversampling::Times512)
            .unwrap();
        assert_eq!(
            part.i2c.transactions[4],
            Transaction::Write(REG_CONF1, [0x1d].to_vec())
        );
        part.read_status().unwrap();
        assert_eq!(part.i2c.transactions[5], Transaction::Read(REG_STATUS, 1));
        part.read_field().unwrap();
        assert_eq!(part.i2c.transactions[6], Transaction::Read(REG_DATA, 6));
    }

    #[test]
    fn configuration_writes_the_part_registers() {
        let mut part = part();
        part.soft_reset().unwrap();
        part.set_reset_period().unwrap();
        assert_eq!(part.i2c.registers[REG_CONF2 as usize], SOFT_RST);
        assert_eq!(part.i2c.registers[REG_SET_RESET as usize], SET_RESET_PERIOD);

        // Every field lands where the part expects it.
        part.configure(Range::Gauss2, OutputDataRate::Hz10, Oversampling::Times512)
            .unwrap();
        assert_eq!(part.i2c.registers[REG_CONF1 as usize], CONTINUOUS);
        part.configure(Range::Gauss8, OutputDataRate::Hz50, Oversampling::Times256)
            .unwrap();
        assert_eq!(
            part.i2c.registers[REG_CONF1 as usize],
            CONTINUOUS | 0x10 | 0x04 | 0x40
        );
        part.configure(Range::Gauss2, OutputDataRate::Hz100, Oversampling::Times128)
            .unwrap();
        assert_eq!(
            part.i2c.registers[REG_CONF1 as usize],
            CONTINUOUS | 0x08 | 0x80
        );
        part.configure(Range::Gauss8, OutputDataRate::Hz200, Oversampling::Times64)
            .unwrap();
        assert_eq!(
            part.i2c.registers[REG_CONF1 as usize],
            CONTINUOUS | 0x10 | 0x0c | 0xc0
        );
        assert_eq!(part.read_id(), Ok(ID));
        assert_eq!(part.read_conf1(), Ok(CONTINUOUS | 0x10 | 0x0c | 0xc0));
    }

    #[test]
    fn status_decodes_bit_for_bit() {
        assert_eq!(Status::from_register(0), Status::default());
        assert_eq!(
            Status::from_register(DRDY),
            Status {
                data_ready: true,
                overflow: false,
                skipped: false,
            }
        );
        assert_eq!(
            Status::from_register(DRDY | OVL),
            Status {
                data_ready: true,
                overflow: true,
                skipped: false,
            }
        );
        assert_eq!(
            Status::from_register(DOR),
            Status {
                data_ready: false,
                overflow: false,
                skipped: true,
            }
        );

        let mut part = part();
        part.i2c.registers[REG_STATUS as usize] = DRDY | DOR;
        assert_eq!(
            part.read_status(),
            Ok(Status {
                data_ready: true,
                overflow: false,
                skipped: true,
            })
        );
    }

    #[test]
    fn a_field_reads_back_little_endian_signed_and_scaled() {
        let mut part = part();
        part.i2c.with_field([0x1234, -4096, -2]);
        assert_eq!(part.read_field(), Ok([0x1234, -4096, -2]));
        // Little endian, least significant byte first.
        assert_eq!(
            &part.i2c.registers[REG_DATA as usize..REG_DATA as usize + 6],
            &[0x34, 0x12, 0x00, 0xf0, 0xfe, 0xff]
        );

        // One gauss is the range's counts per gauss, and one gauss is 100
        // microtesla. The 8-gauss scale is ArduPilot's measured one.
        assert_eq!(
            field_microtesla([3_000, -6_000, 0], Range::Gauss8),
            Some([100.0, -200.0, 0.0])
        );
        // The 2-gauss range carries four times the resolution (inferred,
        // single-source).
        assert_eq!(
            field_microtesla([12_000, -24_000, 0], Range::Gauss2),
            Some([100.0, -200.0, 0.0])
        );
        assert_eq!(
            field_microtesla([-3_000, 3_000, 12_000], Range::Gauss2),
            Some([-25.0, 25.0, 100.0])
        );
    }

    #[test]
    fn a_bus_failure_is_reported() {
        let mut part = part();
        part.i2c.fail = true;
        assert!(part.read_id().is_err());
        assert!(part.read_status().is_err());
        assert!(part.read_field().is_err());
        assert!(
            part.configure(Range::Gauss8, OutputDataRate::Hz100, Oversampling::Times512)
                .is_err()
        );
        assert!(part.soft_reset().is_err());
    }
}
