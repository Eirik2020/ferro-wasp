//! Reusable STM32H7 external-interrupt input initialization.

use ferrowasp_stm32_critical::exti::InterruptInput;
use stm32h7xx_hal::{
    gpio::{Edge, ExtiPin},
    pac::{EXTI, SYSCFG},
};

/// An interrupt-capable input the HAL configured, as the neutral
/// [`InterruptInput`] the task that services it names.
pub struct ExtiInput<P>(P);

impl<P: ExtiPin> InterruptInput for ExtiInput<P> {
    fn check_interrupt(&self) -> bool {
        self.0.check_interrupt()
    }

    fn clear_interrupt_pending_bit(&mut self) {
        self.0.clear_interrupt_pending_bit();
    }
}

/// Configures an interrupt-capable input without embedding a board pin choice.
pub fn init_input<P>(mut pin: P, syscfg: &mut SYSCFG, exti: &mut EXTI, edge: Edge) -> ExtiInput<P>
where
    P: ExtiPin,
{
    pin.make_interrupt_source(syscfg);
    pin.trigger_on_edge(exti, edge);
    pin.clear_interrupt_pending_bit();
    pin.enable_interrupt(exti);
    ExtiInput(pin)
}
