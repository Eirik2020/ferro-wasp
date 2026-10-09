pub use ferrowasp_stm32::watchdog::*;

use crate::timer_tick::PeriodicTimer;

pub fn init_io_watchdog<TIM>(
    timer: TIM,
    rcc: &mut stm32f4xx_hal::rcc::Rcc,
) -> Result<PeriodicTimer<TIM>, stm32f4xx_hal::timer::Error>
where
    TIM: stm32f4xx_hal::timer::Instance,
{
    use stm32f4xx_hal::{prelude::*, timer::Event};

    let mut watchdog = timer.counter_hz(rcc);
    watchdog.start(IO_WATCHDOG_HZ.Hz())?;
    watchdog.listen(Event::Update);
    Ok(PeriodicTimer(watchdog))
}
