//! STM32 mechanisms that do not depend on a chip family's HAL.
//!
//! The UART, SPI, ADC and DShot state machines, serial port binding, and the
//! shared memory and board-description types live here. Each family's
//! backend implements the small traits these define over its own HAL:
//! `ferrowasp-ferrohal`, for every chip.

#![deny(unsafe_code)]
#![no_std]

pub mod adc;
pub mod pwm_config;
pub mod sd_storage;
pub mod status_led;
#[cfg(all(feature = "tasks", target_arch = "arm"))]
pub mod tasks;
pub mod usb_serial;

// The FerroHAL backend, folded in: what `ferrowasp-ferrohal` presented, now
// reached directly. Gated to a target with a chip feature.
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use ferrohal_stm32 as hal;
#[cfg(all(
    target_arch = "arm",
    feature = "dshot",
    any(feature = "stm32f405", feature = "stm32h743")
))]
pub use ferrowasp_stm32_critical::dshot;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use ferrowasp_stm32_critical::{chip_time, clocks, exti, spi_dma, system_bootloader, uart_dma};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod hal_prelude {
    pub use ferrohal_stm32::device as pac;
}
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod signature {
    pub use ferrohal_stm32::signature::device_uid;
}

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
