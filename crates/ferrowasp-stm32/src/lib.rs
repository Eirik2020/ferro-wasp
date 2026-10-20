//! STM32 mechanisms that do not depend on a chip family's HAL.
//!
//! The UART, SPI, ADC and DShot state machines, serial port binding, and the
//! shared memory and board-description types live here. Each family's
//! backend implements the small traits these define over its own HAL:
//! `ferrowasp-stm32f4` for the STM32F4, `ferrowasp-stm32h7` for the H743.

#![deny(unsafe_code)]
#![no_std]

pub mod adc;
pub mod pwm_config;
pub mod status_led;
pub mod usb_serial;

pub use ferrowasp_stm32_critical::app_config;
pub use ferrowasp_stm32_critical::app_storage;
pub use ferrowasp_stm32_critical::board_manifest;
pub use ferrowasp_stm32_critical::board_routes;
#[cfg(feature = "dshot")]
pub use ferrowasp_stm32_critical::dshot_bank;
#[cfg(feature = "dshot")]
pub use ferrowasp_stm32_critical::dshot_lanes;
pub use ferrowasp_stm32_critical::memory;
pub use ferrowasp_stm32_critical::scheduler;
pub use ferrowasp_stm32_critical::serial;
pub use ferrowasp_stm32_critical::spi;
pub use ferrowasp_stm32_critical::spi_common;
pub use ferrowasp_stm32_critical::spi1_imu;
pub use ferrowasp_stm32_critical::system_reset;
pub use ferrowasp_stm32_critical::timebase;
pub use ferrowasp_stm32_critical::timer_dma;
pub use ferrowasp_stm32_critical::timer_tick;
pub use ferrowasp_stm32_critical::uart_common;
pub use ferrowasp_stm32_critical::uart_port;
pub use ferrowasp_stm32_critical::watchdog;
