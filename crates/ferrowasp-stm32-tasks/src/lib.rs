//! RTIC task definitions for STM32 flight boards.
//!
//! A definition here is the task body; the firmware's `app!` declaration
//! selects it and binds the board's resources, priority and interrupt. Board
//! facts reach a definition as its resources or configuration, never by
//! naming a board.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(feature = "stm32f405", feature = "stm32h743"))]
compile_error!("select one chip family: `stm32f405` or `stm32h743`");

// A status line is written to USB in one piece, so the transmit buffer must
// hold the longest one.
const _: () = assert!(
    ferrowasp_stm32::usb_serial::USB_CDC_TX_BUFFER_BYTES
        >= ferrowasp_flight::usb_debug::STATUS_LINE_CAPACITY
);

#[cfg(all(
    target_arch = "arm",
    any(feature = "stm32f405", feature = "stm32h743"),
    feature = "dshot"
))]
mod actuator;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod adc;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod arming;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod cli_link;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod control;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod diagnostics;
#[cfg(all(
    target_arch = "arm",
    any(feature = "stm32f405", feature = "stm32h743"),
    feature = "dshot"
))]
mod dshot;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod esc;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod osd;
// A convenience for task bodies; which names a body uses depends on the
// firmware's features, so an unused one is not a mistake.
#[allow(unused_imports)]
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod prelude;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod rc;
pub mod snapshots;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod spi1;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
mod uart_port;

#[cfg(all(
    target_arch = "arm",
    any(feature = "stm32f405", feature = "stm32h743"),
    feature = "dshot"
))]
pub use actuator::{
    ActuatorHardware, ArmingGuard, actuator_output, current_live_arming_guard,
    inject_idle_qualification_fault, take_fresh_motor_outputs, wait_live_arming_hold,
};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use adc::{adc1_polling, dma_adc1};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use arming::{actuator_idle_notify, safety_master, warn_arming_abort};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use cli_link::configurator_link;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use control::{control_loop, motor_command_timestamp};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use diagnostics::heartbeat;
#[cfg(all(
    target_arch = "arm",
    any(feature = "stm32f405", feature = "stm32h743"),
    feature = "dshot"
))]
pub use dshot::{dshot_dma_complete, dshot_service, service_dshot_dma_irq};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use esc::esc_manager_task;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use osd::osd_refresh;
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use rc::{neutralize_rc_input, rc_input, rc_telemetry};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use spi1::{
    ParsedImuSample, SPI1_MAILBOX, Spi1Device, Spi1Executor, Spi1ImuKind, Spi1Mailbox,
    imu_data_ready, io_watchdog, spi1_owner_service, spi1_parser, spi1_poll, spi1_rx_dma,
    spi1_timeout,
};
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use uart_port::{
    uart1_rx_dma, uart1_rx_idle, uart1_tx_dma_complete, uart1_tx_worker, uart2_rx_dma,
    uart2_rx_idle, uart2_tx_dma_complete, uart2_tx_worker, uart3_rx_dma, uart3_rx_idle,
    uart3_tx_dma_complete, uart3_tx_worker, uart4_rx_dma, uart4_rx_idle, uart4_tx_dma_complete,
    uart4_tx_worker, uart5_rx_dma, uart5_rx_idle, uart5_tx_dma_complete, uart5_tx_worker,
    uart6_rx_dma, uart6_rx_idle, uart6_tx_dma_complete, uart6_tx_worker, uart7_rx_dma,
    uart7_rx_idle, uart7_tx_dma_complete, uart7_tx_worker, uart8_rx_dma, uart8_rx_idle,
    uart8_tx_dma_complete, uart8_tx_worker,
};
