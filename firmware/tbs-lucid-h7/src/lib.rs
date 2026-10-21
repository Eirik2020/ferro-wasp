#![deny(unsafe_code)]
#![no_std]

ferrowasp_stm32_tasks::flight_app_facade! {
    board_name: "TBS Lucid H7",
    msp_identifier: *b"LUH7",
    target_name: "tbs_lucid_h7",
    mcu: mspv2::rpc::McuKind::Unknown(0x0743),
}

use defmt_rtt as _;
use panic_probe as _;

pub mod board;

pub use ferrowasp_stm32::status_led::StatusLed;
pub use ferrowasp_stm32h7::adc as stm32_adc;
pub use ferrowasp_stm32h7::clocks as stm32_clocks;
pub use ferrowasp_stm32h7::hal_prelude::*;
#[cfg(feature = "mspv2_configurator")]
pub use ferrowasp_stm32h7::signature::device_uid;
pub use ferrowasp_stm32h7::spi_dma as stm32_spi;
pub use ferrowasp_stm32h7::spi_dma::*;
pub use ferrowasp_stm32h7::system_bootloader as stm32_bootloader;
pub use ferrowasp_stm32h7::uart_dma as stm32_uart;
pub use ferrowasp_stm32h7::usb_serial as stm32_usb;
pub use fugit::Rate;
pub use rtic_monotonics::systick::prelude::*;
pub use stm32_usb::UsbDeviceState;

pub type DshotShared = board::aliases::DshotMotorBank;
pub type AdcTransfer = board::aliases::Adc1ObservationTransfer;
pub type ControlScheduler = board::aliases::ControlScheduler;
pub type IoTimebase = board::aliases::IoTimebase;
pub type IoWatchdog = board::aliases::IoWatchdog;
/// The microSD card as SPI NOR, so the flash manager and blackbox are the
/// Foxeer's unchanged.
pub type FlashDevice = board::aliases::SdFlash;

/// Logged control ticks queued between the observer and the SD card: half a
/// second at 2 kHz. A card may hold a write busy for up to 250 ms; the 64
/// that suit SPI NOR lost a quarter of a full-rate log here.
pub const BLACKBOX_STREAM_CAPACITY: usize = 1024;
