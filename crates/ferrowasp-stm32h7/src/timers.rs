//! The H7 timers under the shared timebase, scheduler, and watchdog traits.

use ferrowasp_io_core::time::{TimestampMicros, WrappingCounterExtender};
use ferrowasp_stm32f4::timebase::{MICROSECOND_TIMEBASE_HZ, Timebase};
use ferrowasp_stm32f4::timer_tick::TimerTick;
use ferrowasp_stm32f4::watchdog::IO_WATCHDOG_HZ;
use stm32h7xx_hal::{
    pac::{TIM2, TIM4, TIM6},
    prelude::*,
    rcc::{CoreClocks, rec},
    timer::{Event, Timer},
};

/// A free-running microsecond clock on 32-bit TIM2.
pub struct MicrosecondTimebase {
    timer: Timer<TIM2>,
    extender: WrappingCounterExtender,
}

impl MicrosecondTimebase {
    pub fn new(timer: TIM2, prec: rec::Tim2, clocks: &CoreClocks) -> Self {
        let mut timer = timer.tick_timer(MICROSECOND_TIMEBASE_HZ.Hz(), prec, clocks);
        timer.resume();
        Self {
            timer,
            extender: WrappingCounterExtender::new(u32::MAX),
        }
    }

    pub fn now(&mut self) -> TimestampMicros {
        TimestampMicros(self.extender.observe(self.timer.counter()))
    }
}

impl Timebase for MicrosecondTimebase {
    fn now(&mut self) -> TimestampMicros {
        MicrosecondTimebase::now(self)
    }
}

/// A timer raising its update interrupt at a fixed rate.
pub struct PeriodicTimer<TIM>(Timer<TIM>);

macro_rules! periodic_tick {
    ($($TIM:ident),+) => {
        $(
            impl TimerTick for PeriodicTimer<$TIM> {
                fn acknowledge_tick(&mut self) {
                    self.0.clear_irq();
                }
            }
        )+
    };
}

periodic_tick!(TIM4, TIM6);

/// The control scheduler: TIM4 raising its update interrupt at `rate_hz`.
pub fn init_control_scheduler(
    timer: TIM4,
    prec: rec::Tim4,
    clocks: &CoreClocks,
    rate_hz: u32,
) -> PeriodicTimer<TIM4> {
    let mut scheduler = timer.timer(rate_hz.Hz(), prec, clocks);
    scheduler.listen(Event::TimeOut);
    PeriodicTimer(scheduler)
}

/// The I/O watchdog: TIM6 raising its update interrupt at `IO_WATCHDOG_HZ`.
pub fn init_io_watchdog(timer: TIM6, prec: rec::Tim6, clocks: &CoreClocks) -> PeriodicTimer<TIM6> {
    let mut watchdog = timer.timer(IO_WATCHDOG_HZ.Hz(), prec, clocks);
    watchdog.listen(Event::TimeOut);
    PeriodicTimer(watchdog)
}
