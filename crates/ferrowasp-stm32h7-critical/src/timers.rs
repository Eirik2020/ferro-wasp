//! The H7 timers under the shared timebase, scheduler, and watchdog traits.

use ferrowasp_io_contracts::time::{TimestampMicros, WrappingCounterExtender};
use ferrowasp_stm32_critical::timebase::{MICROSECOND_TIMEBASE_HZ, Timebase};
use ferrowasp_stm32_critical::timer_tick::TimerTick;
use stm32h7xx_hal::{
    pac::{TIM2, TIM3, TIM4, TIM5, TIM6, TIM7, TIM12, TIM13, TIM14, TIM15, TIM16, TIM17},
    prelude::*,
    rcc::{CoreClocks, rec},
    timer::{Event, Timer},
};

/// A free-running microsecond clock on a 32-bit timer, TIM2 or TIM5: the
/// board chooses which.
pub struct MicrosecondTimebase<TIM> {
    timer: Timer<TIM>,
    extender: WrappingCounterExtender,
}

macro_rules! microsecond_timebase {
    ($($TIM:ident: $rec:ident),+) => {
        $(
            impl MicrosecondTimebase<$TIM> {
                pub fn new(timer: $TIM, prec: rec::$rec, clocks: &CoreClocks) -> Self {
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

            impl Timebase for MicrosecondTimebase<$TIM> {
                fn now(&mut self) -> TimestampMicros {
                    MicrosecondTimebase::<$TIM>::now(self)
                }
            }
        )+
    };
}

microsecond_timebase!(TIM2: Tim2, TIM5: Tim5);

/// A timer raising its update interrupt at a fixed rate: the control
/// scheduler and the I/O watchdog, on whichever timers the board gives them.
pub struct PeriodicTimer<TIM>(Timer<TIM>);

macro_rules! periodic_timer {
    ($($TIM:ident: $rec:ident),+) => {
        $(
            impl PeriodicTimer<$TIM> {
                pub fn new(timer: $TIM, prec: rec::$rec, clocks: &CoreClocks, rate_hz: u32) -> Self {
                    let mut periodic = timer.timer(rate_hz.Hz(), prec, clocks);
                    periodic.listen(Event::TimeOut);
                    Self(periodic)
                }
            }

            impl TimerTick for PeriodicTimer<$TIM> {
                fn acknowledge_tick(&mut self) {
                    self.0.clear_irq();
                }
            }
        )+
    };
}

periodic_timer!(
    TIM2: Tim2, TIM3: Tim3, TIM4: Tim4, TIM5: Tim5, TIM6: Tim6, TIM7: Tim7,
    TIM12: Tim12, TIM13: Tim13, TIM14: Tim14, TIM15: Tim15, TIM16: Tim16, TIM17: Tim17
);
