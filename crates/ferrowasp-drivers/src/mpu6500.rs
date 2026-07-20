#![allow(dead_code)]

#[cfg(all(target_arch = "arm", feature = "defmt"))]
use defmt::info;
use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiBus};

pub const WHO_AM_I_EXPECTED: u8 = 0x70;

// Constants
pub const SPI_ARRAY_SIZE: usize = 15;

#[derive(Default)]
pub struct ImuData {
    pub acc: [f32; 3],
    pub gyro: [f32; 3],
    pub gyro_raw: [i16; 3],
    pub temp: f32,
    pub sequence: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuBurstSample {
    pub acc_raw: [i16; 3],
    pub temp_raw: i16,
    pub gyro_raw: [i16; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecodeError {
    FrameTooShort,
    ImplausibleFrame,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SampleFreshness {
    Fresh,
    Stale,
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Register {
    // Configuration
    GYRO_CONFIG = 0x1B,  // Gyroscope Configuration
    AccelConfig = 0x1C,  // Accelerometer Configuration
    AccelConfig2 = 0x1D, // Accelerometer Configuration 2
    FifoEn = 0x23,       // FIFO Enable
    INT_PIN_CFG = 0x37,  // Interrupt Pin/Bypass Enable Config
    INT_ENABLE = 0x38,   // Interrupt Enable
    INT_STATUS = 0x3A,   // Interrupt Status

    // Sensor Data (Burst Read Starting Points)
    AccelXoutH = 0x3B, // Start of Accel/Temp/Gyro burst
    TempOutH = 0x41,   // Temperature Measurement High
    GyroXoutH = 0x43,  // Start of Gyro-only burst

    // User & Power Management
    SIGNAL_PATH_RESET = 0x68, // Signal Path Reset
    UserCtrl = 0x6A,          // User Control (FIFO, I2C Master, etc.)
    PWR_MGMT_1 = 0x6B,        // Power Management 1
    PwrMgmt2 = 0x6C,          // Power Management 2
    WHO_AM_I = 0x75,          // Who Am I (Device ID)

    // Sensor configuration
    /// Register to set gyroscope sampling rate, sample rate = Gyroscope Output Rate / (1 + SMPLRT_DIV).
    SMPRT_DIV = 0x19,
    /// This register configures the external Frame Synchronization (FSYNC) pin sampling and
    /// the Digital Low Pass Filter (DLPF) setting for both the gyroscopes and accelerometers.
    CONFIG = 0x1A, // DLPF and Sync configuration

    NONE = 0x00,
}

#[repr(u8)]
pub enum GyroSensitivity {
    Dps250 = 0b00 << 3,
    Dps500 = 0b01 << 3,
    Dps1000 = 0b10 << 3,
    Dps2000 = 0b11 << 3,
}

#[repr(u8)]
pub enum AccelSensitivity {
    G2 = 0b00 << 3,
    G4 = 0b01 << 3,
    G8 = 0b10 << 3,
    G16 = 0b11 << 3,
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum GyroDlpf {
    Bandwidth250Hz = 0,
    Bandwidth184Hz = 1,
    Bandwidth92Hz = 2,
    Bandwidth41Hz = 3,
    Bandwidth20Hz = 4,
    Bandwidth10Hz = 5,
    Bandwidth5Hz = 6,
}

impl GyroDlpf {
    fn output_rate_hz(self) -> u32 {
        match self {
            GyroDlpf::Bandwidth250Hz => 8_000,
            _ => 1_000,
        }
    }
}

// The control task polls the latest gyro value at 800 Hz. With DLPF enabled,
// the MPU6500 gyro output rate is 1 kHz, so keep the internal sample divider
// at 1 kHz and let the timer decide when to read it.
pub const GYRO_SAMPLE_RATE_HZ: u32 = 1_000;
pub const GYRO_DLPF: GyroDlpf = GyroDlpf::Bandwidth184Hz;

pub mod bits {
    pub const CLKSEL_AUTO: u8 = 0x01;
    pub const I2C_IF_DIS: u8 = 0x10;

    pub const INT_RAW_RDY_EN: u8 = 0x01;
    pub const INT_LATCH_EN: u8 = 0x20;
    pub const INT_ANYRD_2CLEAR: u8 = 0x10;

    // PWR_MGMT_1 Bits
    pub const DEVICE_RESET: u8 = 1 << 7; // Bit 7
}

#[derive(Debug)]
pub enum Error<SpiE, PinE> {
    Spi(SpiE),
    Pin(PinE),
    InvalidWhoAmI(u8),
}

#[inline]
pub fn set_bits(reg: u8, mask: u8) -> u8 {
    reg | mask
}

#[inline]
pub fn clear_bits(reg: u8, mask: u8) -> u8 {
    reg & !mask
}

#[inline]
pub fn toggle_bits(reg: u8, mask: u8) -> u8 {
    reg ^ mask
}

#[inline]
pub fn is_bits_set(reg: u8, mask: u8) -> bool {
    (reg & mask) == mask
}

#[inline]
pub fn is_any_bit_set(reg: u8, mask: u8) -> bool {
    (reg & mask) != 0
}

fn write_reg<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
    reg: Register,
    value: u8,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    cs.set_low().map_err(Error::Pin)?;
    spi.write(&[(reg as u8) & 0x7F, value])
        .map_err(Error::Spi)?;
    spi.flush().map_err(Error::Spi)?;
    cs.set_high().map_err(Error::Pin)?;
    Ok(())
}

pub fn read_reg<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
    reg: Register,
) -> Result<u8, Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let mut buf = [(reg as u8) | 0x80, 0x00];

    cs.set_low().map_err(Error::Pin)?;
    spi.transfer_in_place(&mut buf).map_err(Error::Spi)?;
    spi.flush().map_err(Error::Spi)?;
    cs.set_high().map_err(Error::Pin)?;

    Ok(buf[1])
}

pub fn read_gyro_raw<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
) -> Result<[i16; 3], Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let mut buf = [0u8; 7];
    buf[0] = (Register::GyroXoutH as u8) | 0x80;

    cs.set_low().map_err(Error::Pin)?;
    spi.transfer_in_place(&mut buf).map_err(Error::Spi)?;
    spi.flush().map_err(Error::Spi)?;
    cs.set_high().map_err(Error::Pin)?;

    let gx = i16::from_be_bytes([buf[1], buf[2]]);
    let gy = i16::from_be_bytes([buf[3], buf[4]]);
    let gz = i16::from_be_bytes([buf[5], buf[6]]);

    Ok([gx, gy, gz])
}

pub fn clear_interrupt<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let _ = read_reg(spi, cs, Register::INT_STATUS)?;
    Ok(())
}

pub fn enable_interrupt<SPI, CS, D>(
    spi: &mut SPI,
    cs: &mut CS,
    delay: &mut D,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    D: DelayNs,
{
    let mut register = read_reg(spi, cs, Register::INT_PIN_CFG)?;
    register &= !(1 << 7); // active high
    register &= !(1 << 6); // push-pull
    register &= !(1 << 5); // pulse mode, not latched
    register |= 1 << 4; // any read clears interrupt

    write_reg(spi, cs, Register::INT_PIN_CFG, register)?;
    delay.delay_ms(10);

    let mut register = read_reg(spi, cs, Register::INT_ENABLE)?;
    register &= !(1 << 6); // WOM disabled
    register &= !(1 << 4); // FIFO overflow disabled
    register &= !(1 << 3); // FSYNC interrupt disabled
    register |= 1 << 0; // raw data ready interrupt enable

    write_reg(spi, cs, Register::INT_ENABLE, register)?;
    delay.delay_ms(10);

    Ok(())
}

pub fn device_reset<SPI, CS>(spi: &mut SPI, cs: &mut CS) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let mut register = read_reg(spi, cs, Register::PWR_MGMT_1).unwrap();
    register |= 1 << 7;
    write_reg(spi, cs, Register::PWR_MGMT_1, register)
}

pub fn signal_path_reset<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let mut register = read_reg(spi, cs, Register::SIGNAL_PATH_RESET).unwrap();
    register |= 1 << 0;
    register |= 1 << 1;
    register |= 1 << 2;
    write_reg(spi, cs, Register::SIGNAL_PATH_RESET, register)
}

pub fn power_managment_default<SPI, CS>(
    spi: &mut SPI,
    cs: &mut CS,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    let mut register = read_reg(spi, cs, Register::PWR_MGMT_1)?;

    register &= !(1 << 6); // clear sleep
    register &= !(1 << 5); // clear cycle
    register &= !0b0000_0111; // clear CLKSEL
    register |= bits::CLKSEL_AUTO;

    write_reg(spi, cs, Register::PWR_MGMT_1, register)
}

pub fn gyro_set_sample_rate<SPI, CS, D>(
    spi: &mut SPI,
    cs: &mut CS,
    sample_rate: u32,
    dlpf: GyroDlpf,
    delay: &mut D,
) -> Result<(), Error<SPI::Error, CS::Error>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    D: DelayNs,
{
    let mut register = read_reg(spi, cs, Register::CONFIG)?;
    register &= !0b0000_0111;
    register |= dlpf as u8;
    write_reg(spi, cs, Register::CONFIG, register)?;
    delay.delay_ms(10);

    let mut register = read_reg(spi, cs, Register::GYRO_CONFIG)?;
    register &= !0b0000_0011; // FCHOICE_B = 0, use DLPF_CFG.
    write_reg(spi, cs, Register::GYRO_CONFIG, register)?;
    delay.delay_ms(10);

    let gyroscope_output_rate = dlpf.output_rate_hz();
    let sample_rate = sample_rate.clamp(1, gyroscope_output_rate);
    let smplrt_div = ((gyroscope_output_rate / sample_rate) - 1) as u8;

    write_reg(spi, cs, Register::SMPRT_DIV, smplrt_div)?;
    delay.delay_ms(10);

    Ok(())
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
    cs.set_high().map_err(Error::Pin)?;

    device_reset(spi, cs)?;
    delay.delay_ms(100);

    signal_path_reset(spi, cs)?;
    delay.delay_ms(100);

    power_managment_default(spi, cs)?;
    delay.delay_ms(10);

    // Disable I2C interface; use SPI only
    write_reg(spi, cs, Register::UserCtrl, bits::I2C_IF_DIS)?;
    delay.delay_ms(10);

    let who_am_i = read_reg(spi, cs, Register::WHO_AM_I)?;
    if who_am_i != WHO_AM_I_EXPECTED {
        return Err(Error::InvalidWhoAmI(who_am_i));
    }

    #[cfg(all(target_arch = "arm", feature = "defmt"))]
    info!("Correct WHO_AM_I for MPU6500");

    gyro_set_sample_rate(spi, cs, GYRO_SAMPLE_RATE_HZ, GYRO_DLPF, delay)?;

    let mut gyro_config = read_reg(spi, cs, Register::GYRO_CONFIG)?;
    gyro_config &= !0b0001_1000;
    gyro_config |= GyroSensitivity::Dps2000 as u8;
    write_reg(spi, cs, Register::GYRO_CONFIG, gyro_config)?;
    delay.delay_ms(10);

    write_reg(spi, cs, Register::AccelConfig, AccelSensitivity::G8 as u8)?;
    delay.delay_ms(10);

    enable_interrupt(spi, cs, delay)?;

    Ok(())
}

pub enum ClockSelect {
    Internal20MHz = 0,
    AutoSelect = 1,
    StopClock = 7,
}

pub fn decode_accel_temp_gyro_burst(frame: &[u8]) -> Result<ImuBurstSample, DecodeError> {
    if frame.len() < SPI_ARRAY_SIZE {
        return Err(DecodeError::FrameTooShort);
    }

    let sample = ImuBurstSample {
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
    };

    if frame[1..SPI_ARRAY_SIZE].iter().all(|byte| *byte == 0)
        || frame[1..SPI_ARRAY_SIZE].iter().all(|byte| *byte == 0xff)
        || sample.acc_raw == [0, 0, 0]
    {
        return Err(DecodeError::ImplausibleFrame);
    }

    Ok(sample)
}

pub fn decode_gyro_burst(frame: &[u8]) -> Result<[i16; 3], DecodeError> {
    if frame.len() < 7 {
        return Err(DecodeError::FrameTooShort);
    }

    if frame[1..7].iter().all(|byte| *byte == 0xff) {
        return Err(DecodeError::ImplausibleFrame);
    }

    Ok([
        i16::from_be_bytes([frame[1], frame[2]]),
        i16::from_be_bytes([frame[3], frame[4]]),
        i16::from_be_bytes([frame[5], frame[6]]),
    ])
}

pub fn classify_sample_freshness(last_sequence: u32, current_sequence: u32) -> SampleFreshness {
    if current_sequence == last_sequence {
        SampleFreshness::Stale
    } else {
        SampleFreshness::Fresh
    }
}

struct PowerManagment {
    /// When true, the chip is set to sleep mode.
    device_asleep: bool,
    /// When true, and SLEEP and STANDBY are not set, the chip will cycle between sleep and taking a single sample at a rate determined by
    /// LP_ACCEL_ODR (MPU-6500 mode) or LP_WAKE_CTRL (MPU-6050 compatible mode)
    /// NOTE: When all accelerometer axis are disabled via PWR_MGMT_2
    /// register bits and cycle is enabled, the chip will wake up at the rate
    /// determined by the respective registers above, but will not take any samples
    cyclic_sleep: bool,
    gyro_standby: bool,
    temperature_sensor_disabled: bool,
    clock_select: ClockSelect,
}

pub struct Mpu6500 {
    min_us: u32,
    max_us: u32,
}

impl Mpu6500 {}

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

    #[derive(Debug, Default)]
    struct MockSpi {
        writes: Vec<Vec<u8>>,
        transfers: Vec<Vec<u8>>,
    }

    impl SpiErrorType for MockSpi {
        type Error = Infallible;
    }

    impl SpiBus<u8> for MockSpi {
        fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            words.fill(0);
            Ok(())
        }

        fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
            self.writes.push(words.to_vec());
            Ok(())
        }

        fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
            read.fill(0);
            let len = read.len().min(write.len());
            read[..len].copy_from_slice(&write[..len]);
            Ok(())
        }

        fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            self.transfers.push(words.to_vec());
            let reg = words[0] & 0x7f;

            match reg {
                reg if reg == Register::WHO_AM_I as u8 => words[1] = WHO_AM_I_EXPECTED,
                reg if reg == Register::GyroXoutH as u8 && words.len() == 7 => {
                    words[1..].copy_from_slice(&[0x01, 0x02, 0xff, 0xfe, 0x7f, 0xff]);
                }
                _ => {
                    for word in &mut words[1..] {
                        *word = 0;
                    }
                }
            }

            Ok(())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn bit_helpers_only_modify_requested_mask() {
        assert_eq!(set_bits(0b0000_0011, 0b0000_1100), 0b0000_1111);
        assert_eq!(clear_bits(0b0000_1111, 0b0000_1010), 0b0000_0101);
        assert_eq!(toggle_bits(0b0000_1100, 0b0000_1010), 0b0000_0110);
        assert!(is_bits_set(0b0000_1110, 0b0000_0110));
        assert!(!is_bits_set(0b0000_0100, 0b0000_0110));
        assert!(is_any_bit_set(0b0000_0100, 0b0000_0110));
    }

    #[test]
    fn read_reg_sets_read_bit_and_toggles_chip_select() {
        let mut spi = MockSpi::default();
        let mut cs = MockPin::default();

        let value = read_reg(&mut spi, &mut cs, Register::WHO_AM_I).unwrap();

        assert_eq!(value, WHO_AM_I_EXPECTED);
        assert_eq!(spi.transfers, [std::vec![0xf5, 0x00]]);
        assert_eq!(cs.states, [false, true]);
    }

    #[test]
    fn read_gyro_raw_decodes_big_endian_signed_axes() {
        let mut spi = MockSpi::default();
        let mut cs = MockPin::default();

        let gyro = read_gyro_raw(&mut spi, &mut cs).unwrap();

        assert_eq!(gyro, [258, -2, 32767]);
        assert_eq!(spi.transfers[0][0], (Register::GyroXoutH as u8) | 0x80);
        assert_eq!(cs.states, [false, true]);
    }

    #[test]
    fn decode_accel_temp_gyro_burst_decodes_big_endian_axes() {
        let frame = [
            0x00, 0x10, 0x00, 0xf0, 0x00, 0x20, 0x00, 0x01, 0x23, 0x7f, 0xff, 0x80, 0x00, 0xff,
            0xfe,
        ];

        let sample = decode_accel_temp_gyro_burst(&frame).unwrap();

        assert_eq!(sample.acc_raw, [4096, -4096, 8192]);
        assert_eq!(sample.temp_raw, 0x0123);
        assert_eq!(sample.gyro_raw, [32767, -32768, -2]);
    }

    #[test]
    fn decode_accel_temp_gyro_burst_rejects_short_or_implausible_frames() {
        assert_eq!(
            decode_accel_temp_gyro_burst(&[0; SPI_ARRAY_SIZE - 1]),
            Err(DecodeError::FrameTooShort)
        );

        assert_eq!(
            decode_accel_temp_gyro_burst(&[0; SPI_ARRAY_SIZE]),
            Err(DecodeError::ImplausibleFrame)
        );

        assert_eq!(
            decode_accel_temp_gyro_burst(&[0xff; SPI_ARRAY_SIZE]),
            Err(DecodeError::ImplausibleFrame)
        );
    }

    #[test]
    fn decode_gyro_burst_allows_zero_rate_but_rejects_bad_frames() {
        assert_eq!(
            decode_gyro_burst(&[0, 0, 0, 0, 0, 0, 0]).unwrap(),
            [0, 0, 0]
        );
        assert_eq!(
            decode_gyro_burst(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Err(DecodeError::ImplausibleFrame)
        );
        assert_eq!(decode_gyro_burst(&[0; 6]), Err(DecodeError::FrameTooShort));
    }

    #[test]
    fn sample_freshness_only_depends_on_sequence_change() {
        assert_eq!(classify_sample_freshness(10, 10), SampleFreshness::Stale);
        assert_eq!(classify_sample_freshness(10, 11), SampleFreshness::Fresh);
        assert_eq!(
            classify_sample_freshness(u32::MAX, 0),
            SampleFreshness::Fresh
        );
    }

    #[test]
    fn gyro_sample_rate_writes_expected_divider_for_dlpf_rate() {
        let mut spi = MockSpi::default();
        let mut cs = MockPin::default();
        let mut delay = MockDelay::default();

        gyro_set_sample_rate(&mut spi, &mut cs, 500, GyroDlpf::Bandwidth184Hz, &mut delay).unwrap();

        assert!(spi.writes.contains(&std::vec![Register::CONFIG as u8, 1]));
        assert!(
            spi.writes
                .contains(&std::vec![Register::SMPRT_DIV as u8, 1])
        );
        assert!(delay.delay_ns_total >= 30_000_000);
    }
}
