//! An external-interrupt input, as the task that services its edge sees it:
//! the chip family's backend wraps its HAL pin in a type that implements this,
//! so the task names no HAL.
//!
//! Requirements: HW-SPI-001, HW-SPI-002, HW-SPI-003, HW-SPI-004
//! Safety-critical.

pub trait InterruptInput {
    /// Whether this input's edge raised the interrupt now being served.
    fn check_interrupt(&self) -> bool;
    fn clear_interrupt_pending_bit(&mut self);
}

/// The FerroHAL backend's input: a routed line's handle, which needs no
/// owner. Present only with a chip feature on a target.
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod ferrohal {
    use ferrohal_stm32::exti::ExtiHandle;

    use super::InterruptInput;

    /// A routed line's handle, which needs no owner.
    pub struct ExtiInput(pub ExtiHandle);

    impl InterruptInput for ExtiInput {
        fn check_interrupt(&self) -> bool {
            self.0.is_pending()
        }

        fn clear_interrupt_pending_bit(&mut self) {
            self.0.clear();
        }
    }
}

#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use ferrohal::ExtiInput;
