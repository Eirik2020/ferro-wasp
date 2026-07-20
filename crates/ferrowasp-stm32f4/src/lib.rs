#![deny(unsafe_code)]
#![no_std]

pub mod adc;
pub mod clocks;
#[cfg(all(target_arch = "arm", feature = "dshot"))]
#[allow(unsafe_code)]
pub mod dshot;
#[cfg(target_arch = "arm")]
pub mod hal_prelude;
pub mod memory;
#[cfg(target_arch = "arm")]
pub mod scheduler;
pub mod serial;
pub mod spi;
#[cfg(target_arch = "arm")]
pub mod spi_dma;
#[cfg(target_arch = "arm")]
pub mod static_pwm;
#[cfg(target_arch = "arm")]
pub mod timebase;
pub mod timer_dma;
#[cfg(target_arch = "arm")]
pub mod uart_dma;
#[cfg(target_arch = "arm")]
pub mod watchdog;
