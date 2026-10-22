//! The safety-critical half of `ferrowasp-stm32` (ADR-0060): the motor output,
//! SPI and IMU, UART, timing, routing and reset mechanisms. The non-critical
//! half (USB, status LEDs, storage, ADC) stays in `ferrowasp-stm32`, which
//! re-exports these modules under their former paths.
//!
//! Safety-critical.

#![deny(unsafe_code)]
#![no_std]

pub mod app_config;
pub mod app_storage;
pub mod board_manifest;
pub mod board_routes;
#[cfg(any(test, feature = "dshot"))]
pub mod dshot_bank;
#[cfg(any(test, feature = "dshot"))]
pub mod dshot_lanes;
pub mod exti;
pub mod memory;
pub mod scheduler;
pub mod serial;
pub mod spi;
pub mod spi1_imu;
pub mod spi_common;
pub mod system_reset;
#[cfg(all(feature = "tasks", target_arch = "arm"))]
pub mod tasks;
pub mod timebase;
pub mod timer_dma;
pub mod timer_tick;
pub mod uart_common;
pub mod uart_port;
pub mod watchdog;
