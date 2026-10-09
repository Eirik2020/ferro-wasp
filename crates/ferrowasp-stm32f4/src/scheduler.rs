pub use ferrowasp_stm32::scheduler::*;

use crate::timer_tick::PeriodicTimer;

pub fn init_control_scheduler<TIM>(
    timer: TIM,
    rcc: &mut stm32f4xx_hal::rcc::Rcc,
    rate: stm32f4xx_hal::time::Hertz,
) -> Result<PeriodicTimer<TIM>, stm32f4xx_hal::timer::Error>
where
    TIM: stm32f4xx_hal::timer::Instance,
{
    use stm32f4xx_hal::{prelude::*, timer::Event};

    let mut scheduler = timer.counter_hz(rcc);
    scheduler.start(rate)?;
    scheduler.listen(Event::Update);
    Ok(PeriodicTimer(scheduler))
}
