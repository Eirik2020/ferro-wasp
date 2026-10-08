//! STM32 mechanisms that do not depend on a chip family's HAL.
//!
//! The UART, SPI, ADC and DShot state machines, serial port binding, and the
//! shared memory and board-description types live here. Each family's
//! backend implements the small traits these define over its own HAL:
//! `ferrowasp-stm32f4` for the STM32F4, `ferrowasp-stm32h7` for the H743.

#![deny(unsafe_code)]
#![no_std]

pub mod adc;
pub mod app_config;
pub mod app_storage;
pub mod board_manifest;
pub mod board_routes;
#[cfg(any(test, feature = "dshot"))]
pub mod dshot_bank;
pub mod memory;
pub mod pwm_config;
pub mod scheduler;
pub mod serial;
pub mod spi;
pub mod spi_common;
pub mod timebase;
pub mod timer_dma;
pub mod timer_tick;
pub mod uart_common;
pub mod uart_port;
pub mod usb_serial;
pub mod watchdog;
