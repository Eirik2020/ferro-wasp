//! Task groups: sets of definitions a firmware selects with one
//! `#[group(from = ..)]`, binding their resources and configuration once and
//! giving each member only its priority and interrupt.

/// The SPI1 IMU path: the data-ready interrupt, the polling and owner
/// tasks, the receive DMA completion, the I/O watchdog and its timeout, and
/// the sample parser.
#[ferroforge::group]
pub mod spi1_imu {
    pub use crate::imu_data_ready;
    pub use crate::io_watchdog;
    pub use crate::spi1_owner_service;
    pub use crate::spi1_parser;
    pub use crate::spi1_poll;
    pub use crate::spi1_rx_dma;
    pub use crate::spi1_timeout;
}

/// Every flight task a board runs the same way: arming and actuator output,
/// the control loop, DShot service and ESC telemetry, RC, USB, OSD, storage,
/// battery sensing and the heartbeat. Spawns between members are wired here.
#[cfg(feature = "dshot")]
#[ferroforge::group]
pub mod flight {
    pub use crate::actuator_idle_notify;
    pub use crate::actuator_output;
    pub use crate::adc1_polling;
    pub use crate::control_loop;
    pub use crate::dma_adc1;
    pub use crate::dshot_fallback;
    pub use crate::dshot_service;
    pub use crate::esc_manager_task;
    pub use crate::flash_manager_task;
    pub use crate::heartbeat;
    pub use crate::osd_refresh;
    pub use crate::rc_input;
    pub use crate::rc_telemetry;
    pub use crate::safety_master;
    pub use crate::usb_fs;
}
