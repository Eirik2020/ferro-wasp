//! Allocation-free SPI driver for the TDK InvenSense MPU-6000.
//!
//! The default configuration matches the 2 kHz rate loop the flight boards
//! run: +/-2000 dps gyro, +/-16 g accelerometer, and a 2 kHz data-ready
//! pulse. The 14-byte accelerometer/temperature/gyroscope payload starts at
//! `ACCEL_XOUT_H`, the same register and layout as the MPU6500, and fits the
//! existing 15-byte full-duplex SPI DMA frame.
//!
//! The part accepts 1 MHz SPI for every register and 20 MHz only for sensor
//! and interrupt reads. FerroWasp's IMU buses run at 1 MHz throughout, so a
//! 15-byte burst takes about 120 us: 24% of a 2 kHz period, 48% at 4 kHz, and
//! too long for 8 kHz.

use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiBus};

pub const WHO_AM_I_EXPECTED: u8 = 0x68;
pub const SPI_BURST_SIZE: usize = 15;
pub const SPI_READ_BIT: u8 = 0x80;

pub const TEMPERATURE_LSB_PER_C: f32 = 340.0;
pub const TEMPERATURE_OFFSET_C: f32 = 36.53;

const DEVICE_RESET: u8 = 1 << 7;
const GYRO_ACCEL_TEMP_SIGNAL_PATH_RESET: u8 = 0b0000_0111;
/// PLL referenced to the Z gyro, sleep and cycle cleared. The datasheet
/// recommends a gyro reference over the internal 8 MHz oscillator.
const CLKSEL_PLL_GYRO_Z: u8 = 0x03;
const I2C_IF_DIS: u8 = 1 << 4;
const ALL_AXES_ENABLED: u8 = 0;
/// Active-high, push-pull, 50 us pulse, cleared by any read.
const INT_ACTIVE_HIGH_PUSH_PULL_PULSED: u8 = 1 << 4;
const DATA_RDY_EN: u8 = 1 << 0;

/// The SPI reset sequence the register map requires: `DEVICE_RESET`, then a
/// signal-path reset, each followed by 100 ms.
const RESET_SETTLE_MS: u32 = 100;
const CLOCK_SETTLE_MS: u32 = 10;
/// Gyro start-up is 30 ms typical and 100 ms maximum from sleep.
const GYRO_STARTUP_MS: u32 = 100;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Register {
    SmplrtDiv = 0x19,
    Config = 0x1a,
    GyroConfig = 0x1b,
    AccelConfig = 0x1c,
    IntPinCfg = 0x37,
    IntEnable = 0x38,
    IntStatus = 0x3a,
    AccelXoutH = 0x3b,
    SignalPathReset = 0x68,
    UserCtrl = 0x6a,
    PwrMgmt1 = 0x6b,
    PwrMgmt2 = 0x6c,
    WhoAmI = 0x75,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GyroFullScale {
    Dps250 = 0,
    Dps500 = 1,
    Dps1000 = 2,
    Dps2000 = 3,
}

impl GyroFullScale {
    pub const fn lsb_per_dps(self) -> f32 {
        match self {
            Self::Dps250 => 131.0,
            Self::Dps500 => 65.5,
            Self::Dps1000 => 32.8,
            Self::Dps2000 => 16.4,
        }
    }

    const fn register_bits(self) -> u8 {
        (self as u8) << 3
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccelFullScale {
    G2 = 0,
    G4 = 1,
    G8 = 2,
    G16 = 3,
}

impl AccelFullScale {
    pub const fn lsb_per_g(self) -> f32 {
        match self {
            Self::G2 => 16_384.0,
            Self::G4 => 8_192.0,
            Self::G8 => 4_096.0,
            Self::G16 => 2_048.0,
        }
    }

    const fn register_bits(self) -> u8 {
        (self as u8) << 3
    }
}

/// The data-ready rate, and the filter that rate requires.
///
/// The gyro samples internally at 8 kHz only with its DLPF bypassed
/// (`DLPF_CFG = 0`, 256 Hz gyro bandwidth); every other DLPF setting drops it
/// to 1 kHz. Rates above 1 kHz therefore run the 256 Hz filter and decimate
/// the 8 kHz stream with `SMPLRT_DIV`. The accelerometer updates at 1 kHz
/// regardless, so above that rate consecutive samples repeat its value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputDataRate {
    /// `DLPF_CFG = 1`: 188 Hz gyro bandwidth, 1.9 ms delay.
    Hz1000,
    /// `DLPF_CFG = 0`: 256 Hz gyro bandwidth, 0.98 ms delay.
    Hz2000,
    /// `DLPF_CFG = 0`: 256 Hz gyro bandwidth, 0.98 ms delay.
    Hz4000,
}

impl OutputDataRate {
    pub const fn hz(self) -> u32 {
        match self {
            Self::Hz1000 => 1_000,
            Self::Hz2000 => 2_000,
            Self::Hz4000 => 4_000,
        }
    }

    const fn dlpf_cfg(self) -> u8 {
        match self {
            Self::Hz1000 => 1,
            Self::Hz2000 | Self::Hz4000 => 0,
        }
    }

    const fn smplrt_div(self) -> u8 {
        let gyro_output_rate_hz = match self.dlpf_cfg() {
            0 => 8_000,
            _ => 1_000,
        };
        (gyro_output_rate_hz / self.hz() - 1) as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub gyro_full_scale: GyroFullScale,
    pub accel_full_scale: AccelFullScale,
    pub output_data_rate: OutputDataRate,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gyro_full_scale: GyroFullScale::Dps2000,
            accel_full_scale: AccelFullScale::G16,
            output_data_rate: OutputDataRate::Hz2000,
        }
    }
}

impl Config {
    const fn config(self) -> u8 {
        self.output_data_rate.dlpf_cfg()
    }

    const fn smplrt_div(self) -> u8 {
        self.output_data_rate.smplrt_div()
    }

    const fn gyro_config(self) -> u8 {
        self.gyro_full_scale.register_bits()
    }

    const fn accel_config(self) -> u8 {
        self.accel_full_scale.register_bits()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    FrameTooShort,
    ImplausibleFrame,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error<SpiE, PinE> {
    Spi(SpiE),
    Pin(PinE),
    InvalidWhoAmI(u8),
    RegisterVerification {
        register: Register,
        expected: u8,
        observed: u8,
    },
    Decode(DecodeError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuBurstSample {
    pub acc_raw: [i16; 3],
    pub temp_raw: i16,
    pub gyro_raw: [i16; 3],
}

impl ImuBurstSample {
    pub fn temperature_c(self) -> f32 {
        self.temp_raw as f32 / TEMPERATURE_LSB_PER_C + TEMPERATURE_OFFSET_C
    }

    pub fn accel_g(self, scale: AccelFullScale) -> [f32; 3] {
        let divisor = scale.lsb_per_g();
        [
            self.acc_raw[0] as f32 / divisor,
            self.acc_raw[1] as f32 / divisor,
            self.acc_raw[2] as f32 / divisor,
        ]
    }

    pub fn gyro_dps(self, scale: GyroFullScale) -> [f32; 3] {
        let divisor = scale.lsb_per_dps();
        [
            self.gyro_raw[0] as f32 / divisor,
            self.gyro_raw[1] as f32 / divisor,
            self.gyro_raw[2] as f32 / divisor,
        ]
    }
}

fn with_chip_select<SPI, CS, T, F>(
    spi: &mut SPI,
    cs: &mut CS,
    operation: F,
) -> Result<T, Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    F: FnOnce(&mut SPI) -> Result<T, SPI::Error>,
{
    if let Err(error) = cs.set_low() {
        let _ = cs.set_high();
        return Err(Error::Pin(error));
    }

    let operation_result = operation(spi).map_err(Error::Spi);
    let deselect_result = cs.set_high().map_err(Error::Pin);

    match operation_result {
        Ok(value) => deselect_result.map(|()| value),
        Err(error) => {
            let _ = deselect_result;
            Err(error)
        }
    }
}

pub fn write_reg<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
    register: Register,
    value: u8,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    with_chip_select(spi, cs, |spi| {
        spi.write(&[(register as u8) & !SPI_READ_BIT, value])?;
        spi.flush()
    })
}

pub fn read_reg<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
    register: Register,
) -> Result<u8, Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let mut frame = [(register as u8) | SPI_READ_BIT, 0];
    with_chip_select(spi, cs, |spi| {
        spi.transfer_in_place(&mut frame)?;
        spi.flush()
    })?;
    Ok(frame[1])
}

pub fn read_who_am_i<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
) -> Result<u8, Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    read_reg(spi, cs, Register::WhoAmI)
}

fn verify_register<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
    register: Register,
    expected: u8,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let observed = read_reg(spi, cs, register)?;
    if observed == expected {
        Ok(())
    } else {
        Err(Error::RegisterVerification {
            register,
            expected,
            observed,
        })
    }
}

pub fn init<SPI, CS, D>(
    spi: &mut SPI,
    cs: &mut CS,
    delay: &mut D,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    D: DelayNs,
{
    init_with_config(spi, cs, delay, Config::default())
}

pub fn init_with_config<SPI, CS, D>(
    spi: &mut SPI,
    cs: &mut CS,
    delay: &mut D,
    config: Config,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    D: DelayNs,
{
    cs.set_high().map_err(Error::Pin)?;

    // Over SPI, DEVICE_RESET alone does not reset the sensor signal paths.
    write_reg(spi, cs, Register::PwrMgmt1, DEVICE_RESET)?;
    delay.delay_ms(RESET_SETTLE_MS);
    write_reg(
        spi,
        cs,
        Register::SignalPathReset,
        GYRO_ACCEL_TEMP_SIGNAL_PATH_RESET,
    )?;
    delay.delay_ms(RESET_SETTLE_MS);

    // Leaving sleep with the gyro PLL as the clock.
    write_reg(spi, cs, Register::PwrMgmt1, CLKSEL_PLL_GYRO_Z)?;
    delay.delay_ms(CLOCK_SETTLE_MS);

    // Reset re-enables the I2C slave interface; SPI-only from here.
    write_reg(spi, cs, Register::UserCtrl, I2C_IF_DIS)?;

    let who_am_i = read_who_am_i(spi, cs)?;
    if who_am_i != WHO_AM_I_EXPECTED {
        return Err(Error::InvalidWhoAmI(who_am_i));
    }

    write_reg(spi, cs, Register::PwrMgmt2, ALL_AXES_ENABLED)?;
    write_reg(spi, cs, Register::Config, config.config())?;
    write_reg(spi, cs, Register::SmplrtDiv, config.smplrt_div())?;
    write_reg(spi, cs, Register::GyroConfig, config.gyro_config())?;
    write_reg(spi, cs, Register::AccelConfig, config.accel_config())?;
    write_reg(
        spi,
        cs,
        Register::IntPinCfg,
        INT_ACTIVE_HIGH_PUSH_PULL_PULSED,
    )?;
    write_reg(spi, cs, Register::IntEnable, DATA_RDY_EN)?;

    verify_register(spi, cs, Register::PwrMgmt1, CLKSEL_PLL_GYRO_Z)?;
    verify_register(spi, cs, Register::PwrMgmt2, ALL_AXES_ENABLED)?;
    verify_register(spi, cs, Register::Config, config.config())?;
    verify_register(spi, cs, Register::SmplrtDiv, config.smplrt_div())?;
    verify_register(spi, cs, Register::GyroConfig, config.gyro_config())?;
    verify_register(spi, cs, Register::AccelConfig, config.accel_config())?;
    verify_register(
        spi,
        cs,
        Register::IntPinCfg,
        INT_ACTIVE_HIGH_PUSH_PULL_PULSED,
    )?;
    verify_register(spi, cs, Register::IntEnable, DATA_RDY_EN)?;

    delay.delay_ms(GYRO_STARTUP_MS);
    Ok(())
}

pub fn read_sample<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
) -> Result<ImuBurstSample, Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let mut frame = [0; SPI_BURST_SIZE];
    frame[0] = (Register::AccelXoutH as u8) | SPI_READ_BIT;

    with_chip_select(spi, cs, |spi| {
        spi.transfer_in_place(&mut frame)?;
        spi.flush()
    })?;

    decode_accel_temp_gyro_burst(&frame).map_err(Error::Decode)
}

pub fn decode_accel_temp_gyro_burst(frame: &[u8]) -> Result<ImuBurstSample, DecodeError> {
    if frame.len() < SPI_BURST_SIZE {
        return Err(DecodeError::FrameTooShort);
    }

    let payload = &frame[1..SPI_BURST_SIZE];
    if payload.iter().all(|byte| *byte == 0) || payload.iter().all(|byte| *byte == 0xff) {
        return Err(DecodeError::ImplausibleFrame);
    }

    Ok(ImuBurstSample {
        acc_raw: [
            i16::from_be_bytes([frame[1], frame[2]]),
            i16::from_be_bytes([frame[3], frame[4]]),
            i16::from_be_bytes([frame[5], frame[6]]),
        ],
        temp_raw: i16::from_be_bytes([frame[7], frame[8]]),
        gyro_raw: [
            i16::from_be_bytes([frame[9], frame[10]]),
            i16::from_be_bytes([frame[11], frame[12]]),
            i16::from_be_bytes([frame[13], frame[14]]),
        ],
    })
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::{digital::ErrorType as DigitalErrorType, spi::ErrorType as SpiErrorType};
    use std::vec::Vec;

    #[derive(Debug, Default)]
    struct MockPin {
        states: Vec<bool>,
    }

    impl DigitalErrorType for MockPin {
        type Error = Infallible;
    }

    impl OutputPin for MockPin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.states.push(false);
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.states.push(true);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct MockDelay {
        delay_ns_total: u64,
    }

    impl DelayNs for MockDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.delay_ns_total += ns as u64;
        }
    }

    /// A register file that reads back what was written, except where a test
    /// pins a value. `WHO_AM_I` defaults to the MPU-6000's.
    struct MockMpu {
        registers: [u8; 128],
        pinned: Vec<(u8, u8)>,
        writes: Vec<[u8; 2]>,
    }

    impl Default for MockMpu {
        fn default() -> Self {
            let mut registers = [0; 128];
            registers[Register::WhoAmI as usize] = WHO_AM_I_EXPECTED;
            Self {
                registers,
                pinned: Vec::new(),
                writes: Vec::new(),
            }
        }
    }

    impl SpiErrorType for MockMpu {
        type Error = Infallible;
    }

    impl SpiBus<u8> for MockMpu {
        fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            words.fill(0);
            Ok(())
        }

        fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
            let address = words[0] & !SPI_READ_BIT;
            self.writes.push([address, words[1]]);
            self.registers[address as usize] = words[1];
            Ok(())
        }

        fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
            read.fill(0);
            let len = read.len().min(write.len());
            read[..len].copy_from_slice(&write[..len]);
            Ok(())
        }

        fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            let address = words[0] & !SPI_READ_BIT;
            for (offset, word) in words[1..].iter_mut().enumerate() {
                let register = address + offset as u8;
                *word = self
                    .pinned
                    .iter()
                    .find(|(pinned, _)| *pinned == register)
                    .map_or(self.registers[register as usize], |(_, value)| *value);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn default_config_is_2khz_with_the_dlpf_bypassed() {
        let config = Config::default();
        assert_eq!(config.output_data_rate.hz(), 2_000);
        assert_eq!(config.config(), 0);
        assert_eq!(config.smplrt_div(), 3);
        assert_eq!(config.gyro_config(), 0x18);
        assert_eq!(config.accel_config(), 0x18);
    }

    #[test]
    fn every_output_rate_divides_its_internal_rate_exactly() {
        assert_eq!(OutputDataRate::Hz1000.smplrt_div(), 0);
        assert_eq!(OutputDataRate::Hz1000.dlpf_cfg(), 1);
        assert_eq!(OutputDataRate::Hz2000.smplrt_div(), 3);
        assert_eq!(OutputDataRate::Hz4000.smplrt_div(), 1);
    }

    #[test]
    fn init_resets_over_spi_then_configures_and_verifies() {
        let mut spi = MockMpu::default();
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();

        init(&mut spi, &mut cs, &mut delay).unwrap();

        assert_eq!(
            &spi.writes[..4],
            &[
                [Register::PwrMgmt1 as u8, DEVICE_RESET],
                [Register::SignalPathReset as u8, 0x07],
                [Register::PwrMgmt1 as u8, CLKSEL_PLL_GYRO_Z],
                [Register::UserCtrl as u8, I2C_IF_DIS],
            ]
        );
        assert!(
            spi.writes
                .contains(&[Register::IntPinCfg as u8, INT_ACTIVE_HIGH_PUSH_PULL_PULSED])
        );
        assert!(
            spi.writes
                .contains(&[Register::IntEnable as u8, DATA_RDY_EN])
        );
        assert!(delay.delay_ns_total >= 300_000_000);
        assert_eq!(cs.states.last(), Some(&true));
    }

    #[test]
    fn init_rejects_another_invensense_part() {
        let mut spi = MockMpu::default();
        spi.pinned.push((Register::WhoAmI as u8, 0x70));
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();

        assert_eq!(
            init(&mut spi, &mut cs, &mut delay),
            Err(Error::InvalidWhoAmI(0x70))
        );
    }

    #[test]
    fn init_reports_a_register_that_did_not_take() {
        let mut spi = MockMpu::default();
        spi.pinned.push((Register::GyroConfig as u8, 0x00));
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();

        assert_eq!(
            init(&mut spi, &mut cs, &mut delay),
            Err(Error::RegisterVerification {
                register: Register::GyroConfig,
                expected: 0x18,
                observed: 0x00,
            })
        );
    }

    #[test]
    fn read_reg_sets_read_bit_and_toggles_chip_select() {
        let mut spi = MockMpu::default();
        let mut cs = MockPin::default();

        assert_eq!(read_who_am_i(&mut spi, &mut cs).unwrap(), 0x68);
        assert_eq!(cs.states, [false, true]);
    }

    #[test]
    fn burst_decodes_accel_temp_gyro_big_endian() {
        let frame = [
            0x00, 0x08, 0x00, 0xf8, 0x00, 0x00, 0x10, 0xf3, 0x20, 0x7f, 0xff, 0x80, 0x00, 0xff,
            0xfe,
        ];

        let sample = decode_accel_temp_gyro_burst(&frame).unwrap();

        assert_eq!(sample.acc_raw, [2048, -2048, 16]);
        assert_eq!(sample.temp_raw, -3296);
        assert_eq!(sample.gyro_raw, [32767, -32768, -2]);
        assert_eq!(sample.accel_g(AccelFullScale::G16)[..2], [1.0, -1.0]);
        // -3296 / 340 + 36.53 = 26.836 C
        assert!((sample.temperature_c() - 26.836).abs() < 0.01);
        assert!((sample.gyro_dps(GyroFullScale::Dps2000)[2] + 0.122).abs() < 0.001);
    }

    #[test]
    fn burst_rejects_short_or_implausible_frames() {
        assert_eq!(
            decode_accel_temp_gyro_burst(&[0x01; SPI_BURST_SIZE - 1]),
            Err(DecodeError::FrameTooShort)
        );
        assert_eq!(
            decode_accel_temp_gyro_burst(&[0; SPI_BURST_SIZE]),
            Err(DecodeError::ImplausibleFrame)
        );
        assert_eq!(
            decode_accel_temp_gyro_burst(&[0xff; SPI_BURST_SIZE]),
            Err(DecodeError::ImplausibleFrame)
        );
    }

    #[test]
    fn read_sample_bursts_from_accel_xout_h() {
        let mut spi = MockMpu::default();
        spi.registers[Register::AccelXoutH as usize + 4] = 0x08;
        let mut cs = MockPin::default();

        let sample = read_sample(&mut spi, &mut cs).unwrap();

        assert_eq!(sample.acc_raw, [0, 0, 2048]);
        assert_eq!(cs.states, [false, true]);
    }
}
