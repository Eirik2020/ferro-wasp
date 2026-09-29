use stm32f4xx_hal::{
    prelude::*,
    rcc::Rcc,
    time::Hertz,
    timer::{CounterHz, Error, Event, Instance},
};

pub fn init_control_scheduler<TIM>(
    timer: TIM,
    rcc: &mut Rcc,
    rate: Hertz,
) -> Result<CounterHz<TIM>, Error>
where
    TIM: Instance,
{
    let mut scheduler = timer.counter_hz(rcc);
    scheduler.start(rate)?;
    scheduler.listen(Event::Update);
    Ok(scheduler)
}

pub use crate::timer_tick::TimerTick;

pub fn acknowledge_control_tick(scheduler: &mut impl TimerTick) {
    scheduler.acknowledge_tick();
}
