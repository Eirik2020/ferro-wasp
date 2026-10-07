//! STM32H7 mechanisms for FerroWasp flight boards.
//!
//! Each module puts one H7 peripheral under a HAL-neutral mechanism from
//! `ferrowasp-stm32f4`: the UART, SPI, ADC, and DShot state machines are
//! shared, so an H7 board runs the same receive planning, SPI ownership,
//! command lease, and fault latching as the flight-tested F405 boards. Board
//! facts - which pins, streams, and timers - stay in the board's firmware.
//!
//! All DMA buffers must live in memory DMA1 and DMA2 can reach: AXI SRAM or
//! SRAM1-3, never DTCM. The D-cache stays disabled.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod adc;
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
pub mod sd_storage;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod signature;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod spi_dma;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod timers;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod uart_dma;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod usb_serial;
