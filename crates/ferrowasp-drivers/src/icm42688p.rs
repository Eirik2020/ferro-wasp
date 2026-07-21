//! Allocation-free SPI driver for the TDK InvenSense ICM-42688-P.
//!
//! The default configuration matches FerroWasp's current control path:
//! 1 kHz accelerometer and gyroscope output, +/-16 g, +/-2000 dps, and
//! big-endian sensor data. The 14-byte temperature/accelerometer/gyroscope
//! payload fits the existing 15-byte full-duplex SPI DMA frame.

use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiBus};

pub const WHO_AM_I_EXPECTED: u8 = 0x47;
pub const SPI_BURST_SIZE: usize = 15;
pub const SPI_READ_BIT: u8 = 0x80;

pub const OUTPUT_DATA_RATE_HZ: u32 = 1_000;
pub const TEMPERATURE_LSB_PER_C: f32 = 132.48;
pub const TEMPERATURE_OFFSET_C: f32 = 25.0;

const RESET_DONE: u8 = 1 << 4;
const SOFT_RESET: u8 = 1;
const DISABLE_I2C_BIG_ENDIAN: u8 = 0x33;
const ODR_1_KHZ: u8 = 0x06;
const UI_FILTER_ODR_DIV_4: u8 = 0x11;
const ACCEL_GYRO_LOW_NOISE: u8 = 0x0f;
const RESET_SETTLE_MS: u32 = 2;
const POWER_MODE_SETTLE_US: u32 = 1_000;
const GYRO_STARTUP_REMAINDER_MS: u32 = 49;
const INT1_ACTIVE_HIGH_PUSH_PULL_PULSED: u8 = (1 << 1) | (1 << 0);
const INT_CONFIG1_PROPER_PIN_OPERATION: u8 = 0;
const UI_DATA_READY_INT1_ENABLE: u8 = 1 << 3;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Register {
    DeviceConfig = 0x11,
    IntConfig = 0x14,
    TempData1 = 0x1d,
    IntStatus = 0x2d,
    IntfConfig0 = 0x4c,
    PwrMgmt0 = 0x4e,
    GyroConfig0 = 0x4f,
    AccelConfig0 = 0x50,
    GyroAccelConfig0 = 0x52,
    IntConfig1 = 0x64,
    IntSource0 = 0x65,
    WhoAmI = 0x75,
    RegBankSel = 0x76,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GyroFullScale {
    Dps2000 = 0,
    Dps1000 = 1,
    Dps500 = 2,
    Dps250 = 3,
    Dps125 = 4,
    Dps62_5 = 5,
    Dps31_25 = 6,
    Dps15_625 = 7,
}

impl GyroFullScale {
    pub const fn lsb_per_dps(self) -> f32 {
        match self {
            Self::Dps2000 => 16.4,
            Self::Dps1000 => 32.8,
            Self::Dps500 => 65.5,
            Self::Dps250 => 131.0,
            Self::Dps125 => 262.0,
            Self::Dps62_5 => 524.3,
            Self::Dps31_25 => 1_048.6,
            Self::Dps15_625 => 2_097.2,
        }
    }

    const fn register_bits(self) -> u8 {
        (self as u8) << 5
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccelFullScale {
    G16 = 0,
    G8 = 1,
    G4 = 2,
    G2 = 3,
}

impl AccelFullScale {
    pub const fn lsb_per_g(self) -> f32 {
        match self {
            Self::G16 => 2_048.0,
            Self::G8 => 4_096.0,
            Self::G4 => 8_192.0,
            Self::G2 => 16_384.0,
        }
    }

    const fn register_bits(self) -> u8 {
        (self as u8) << 5
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub gyro_full_scale: GyroFullScale,
    pub accel_full_scale: AccelFullScale,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gyro_full_scale: GyroFullScale::Dps2000,
            accel_full_scale: AccelFullScale::G16,
        }
    }
}

impl Config {
    const fn gyro_config0(self) -> u8 {
        self.gyro_full_scale.register_bits() | ODR_1_KHZ
    }

    const fn accel_config0(self) -> u8 {
        self.accel_full_scale.register_bits() | ODR_1_KHZ
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
    ResetNotComplete(u8),
    RegisterVerification {
        register: Register,
        expected: u8,
        observed: u8,
    },
    Decode(DecodeError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuBurstSample {
    pub temp_raw: i16,
    pub acc_raw: [i16; 3],
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

    write_reg(spi, cs, Register::DeviceConfig, SOFT_RESET)?;
    delay.delay_ms(RESET_SETTLE_MS);

    let reset_status = read_reg(spi, cs, Register::IntStatus)?;
    if reset_status & RESET_DONE == 0 {
        return Err(Error::ResetNotComplete(reset_status));
    }

    let who_am_i = read_who_am_i(spi, cs)?;
    if who_am_i != WHO_AM_I_EXPECTED {
        return Err(Error::InvalidWhoAmI(who_am_i));
    }

    write_reg(spi, cs, Register::RegBankSel, 0)?;
    write_reg(spi, cs, Register::IntfConfig0, DISABLE_I2C_BIG_ENDIAN)?;
    write_reg(spi, cs, Register::GyroConfig0, config.gyro_config0())?;
    write_reg(spi, cs, Register::AccelConfig0, config.accel_config0())?;
    write_reg(spi, cs, Register::GyroAccelConfig0, UI_FILTER_ODR_DIV_4)?;

    verify_register(spi, cs, Register::RegBankSel, 0)?;
    verify_register(spi, cs, Register::IntfConfig0, DISABLE_I2C_BIG_ENDIAN)?;
    verify_register(spi, cs, Register::GyroConfig0, config.gyro_config0())?;
    verify_register(spi, cs, Register::AccelConfig0, config.accel_config0())?;
    verify_register(spi, cs, Register::GyroAccelConfig0, UI_FILTER_ODR_DIV_4)?;

    // PWR_MGMT0 is deliberately the final configuration write. The device
    // forbids register writes for at least 200 us after sensors leave OFF.
    write_reg(spi, cs, Register::PwrMgmt0, ACCEL_GYRO_LOW_NOISE)?;
    delay.delay_us(POWER_MODE_SETTLE_US);
    verify_register(spi, cs, Register::PwrMgmt0, ACCEL_GYRO_LOW_NOISE)?;

    // Emit the 1 kHz UI data-ready event on INT1 as an active-high, push-pull,
    // 100 us pulse. INT_ASYNC_RESET must be cleared from its reset value for
    // proper INT1 operation.
    write_reg(
        spi,
        cs,
        Register::IntConfig,
        INT1_ACTIVE_HIGH_PUSH_PULL_PULSED,
    )?;
    write_reg(
        spi,
        cs,
        Register::IntConfig1,
        INT_CONFIG1_PROPER_PIN_OPERATION,
    )?;
    write_reg(spi, cs, Register::IntSource0, UI_DATA_READY_INT1_ENABLE)?;
    verify_register(
        spi,
        cs,
        Register::IntConfig,
        INT1_ACTIVE_HIGH_PUSH_PULL_PULSED,
    )?;
    verify_register(
        spi,
        cs,
        Register::IntConfig1,
        INT_CONFIG1_PROPER_PIN_OPERATION,
    )?;
    verify_register(spi, cs, Register::IntSource0, UI_DATA_READY_INT1_ENABLE)?;

    // Gyroscope output is not considered usable until its 45 ms startup time
    // has elapsed. A conservative 50 ms total power-mode wait is used.
    delay.delay_ms(GYRO_STARTUP_REMAINDER_MS);
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
    frame[0] = (Register::TempData1 as u8) | SPI_READ_BIT;

    with_chip_select(spi, cs, |spi| {
        spi.transfer_in_place(&mut frame)?;
        spi.flush()
    })?;

    decode_temp_accel_gyro_burst(&frame).map_err(Error::Decode)
}

pub fn decode_temp_accel_gyro_burst(frame: &[u8]) -> Result<ImuBurstSample, DecodeError> {
    if frame.len() < SPI_BURST_SIZE {
        return Err(DecodeError::FrameTooShort);
    }

    let payload = &frame[1..SPI_BURST_SIZE];
    if payload.iter().all(|byte| *byte == 0) || payload.iter().all(|byte| *byte == 0xff) {
        return Err(DecodeError::ImplausibleFrame);
    }

    Ok(ImuBurstSample {
        temp_raw: i16::from_be_bytes([frame[1], frame[2]]),
        acc_raw: [
            i16::from_be_bytes([frame[3], frame[4]]),
            i16::from_be_bytes([frame[5], frame[6]]),
            i16::from_be_bytes([frame[7], frame[8]]),
        ],
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
    use embedded_hal::{
        digital::ErrorType as DigitalErrorType,
        spi::{ErrorKind, ErrorType as SpiErrorType},
    };
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
        elapsed_ns: u64,
    }

    impl DelayNs for MockDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.elapsed_ns += ns as u64;
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum MockSpiError {
        Injected,
    }

    impl embedded_hal::spi::Error for MockSpiError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    struct MockSpi {
        registers: [u8; 0x77],
        writes: Vec<[u8; 2]>,
        burst_payload: [u8; SPI_BURST_SIZE - 1],
        fail_next_transfer: bool,
        suppress_reset_done: bool,
        stuck_register: Option<u8>,
    }

    impl MockSpi {
        fn new(who_am_i: u8) -> Self {
            let mut registers = [0; 0x77];
            registers[Register::WhoAmI as usize] = who_am_i;
            registers[Register::IntfConfig0 as usize] = 0x30;
            registers[Register::GyroAccelConfig0 as usize] = 0x11;
            registers[Register::IntConfig1 as usize] = 0x10;
            registers[Register::IntSource0 as usize] = 0x10;
            Self {
                registers,
                writes: Vec::new(),
                burst_payload: [0; SPI_BURST_SIZE - 1],
                fail_next_transfer: false,
                suppress_reset_done: false,
                stuck_register: None,
            }
        }

        fn reset_registers(&mut self) {
            let who_am_i = self.registers[Register::WhoAmI as usize];
            self.registers.fill(0);
            self.registers[Register::WhoAmI as usize] = who_am_i;
            self.registers[Register::IntfConfig0 as usize] = 0x30;
            self.registers[Register::GyroAccelConfig0 as usize] = 0x11;
            self.registers[Register::IntConfig1 as usize] = 0x10;
            self.registers[Register::IntSource0 as usize] = 0x10;
            if !self.suppress_reset_done {
                self.registers[Register::IntStatus as usize] = RESET_DONE;
            }
        }
    }

    impl SpiErrorType for MockSpi {
        type Error = MockSpiError;
    }

    impl SpiBus<u8> for MockSpi {
        fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            words.fill(0);
            Ok(())
        }

        fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
            if let [register, value] = words {
                self.writes.push([*register, *value]);
                if *register == Register::DeviceConfig as u8 && value & SOFT_RESET != 0 {
                    self.reset_registers();
                } else if self.stuck_register != Some(*register)
                    && (*register as usize) < self.registers.len()
                {
                    self.registers[*register as usize] = *value;
                }
            }
            Ok(())
        }

        fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
            read.fill(0);
            let len = read.len().min(write.len());
            read[..len].copy_from_slice(&write[..len]);
            Ok(())
        }

        fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            if self.fail_next_transfer {
                self.fail_next_transfer = false;
                return Err(MockSpiError::Injected);
            }

            let register = words[0] & !SPI_READ_BIT;
            if register == Register::TempData1 as u8 && words.len() == SPI_BURST_SIZE {
                words[1..].copy_from_slice(&self.burst_payload);
            } else if words.len() >= 2 {
                words[1] = self.registers[register as usize];
                if register == Register::IntStatus as u8 {
                    self.registers[register as usize] = 0;
                }
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn default_configuration_matches_ferrowasp_control_contract() {
        let config = Config::default();

        assert_eq!(config.gyro_full_scale, GyroFullScale::Dps2000);
        assert_eq!(config.accel_full_scale, AccelFullScale::G16);
        assert_eq!(config.gyro_config0(), 0x06);
        assert_eq!(config.accel_config0(), 0x06);
        assert_eq!(OUTPUT_DATA_RATE_HZ, 1_000);
    }

    #[test]
    fn init_resets_verifies_configures_and_waits_for_gyro_startup() {
        let mut spi = MockSpi::new(WHO_AM_I_EXPECTED);
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();

        init(&mut spi, &mut cs, &mut delay).unwrap();

        assert_eq!(
            spi.writes,
            [
                [Register::DeviceConfig as u8, SOFT_RESET],
                [Register::RegBankSel as u8, 0],
                [Register::IntfConfig0 as u8, DISABLE_I2C_BIG_ENDIAN],
                [Register::GyroConfig0 as u8, 0x06],
                [Register::AccelConfig0 as u8, 0x06],
                [Register::GyroAccelConfig0 as u8, UI_FILTER_ODR_DIV_4],
                [Register::PwrMgmt0 as u8, ACCEL_GYRO_LOW_NOISE],
                [Register::IntConfig as u8, INT1_ACTIVE_HIGH_PUSH_PULL_PULSED],
                [Register::IntConfig1 as u8, INT_CONFIG1_PROPER_PIN_OPERATION],
                [Register::IntSource0 as u8, UI_DATA_READY_INT1_ENABLE],
            ]
        );
        assert!(delay.elapsed_ns >= 52_000_000);
        assert_eq!(cs.states.last(), Some(&true));
    }

    #[test]
    fn init_rejects_wrong_identity_and_missing_reset_completion() {
        let mut wrong_spi = MockSpi::new(0x70);
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();
        assert_eq!(
            init(&mut wrong_spi, &mut cs, &mut delay),
            Err(Error::InvalidWhoAmI(0x70))
        );

        let mut reset_spi = MockSpi::new(WHO_AM_I_EXPECTED);
        reset_spi.suppress_reset_done = true;
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();
        assert_eq!(
            init(&mut reset_spi, &mut cs, &mut delay),
            Err(Error::ResetNotComplete(0))
        );
    }

    #[test]
    fn init_rejects_configuration_that_does_not_read_back() {
        let mut spi = MockSpi::new(WHO_AM_I_EXPECTED);
        spi.stuck_register = Some(Register::IntfConfig0 as u8);
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();

        assert_eq!(
            init(&mut spi, &mut cs, &mut delay),
            Err(Error::RegisterVerification {
                register: Register::IntfConfig0,
                expected: DISABLE_I2C_BIG_ENDIAN,
                observed: 0x30,
            })
        );
        assert!(
            !spi.writes
                .iter()
                .any(|write| write[0] == Register::PwrMgmt0 as u8)
        );
    }

    #[test]
    fn init_rejects_data_ready_interrupt_configuration_that_does_not_read_back() {
        let mut spi = MockSpi::new(WHO_AM_I_EXPECTED);
        spi.stuck_register = Some(Register::IntConfig1 as u8);
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();

        assert_eq!(
            init(&mut spi, &mut cs, &mut delay),
            Err(Error::RegisterVerification {
                register: Register::IntConfig1,
                expected: INT_CONFIG1_PROPER_PIN_OPERATION,
                observed: 0x10,
            })
        );
    }

    #[test]
    fn decode_uses_icm_temperature_accel_gyro_register_order() {
        let frame = [
            0x00, 0x01, 0x32, 0x08, 0x00, 0xf0, 0x00, 0x20, 0x00, 0x01, 0x06, 0xff, 0xfe, 0x7f,
            0xff,
        ];

        let sample = decode_temp_accel_gyro_burst(&frame).unwrap();

        assert_eq!(sample.temp_raw, 0x0132);
        assert_eq!(sample.acc_raw, [2_048, -4_096, 8_192]);
        assert_eq!(sample.gyro_raw, [262, -2, 32_767]);
    }

    #[test]
    fn sample_conversions_match_selected_full_scales() {
        let sample = ImuBurstSample {
            temp_raw: 1_324,
            acc_raw: [2_048, -4_096, 0],
            gyro_raw: [164, -328, 0],
        };

        assert!((sample.temperature_c() - 34.994).abs() < 0.01);
        assert_eq!(sample.accel_g(AccelFullScale::G16), [1.0, -2.0, 0.0]);
        assert_eq!(sample.gyro_dps(GyroFullScale::Dps2000), [10.0, -20.0, 0.0]);
    }

    #[test]
    fn decode_rejects_short_and_stuck_bus_frames() {
        assert_eq!(
            decode_temp_accel_gyro_burst(&[0; SPI_BURST_SIZE - 1]),
            Err(DecodeError::FrameTooShort)
        );
        assert_eq!(
            decode_temp_accel_gyro_burst(&[0; SPI_BURST_SIZE]),
            Err(DecodeError::ImplausibleFrame)
        );
        assert_eq!(
            decode_temp_accel_gyro_burst(&[0xff; SPI_BURST_SIZE]),
            Err(DecodeError::ImplausibleFrame)
        );
    }

    #[test]
    fn read_sample_uses_temp_data_start_register() {
        let mut spi = MockSpi::new(WHO_AM_I_EXPECTED);
        spi.burst_payload = [
            0x00, 0x00, 0x08, 0x00, 0xf8, 0x00, 0x10, 0x00, 0x00, 0xa4, 0xff, 0x5c, 0x00, 0x01,
        ];
        let mut cs = MockPin::default();

        let sample = read_sample(&mut spi, &mut cs).unwrap();

        assert_eq!(sample.acc_raw, [2_048, -2_048, 4_096]);
        assert_eq!(sample.gyro_raw, [164, -164, 1]);
        assert_eq!(cs.states, [false, true]);
    }

    #[test]
    fn spi_failure_still_deasserts_chip_select() {
        let mut spi = MockSpi::new(WHO_AM_I_EXPECTED);
        spi.fail_next_transfer = true;
        let mut cs = MockPin::default();

        assert_eq!(
            read_who_am_i(&mut spi, &mut cs),
            Err(Error::Spi(MockSpiError::Injected))
        );
        assert_eq!(cs.states, [false, true]);
    }
}
