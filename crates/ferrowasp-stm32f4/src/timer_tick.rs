//! The STM32F4 periodic timer behind the control scheduler and the I/O
//! watchdog.

pub use ferrowasp_stm32::timer_tick::TimerTick;
use stm32f4xx_hal::timer::{CounterHz, Instance};

/// A timer started at a fixed rate with its update interrupt enabled.
pub struct PeriodicTimer<TIM>(pub CounterHz<TIM>);

impl<TIM> TimerTick for PeriodicTimer<TIM>
where
    TIM: Instance,
{
    fn acknowledge_tick(&mut self) {
        use stm32f4xx_hal::prelude::*;

        self.0.clear_all_flags();
    }
}
