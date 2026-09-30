//! embedded-hal 1.0 views of the H7 HAL's embedded-hal 0.2 types, for the
//! drivers and the SPI owner, which take 1.0 traits.

use core::convert::Infallible;
use embedded_hal::{delay::DelayNs, digital, spi};
use embedded_hal_02::blocking::spi::{Transfer as Transfer02, Write as Write02};
use stm32h7xx_hal::gpio::{Output, Pin, PushPull};

/// An output pin as an embedded-hal 1.0 `OutputPin`.
pub struct OutputPin<const P: char, const N: u8> {
    pin: Pin<P, N, Output<PushPull>>,
}

impl<const P: char, const N: u8> OutputPin<P, N> {
    pub const fn new(pin: Pin<P, N, Output<PushPull>>) -> Self {
        Self { pin }
    }
}

impl<const P: char, const N: u8> digital::ErrorType for OutputPin<P, N> {
    type Error = Infallible;
}

impl<const P: char, const N: u8> digital::OutputPin for OutputPin<P, N> {
    fn set_low(&mut self) -> Result<(), Infallible> {
        self.pin.set_low();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        self.pin.set_high();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiBusError;

impl spi::Error for SpiBusError {
    fn kind(&self) -> spi::ErrorKind {
        spi::ErrorKind::Other
    }
}

/// A blocking HAL SPI bus as an embedded-hal 1.0 `SpiBus`, for bring-up
/// before the bus is handed to DMA.
pub struct SpiBus<S> {
    spi: S,
}

impl<S> SpiBus<S> {
    pub const fn new(spi: S) -> Self {
        Self { spi }
    }

    pub fn into_inner(self) -> S {
        self.spi
    }
}

impl<S> spi::ErrorType for SpiBus<S> {
    type Error = SpiBusError;
}

impl<S> spi::SpiBus<u8> for SpiBus<S>
where
    S: Transfer02<u8> + Write02<u8>,
{
    fn read(&mut self, words: &mut [u8]) -> Result<(), SpiBusError> {
        words.fill(0);
        self.spi
            .transfer(words)
            .map(|_| ())
            .map_err(|_| SpiBusError)
    }

    fn write(&mut self, words: &[u8]) -> Result<(), SpiBusError> {
        self.spi.write(words).map_err(|_| SpiBusError)
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), SpiBusError> {
        // Clock the longer of the two, as the trait requires: bytes past
        // `write` are sent as zero and bytes past `read` are discarded.
        let mut word = [0u8; 1];
        for index in 0..read.len().max(write.len()) {
            word[0] = write.get(index).copied().unwrap_or(0);
            self.spi.transfer(&mut word).map_err(|_| SpiBusError)?;
            if let Some(slot) = read.get_mut(index) {
                *slot = word[0];
            }
        }
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), SpiBusError> {
        self.spi
            .transfer(words)
            .map(|_| ())
            .map_err(|_| SpiBusError)
    }

    fn flush(&mut self) -> Result<(), SpiBusError> {
        Ok(())
    }
}

/// A busy-wait delay from the core clock, for bring-up.
pub struct CycleDelay {
    cycles_per_us: u32,
}

impl CycleDelay {
    pub const fn new(core_clock_hz: u32) -> Self {
        Self {
            cycles_per_us: core_clock_hz / 1_000_000,
        }
    }
}

impl DelayNs for CycleDelay {
    fn delay_ns(&mut self, ns: u32) {
        let cycles = u64::from(ns) * u64::from(self.cycles_per_us) / 1_000;
        cortex_m::asm::delay(cycles.min(u64::from(u32::MAX)) as u32);
    }

    fn delay_us(&mut self, us: u32) {
        for _ in 0..us {
            cortex_m::asm::delay(self.cycles_per_us);
        }
    }

    fn delay_ms(&mut self, ms: u32) {
        for _ in 0..ms {
            self.delay_us(1_000);
        }
    }
}

/// The HAL's ADC calibration takes an embedded-hal 0.2 delay.
impl embedded_hal_02::blocking::delay::DelayUs<u8> for CycleDelay {
    fn delay_us(&mut self, us: u8) {
        DelayNs::delay_us(self, u32::from(us));
    }
}
