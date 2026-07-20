#[cfg(all(target_arch = "arm", feature = "ferrowasp-fcu3"))]
pub mod aliases;
#[cfg(all(target_arch = "arm", feature = "ferrowasp-fcu3"))]
pub mod init;
pub mod manifest;
pub mod profiles;
pub mod routes;
pub mod serial;
#[cfg(all(target_arch = "arm", feature = "ferrowasp-fcu3"))]
pub mod storage;

pub use manifest::{
    BOARD_IDENTITY, CLAIMS, CONTROL_SCHEDULER_TIMER, DISPATCHER_IRQS, HARDWARE_IRQS,
    IO_TIMEBASE_TIMER, IO_WATCHDOG_TIMER, PIN_MAP, SWD_PINS, TIMER_GROUPS, USB_FS_PINS,
};
pub use routes::{
    ACTIVE_DMA_ROUTES, ACTIVE_SPI_ROUTES, ACTIVE_STATIC_PWM_ROUTES, MOTOR_DSHOT_DMA_ROUTES,
    MOTOR1_DSHOT_DMA_ROUTE,
};
pub use serial::ACTIVE_SERIAL_ROUTES;
