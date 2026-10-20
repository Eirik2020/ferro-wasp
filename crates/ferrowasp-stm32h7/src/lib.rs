//! STM32H7 mechanisms for FerroWasp flight boards.
//!
//! Each module puts one H7 peripheral under a HAL-neutral mechanism from
//! `ferrowasp-stm32`: the UART, SPI, ADC, and DShot state machines are
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
pub mod sd_storage;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod signature;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub mod usb_serial;

#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::clocks;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::dma_endpoints;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::dshot;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::eh1;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::exti;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::hal_prelude;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::spi_dma;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::system_bootloader;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::timers;
#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use ferrowasp_stm32h7_critical::uart_dma;
