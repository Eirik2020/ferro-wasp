//! The microsecond timebase, the periodic timers behind the control scheduler and
//! the I/O watchdog, and a delay for bring-up.
//!
//! Requirements: HW-TIME-001
//! Safety-critical.

use ferrohal_stm32::{
    rcc::Rcc,
    time::Hertz,
    timer::{ConfigError, Instance, Instance32, Tick, Timebase as HalTimebase},
};
use ferrowasp_io_contracts::time::{TimestampMicros, WrappingCounterExtender};
pub use ferrowasp_stm32_critical::scheduler::*;
pub use ferrowasp_stm32_critical::timebase::*;
pub use ferrowasp_stm32_critical::timer_tick::TimerTick;
pub use ferrowasp_stm32_critical::watchdog::*;

/// A free-running 32-bit timer counting microseconds, extended past its wrap.
pub struct MicrosecondTimebase {
    counter: HalTimebase,
    extender: WrappingCounterExtender,
}

impl MicrosecondTimebase {
    pub fn new<T: Instance32>(rcc: &mut Rcc, timer: T) -> Result<Self, ConfigError> {
        Ok(Self {
            counter: HalTimebase::new(rcc, timer, Hertz(MICROSECOND_TIMEBASE_HZ))?,
            extender: WrappingCounterExtender::new(MICROSECOND_TIMEBASE_PERIOD_TICKS),
        })
    }

    pub fn now(&mut self) -> TimestampMicros {
        TimestampMicros(self.extender.observe(self.counter.now()))
    }
}

impl Timebase for MicrosecondTimebase {
    fn now(&mut self) -> TimestampMicros {
        MicrosecondTimebase::now(self)
    }
}

/// A timer ticking at a fixed rate with its update interrupt enabled.
pub struct PeriodicTimer(Tick);

impl PeriodicTimer {
    pub fn new<T: Instance>(rcc: &mut Rcc, timer: T, rate: Hertz) -> Result<Self, ConfigError> {
        Ok(Self(Tick::new(rcc, timer, rate)?))
    }
}

impl TimerTick for PeriodicTimer {
    fn acknowledge_tick(&mut self) {
        self.0.on_interrupt();
    }
}

/// The I/O watchdog's periodic timer.
pub fn init_io_watchdog<T: Instance>(
    rcc: &mut Rcc,
    timer: T,
) -> Result<PeriodicTimer, ConfigError> {
    PeriodicTimer::new(rcc, timer, Hertz(IO_WATCHDOG_HZ))
}

/// A delay in CPU cycles, for bring-up before the tasks run. At least as long as
/// asked, and never used once they do.
pub struct CycleDelay {
    cycles_per_us: u32,
}

impl CycleDelay {
    pub const fn new(cpu_hz: u32) -> Self {
        Self {
            cycles_per_us: cpu_hz / 1_000_000,
        }
    }
}

impl embedded_hal::delay::DelayNs for CycleDelay {
    fn delay_ns(&mut self, ns: u32) {
        cortex_m::asm::delay(self.cycles_per_us.saturating_mul(ns / 1000 + 1));
    }
}
