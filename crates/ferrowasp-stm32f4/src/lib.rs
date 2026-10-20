//! The STM32F4 backend: the F4 HAL under the chip-neutral mechanisms of
//! `ferrowasp-stm32`. Nothing builds without a chip feature.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", any(feature = "stm32f401", feature = "stm32f405")))]
pub mod adc;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod signature;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub mod usb_serial;

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::board_routes;
pub use ferrowasp_stm32f4_critical::clocks;
#[cfg(all(target_arch = "arm", feature = "stm32f405", feature = "dshot"))]
pub use ferrowasp_stm32f4_critical::dshot;
#[cfg(all(target_arch = "arm", any(feature = "stm32f401", feature = "stm32f405")))]
pub use ferrowasp_stm32f4_critical::exti;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::hal_prelude;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::scheduler;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::spi_dma;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::system_bootloader;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::timebase;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::timer_tick;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::uart_dma;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use ferrowasp_stm32f4_critical::watchdog;
