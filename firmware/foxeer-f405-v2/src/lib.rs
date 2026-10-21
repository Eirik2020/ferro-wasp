#![deny(unsafe_code)]
#![no_std]

ferrowasp_stm32_tasks::flight_app_facade! {
    board_name: "Foxeer F405 V2",
    msp_identifier: *b"FXR2",
    target_name: "foxeer_f405_v2",
    mcu: mspv2::rpc::McuKind::Stm32F405,
}

use defmt_rtt as _;
use panic_probe as _;

pub mod board;

pub use ferrowasp_stm32f4::adc as stm32_adc;
pub use ferrowasp_stm32f4::clocks as stm32_clocks;
pub use ferrowasp_stm32f4::hal_prelude::*;
pub use ferrowasp_stm32f4::scheduler as stm32_scheduler;
#[cfg(feature = "mspv2_configurator")]
pub use ferrowasp_stm32f4::signature::device_uid;
pub use ferrowasp_stm32f4::spi_dma as stm32_spi;
pub use ferrowasp_stm32f4::spi_dma::*;
pub use ferrowasp_stm32f4::system_bootloader as stm32_bootloader;
pub use ferrowasp_stm32f4::timebase as stm32_timebase;
pub use ferrowasp_stm32f4::uart_dma as stm32_uart;
pub use ferrowasp_stm32f4::usb_serial as stm32_usb;
pub use ferrowasp_stm32f4::watchdog as stm32_watchdog;
pub use fugit::Rate;
pub use rtic_monotonics::systick::prelude::*;
pub use stm32_usb::UsbDeviceState;

pub type DshotShared = board::aliases::DshotMotorBank;
pub type AdcTransfer = board::aliases::Adc1ObservationTransfer;
pub type ControlScheduler = board::aliases::ControlScheduler;
pub type IoTimebase = stm32_timebase::MicrosecondTimebase<board::aliases::IoTimebaseTimer>;
pub type IoWatchdog = board::aliases::IoWatchdog;
pub type FlashDevice = board::aliases::Spi2Flash;

/// Logged control ticks queued between the observer and the SPI NOR flash,
/// whose page programs are quick and steady: 32 ms at 2 kHz.
pub const BLACKBOX_STREAM_CAPACITY: usize = 64;
