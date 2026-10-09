//! The STM32F4 backend: the F4 HAL under the chip-neutral mechanisms of
//! `ferrowasp-stm32`. Nothing builds without a chip feature.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", any(feature = "stm32f401", feature = "stm32f405")))]
pub mod adc;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod advanced_pwm;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod board_routes;
#[cfg(all(target_arch = "arm", feature = "stm32f401"))]
pub mod bringup;
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
pub mod static_pwm;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod timebase;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod timer_tick;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod uart_dma;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod usb_serial;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod watchdog;
