//! A periodic timer whose update tick a handler acknowledges by clearing its
//! flags - the control scheduler and the I/O watchdog alike.

/// Acknowledge one tick, whichever timer a board uses. What a shared task
/// definition bounds on, so a board's timer choice stays in the board.
pub trait TimerTick {
    fn acknowledge_tick(&mut self);
}

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
impl<TIM> TimerTick for stm32f4xx_hal::timer::CounterHz<TIM>
where
    TIM: stm32f4xx_hal::timer::Instance,
{
    fn acknowledge_tick(&mut self) {
        use stm32f4xx_hal::prelude::*;

        self.clear_all_flags();
    }
}
