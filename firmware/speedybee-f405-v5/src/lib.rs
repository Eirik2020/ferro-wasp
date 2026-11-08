//! Requirements: BUILD-001

#![deny(unsafe_code)]
#![no_std]

ferrowasp_stm32_app::flight_app_facade! {
    board_name: "SpeedyBee F405 V5",
    msp_identifier: *b"SPB5",
    target_name: "speedybee_f405_v5",
    mcu: mspv2::rpc::McuKind::Stm32F405,
}

use defmt_rtt as _;
use panic_probe as _;

pub mod board;

pub use ferrowasp_stm32::adc as stm32_adc;
pub use ferrowasp_stm32::chip_time as stm32_scheduler;
pub use ferrowasp_stm32::chip_time as stm32_timebase;
pub use ferrowasp_stm32::chip_time as stm32_watchdog;
pub use ferrowasp_stm32::clocks as stm32_clocks;
pub use ferrowasp_stm32::hal;
pub use ferrowasp_stm32::hal_prelude::*;
#[cfg(feature = "mspv2_configurator")]
pub use ferrowasp_stm32::signature::device_uid;
pub use ferrowasp_stm32::spi_dma as stm32_spi;
pub use ferrowasp_stm32::spi_dma::*;
pub use ferrowasp_stm32::system_bootloader as stm32_bootloader;
pub use ferrowasp_stm32::uart_dma as stm32_uart;
pub use ferrowasp_stm32::usb_serial as stm32_usb;
pub use fugit::Rate;
pub use rtic_monotonics::systick::prelude::*;
pub use stm32_usb::UsbDeviceState;

pub type DshotShared = board::aliases::DshotMotorBank;
pub type AdcTransfer = board::aliases::Adc1ObservationTransfer;
pub type ControlScheduler = board::aliases::ControlScheduler;
pub type IoTimebase = stm32_timebase::MicrosecondTimebase;
pub type IoWatchdog = board::aliases::IoWatchdog;
pub type FlashDevice = board::aliases::Spi3Flash;

/// Logged control ticks queued between the observer and the SPI NOR flash,
/// whose page programs are quick and steady: 32 ms at 2 kHz.
pub const BLACKBOX_STREAM_CAPACITY: usize = 64;
