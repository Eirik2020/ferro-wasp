//! An EXTI line as the neutral input the task that services it names.
//!
//! Requirements: HW-SPI-001, HW-SPI-002, HW-SPI-003, HW-SPI-004
//! Safety-critical.

use ferrohal_stm32::exti::ExtiHandle;
use ferrowasp_stm32_critical::exti::InterruptInput;

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
