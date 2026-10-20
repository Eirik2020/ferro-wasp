//! The safety-critical half of `ferrowasp-stm32f4` (ADR-0060): the motor output,
//! SPI and IMU, UART, timing, routing and reset mechanisms. The non-critical
//! half (USB, status LEDs, storage, ADC) stays in `ferrowasp-stm32f4`, which
//! re-exports these modules under their former paths.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod board_routes;
pub mod clocks;
#[cfg(all(target_arch = "arm", feature = "stm32f405", feature = "dshot"))]
#[allow(unsafe_code)]
pub mod dshot;
#[cfg(all(target_arch = "arm", any(feature = "stm32f401", feature = "stm32f405")))]
pub mod exti;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod hal_prelude;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod scheduler;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod spi_dma;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
#[allow(unsafe_code)]
pub mod system_bootloader;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod timebase;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod timer_tick;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod uart_dma;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod watchdog;
