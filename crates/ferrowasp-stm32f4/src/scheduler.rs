#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub fn init_control_scheduler<TIM>(
    timer: TIM,
    rcc: &mut stm32f4xx_hal::rcc::Rcc,
    rate: stm32f4xx_hal::time::Hertz,
) -> Result<stm32f4xx_hal::timer::CounterHz<TIM>, stm32f4xx_hal::timer::Error>
where
    TIM: stm32f4xx_hal::timer::Instance,
{
    use stm32f4xx_hal::{prelude::*, timer::Event};

    let mut scheduler = timer.counter_hz(rcc);
    scheduler.start(rate)?;
    scheduler.listen(Event::Update);
    Ok(scheduler)
}

pub use crate::timer_tick::TimerTick;

pub fn acknowledge_control_tick(scheduler: &mut impl TimerTick) {
    scheduler.acknowledge_tick();
}
