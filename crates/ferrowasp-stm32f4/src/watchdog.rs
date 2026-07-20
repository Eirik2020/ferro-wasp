use stm32f4xx_hal::{
    prelude::*,
    rcc::Rcc,
    timer::{CounterHz, Error, Event, Instance},
};

pub const IO_WATCHDOG_HZ: u32 = 8_000;

pub fn init_io_watchdog<TIM>(timer: TIM, rcc: &mut Rcc) -> Result<CounterHz<TIM>, Error>
where
    TIM: Instance,
{
    let mut watchdog = timer.counter_hz(rcc);
    watchdog.start(IO_WATCHDOG_HZ.Hz())?;
    watchdog.listen(Event::Update);
    Ok(watchdog)
}

pub fn acknowledge_watchdog_tick<TIM>(watchdog: &mut CounterHz<TIM>)
where
    TIM: Instance,
{
    watchdog.clear_all_flags();
}
