//! An external-interrupt input, as the task that services its edge sees it:
//! the chip family's backend wraps its HAL pin in a type that implements this,
//! so the task names no HAL.
//!
//! Requirements: HW-SPI-001
//! Safety-critical.

pub trait InterruptInput {
    /// Whether this input's edge raised the interrupt now being served.
    fn check_interrupt(&self) -> bool;
    fn clear_interrupt_pending_bit(&mut self);
}
