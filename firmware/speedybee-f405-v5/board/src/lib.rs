//! The SpeedyBee F405 V5's hardware data, the facts its app's board support
//! turns into peripherals. Plain data over the shared crates: no chip HAL, so
//! these tables and their tests build on the host.
//!
//! Bring-up scope, per the board plan: the four motor lanes on TIM3 and TIM2
//! (TIM3 leading TIM2 from one start, the arrangement FerroHAL's run R proved
//! on hardware), the ICM42688-P on SPI1, SBUS RC on the DJI O4 connector
//! (UART6), the configurator line on UART1, ESC telemetry on UART5, the SPL06
//! barometer on I2C1, the inverted buzzer on PB9, and blackbox storage on
//! SPI3. Deferred until their drivers exist: the MAX7456 OSD on SPI2, the
//! servo, LED-strip, camera-control, PPM and PINIO pads, and the free UART2,
//! UART3 and UART4 pads.

#![cfg_attr(not(test), no_std)]

pub mod manifest;
pub mod profiles;
pub mod routes;
pub mod serial;
pub mod usb;

pub use manifest::{
    BOARD_CAPABILITIES, BOARD_IDENTITY, CLAIMS, CONTROL_SCHEDULER_TIMER, DISPATCHER_IRQS,
    ESC_TELEMETRY_CLAIMS, HARDWARE_IRQS, HSE_FREQUENCY_HZ, IMU_DATA_READY_PIN, IO_TIMEBASE_TIMER,
    IO_WATCHDOG_TIMER, PIN_MAP, SPI_FLASH_CLAIMS, SWD_PINS, SYSTEM_CLOCK_HZ, Spi1ImuKind,
    TIMER_GROUPS, USB_CDC_CLAIMS, USB_FS_PINS,
};
pub use routes::{
    ACTIVE_DMA_ROUTES, ACTIVE_IO_DMA_ROUTES, ACTIVE_SPI_ROUTES, ESC_TELEMETRY_DMA_ROUTE,
    MOTOR_DSHOT_DMA_ROUTES, SERIAL_ROUTES, SPI3_FLASH,
};
pub use usb::USB_CDC_IDENTITY;
