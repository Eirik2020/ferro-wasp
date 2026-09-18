//! RTIC task definitions for STM32F4 flight boards.
//!
//! A definition here is the task body; the firmware's `app!` declaration
//! selects it and binds the board's resources, priority and interrupt. Board
//! facts reach a definition as its resources or configuration, never by
//! naming a board.

#![deny(unsafe_code)]
#![no_std]

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod diagnostics;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod esc;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod osd;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod prelude;
pub mod snapshots;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod uart4;

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use diagnostics::heartbeat;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use esc::esc_manager_task;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use osd::osd_refresh;
#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use uart4::uart4_tx_worker;
