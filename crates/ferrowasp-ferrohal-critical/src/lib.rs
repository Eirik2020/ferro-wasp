//! The safety-critical half of FerroWasp's backend on FerroHAL: the motor output,
//! SPI and IMU, UART, timing and reset mechanisms. The non-critical half (USB, ADC,
//! the chip's ID) stays in `ferrowasp-ferrohal`, which re-exports these.
//!
//! Much of this goes once FerroWasp uses FerroHAL's own model: it presents FerroHAL's
//! drivers through the transfer traits FerroWasp's shared logic was written against.
//! That part is marked `TRANSITION:` where it lives.
//!
//! Safety-critical.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod chip_time;
pub mod clocks;
#[cfg(all(
    target_arch = "arm",
    feature = "dshot",
    any(feature = "stm32f405", feature = "stm32h743")
))]
pub mod dshot;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod exti;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod spi_dma;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod system_bootloader;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod uart_dma;
