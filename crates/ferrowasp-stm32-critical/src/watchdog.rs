pub const IO_WATCHDOG_HZ: u32 = 8_000;

pub use crate::timer_tick::TimerTick;

pub fn acknowledge_watchdog_tick(watchdog: &mut impl TimerTick) {
    watchdog.acknowledge_tick();
}
