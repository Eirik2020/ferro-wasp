//! The TBS Lucid H7's hardware data, the facts its app's board support turns into
//! peripherals. Plain data over the shared crates: no chip HAL, so these
//! tables and their tests build on the host.

#![cfg_attr(not(test), no_std)]

pub mod manifest;
pub mod profiles;
pub mod routes;
pub mod serial;
pub mod usb;

pub use manifest::{
    BOARD_CAPABILITIES, BOARD_IDENTITY, CLAIMS, CONTROL_SCHEDULER_TIMER, DISPATCHER_IRQS,
    ESC_TELEMETRY_CLAIMS, HARDWARE_IRQS, HSE_FREQUENCY_HZ, IMU_DATA_READY_PIN, IO_TIMEBASE_TIMER,
    IO_WATCHDOG_TIMER, PIN_MAP, SD_STORAGE_CLAIMS, SWD_PINS, SYSTEM_CLOCK_HZ, Spi1ImuKind,
    TIMER_GROUPS, USB_CDC_CLAIMS, USB_FS_PINS,
};
pub use routes::{
    ACTIVE_DMA_ROUTES, ACTIVE_IO_DMA_ROUTES, ACTIVE_SPI_ROUTES, ESC_TELEMETRY_DMA_ROUTE,
    MOTOR_DSHOT_DMA_ROUTES, SERIAL_ROUTES,
};
pub use usb::USB_CDC_IDENTITY;
