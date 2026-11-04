//! FerroWasp's backend on FerroHAL: USB, the ADC and the chip's ID, with the
//! safety-critical half re-exported under its own names. One implementation for every
//! chip, since FerroHAL has one API. Nothing builds without a chip feature.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod adc;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod usb_serial;

#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use ferrohal_stm32 as hal;
pub use ferrowasp_ferrohal_critical::clocks;
#[cfg(all(
    target_arch = "arm",
    feature = "dshot",
    any(feature = "stm32f405", feature = "stm32h743")
))]
pub use ferrowasp_ferrohal_critical::dshot;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use ferrowasp_ferrohal_critical::{chip_time, exti, spi_dma, system_bootloader, uart_dma};

/// The names apps reach the HAL through.
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod hal_prelude {
    pub use ferrohal_stm32::device as pac;
}

/// The chip's 96-bit unique ID.
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod signature {
    pub use ferrohal_stm32::signature::device_uid;
}
