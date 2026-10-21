//! Task groups: sets of definitions a firmware selects with one
//! `#[group(from = ..)]`, binding their resources and configuration once and
//! giving each member only its priority and interrupt.

/// The SPI1 IMU path: the data-ready interrupt, the polling and owner
/// tasks, the receive DMA completion, the I/O watchdog and its timeout, and
/// the sample parser.
#[ferroforge::group]
pub mod spi1_imu {
    pub use ferrowasp_stm32_critical::tasks::spi1::imu_data_ready;
    pub use ferrowasp_stm32_critical::tasks::spi1::io_watchdog;
    pub use ferrowasp_stm32_critical::tasks::spi1::spi1_owner_service;
    pub use ferrowasp_stm32_critical::tasks::spi1::spi1_parser;
    pub use ferrowasp_stm32_critical::tasks::spi1::spi1_poll;
    pub use ferrowasp_stm32_critical::tasks::spi1::spi1_rx_dma;
    pub use ferrowasp_stm32_critical::tasks::spi1::spi1_timeout;
}

/// Every flight task a board runs the same way: arming and actuator output,
/// the control loop, DShot service and ESC telemetry, RC, USB, OSD, storage,
/// battery sensing, the observer and the heartbeat. Spawns between members
/// are wired here.
#[cfg(feature = "dshot")]
#[ferroforge::group]
pub mod flight {
    pub use crate::actuator_output;
    pub use crate::dshot_fallback;
    pub use crate::dshot_service;
    pub use crate::esc_manager_task;
    pub use crate::flash_manager_task;
    pub use crate::heartbeat;
    pub use crate::rc_input;
    pub use crate::usb_fs;
    pub use ferrowasp_border::tasks::observer_task;
    pub use ferrowasp_flight_control::tasks::control_loop;
    pub use ferrowasp_osd::tasks::osd_refresh;
    pub use ferrowasp_osd::tasks::rc_telemetry;
    pub use ferrowasp_safety::tasks::actuator_idle_notify;
    pub use ferrowasp_safety::tasks::safety_master;
    pub use ferrowasp_stm32::tasks::adc::adc1_polling;
    pub use ferrowasp_stm32::tasks::adc::dma_adc1;
}
