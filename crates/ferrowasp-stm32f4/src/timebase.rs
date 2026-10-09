use ferrowasp_io_core::time::{TimestampMicros, WrappingCounterExtender};
pub use ferrowasp_stm32::timebase::*;
use stm32f4xx_hal::{
    prelude::*,
    rcc::Rcc,
    timer::{Counter, Error, Instance},
};

pub struct MicrosecondTimebase<TIM> {
    counter: Counter<TIM, MICROSECOND_TIMEBASE_HZ>,
    extender: WrappingCounterExtender,
}

impl<TIM> MicrosecondTimebase<TIM>
where
    TIM: Instance,
{
    pub fn new(timer: TIM, rcc: &mut Rcc) -> Result<Self, Error> {
        let mut counter = timer.counter::<MICROSECOND_TIMEBASE_HZ>(rcc);
        counter.start(MICROSECOND_TIMEBASE_PERIOD_TICKS.micros())?;

        Ok(Self {
            counter,
            extender: WrappingCounterExtender::new(MICROSECOND_TIMEBASE_PERIOD_TICKS),
        })
    }

    pub fn now(&mut self) -> TimestampMicros {
        TimestampMicros(self.extender.observe(self.counter.now().ticks()))
    }
}

impl<TIM> Timebase for MicrosecondTimebase<TIM>
where
    TIM: Instance,
{
    fn now(&mut self) -> TimestampMicros {
        MicrosecondTimebase::now(self)
    }
}
