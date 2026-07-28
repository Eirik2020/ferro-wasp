#![deny(unsafe_code)]
#![no_std]

pub mod adc;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod advanced_pwm;
pub mod app_config;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod app_storage;
pub mod board_manifest;
pub mod board_routes;
#[cfg(all(target_arch = "arm", feature = "stm32f401"))]
pub mod bringup;
pub mod clocks;
#[cfg(all(target_arch = "arm", feature = "dshot"))]
#[allow(unsafe_code)]
pub mod dshot;
#[cfg(all(target_arch = "arm", any(feature = "stm32f401", feature = "stm32f405")))]
pub mod exti;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod hal_prelude;
pub mod memory;
pub mod pwm_config;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod scheduler;
pub mod serial;
pub mod spi;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod spi_dma;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod static_pwm;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod timebase;
pub mod timer_dma;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod uart_dma;
pub mod usb_serial;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod watchdog;
