#[cfg(all(target_arch = "arm", feature = "foxeer-f405-v2"))]
pub mod aliases;
#[cfg(all(target_arch = "arm", feature = "foxeer-f405-v2"))]
pub mod init;
pub mod manifest;
pub mod profiles;
#[cfg(all(target_arch = "arm", feature = "foxeer-f405-v2"))]
pub mod pwm;
pub mod routes;
pub mod serial;
#[cfg(all(target_arch = "arm", feature = "foxeer-f405-v2"))]
pub mod storage;

pub use manifest::{
    BOARD_CAPABILITIES, BOARD_IDENTITY, CLAIMS, CONTROL_SCHEDULER_TIMER, DISPATCHER_IRQS,
    HARDWARE_IRQS, HSE_FREQUENCY_HZ, IMU_DATA_READY_PIN, IO_TIMEBASE_TIMER, IO_WATCHDOG_TIMER,
    OPTIONAL_USB_CDC_CLAIMS, PIN_MAP, SWD_PINS, SYSTEM_CLOCK_HZ, Spi1ImuKind, TIMER_GROUPS,
    USB_FS_PINS,
};
pub use routes::{
    ACTIVE_DMA_ROUTES, ACTIVE_SERIAL_ROUTES, ACTIVE_SPI_ROUTES, ACTIVE_STATIC_PWM_ROUTES,
    DEFERRED_MOTOR_DMA_ROUTES,
};
