//! The system clock the boards run at, and the rates shared tasks read.
//!
//! Requirements: HW-TIME-001
//! Safety-critical.

#[cfg(feature = "stm32f405")]
pub const SYSTEM_CLOCK_HZ: u32 = 168_000_000;
#[cfg(feature = "stm32h743")]
pub const SYSTEM_CLOCK_HZ: u32 = 400_000_000;
