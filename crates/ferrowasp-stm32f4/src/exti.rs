//! Reusable STM32F4 external-interrupt input initialization.

use stm32f4xx_hal::{
    gpio::{Edge, ExtiPin},
    pac::EXTI,
    syscfg::SysCfg,
};

/// Configures an interrupt-capable input without embedding a board pin choice.
pub fn init_input<P>(mut pin: P, syscfg: &mut SysCfg, exti: &mut EXTI, edge: Edge) -> P
where
    P: ExtiPin,
{
    pin.make_interrupt_source(syscfg);
    pin.trigger_on_edge(exti, edge);
    pin.clear_interrupt_pending_bit();
    pin.enable_interrupt(exti);
    pin
}
