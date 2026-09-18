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

/// A watchdog timer whose tick is acknowledged by clearing its flags,
/// whichever timer a board uses. What a shared task definition bounds on.
pub trait WatchdogTick {
    fn acknowledge_tick(&mut self);
}

impl<TIM> WatchdogTick for CounterHz<TIM>
where
    TIM: Instance,
{
    fn acknowledge_tick(&mut self) {
        self.clear_all_flags();
    }
}

pub fn acknowledge_watchdog_tick(watchdog: &mut impl WatchdogTick) {
    watchdog.acknowledge_tick();
}
