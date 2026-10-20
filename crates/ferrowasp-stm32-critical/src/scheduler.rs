pub use crate::timer_tick::TimerTick;

pub fn acknowledge_control_tick(scheduler: &mut impl TimerTick) {
    scheduler.acknowledge_tick();
}
