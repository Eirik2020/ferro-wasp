pub const IO_WATCHDOG_HZ: u32 = 8_000;

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub fn init_io_watchdog<TIM>(
    timer: TIM,
    rcc: &mut stm32f4xx_hal::rcc::Rcc,
) -> Result<stm32f4xx_hal::timer::CounterHz<TIM>, stm32f4xx_hal::timer::Error>
where
    TIM: stm32f4xx_hal::timer::Instance,
{
    use stm32f4xx_hal::{prelude::*, timer::Event};

    let mut watchdog = timer.counter_hz(rcc);
    watchdog.start(IO_WATCHDOG_HZ.Hz())?;
    watchdog.listen(Event::Update);
    Ok(watchdog)
}

pub use crate::timer_tick::TimerTick;

pub fn acknowledge_watchdog_tick(watchdog: &mut impl TimerTick) {
    watchdog.acknowledge_tick();
}
