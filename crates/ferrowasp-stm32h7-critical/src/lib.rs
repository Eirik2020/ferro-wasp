//! The safety-critical half of `ferrowasp-stm32h7` (ADR-0060): the motor output,
//! SPI and IMU, UART, timing, routing and reset mechanisms. The non-critical
//! half (USB, status LEDs, storage, ADC) stays in `ferrowasp-stm32h7`, which
//! re-exports these modules under their former paths.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod clocks;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
#[allow(unsafe_code)]
pub mod dma_endpoints;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod dshot;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod eh1;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod exti;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod hal_prelude;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod spi_dma;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
#[allow(unsafe_code)]
pub mod system_bootloader;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod timers;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod uart_dma;
