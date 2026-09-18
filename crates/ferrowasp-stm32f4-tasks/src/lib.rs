//! RTIC task definitions for STM32F4 flight boards.
//!
//! A definition here is the task body; the firmware's `app!` declaration
//! selects it and binds the board's resources, priority and interrupt. Board
//! facts reach a definition as its resources or configuration, never by
//! naming a board.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod adc;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod arming;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod diagnostics;
#[cfg(all(target_arch = "arm", feature = "stm32f405", feature = "dshot"))]
mod dshot;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod esc;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod osd;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod prelude;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod rc;
pub mod snapshots;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod spi1;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod uart1;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod uart2;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod uart4;

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use adc::{adc1_polling, dma_adc1};
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use arming::{actuator_idle_notify, safety_master, warn_arming_abort};
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use diagnostics::heartbeat;
#[cfg(all(target_arch = "arm", feature = "stm32f405", feature = "dshot"))]
pub use dshot::{dshot_dma_complete, service_dshot_dma_irq};
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use esc::esc_manager_task;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use osd::osd_refresh;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use rc::{neutralize_rc_input, rc_input};
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use spi1::{
    ParsedImuSample, SPI1_MAILBOX, Spi1Device, Spi1Executor, Spi1ImuKind, Spi1Mailbox,
    imu_data_ready, io_watchdog, spi1_owner_service, spi1_parser, spi1_poll, spi1_rx_dma,
    spi1_timeout,
};
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use uart1::{usart1_rx_dma_transfer, usart1_rx_peripheral};
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use uart2::{
    Uart2OwnedRxBridge, publish_uart2_owned, record_uart2_discontinuity, record_uart2_dma_error,
    usart2_rx_dma_transfer, usart2_rx_peripheral,
};
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use uart4::{
    uart4_rx_dma_transfer, uart4_rx_peripheral, uart4_tx_dma_transfer, uart4_tx_worker,
};
