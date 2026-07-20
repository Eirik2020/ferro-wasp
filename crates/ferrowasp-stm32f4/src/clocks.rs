pub const SYSTEM_CLOCK_HZ: u32 = 168_000_000;
pub const CONTROL_SCHEDULER_HZ: u32 = 800;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockPlan {
    pub system_hz: u32,
    pub control_scheduler_hz: u32,
    pub requires_pll48: bool,
}

pub const DEFAULT_CLOCK_PLAN: ClockPlan = ClockPlan {
    system_hz: SYSTEM_CLOCK_HZ,
    control_scheduler_hz: CONTROL_SCHEDULER_HZ,
    requires_pll48: false,
};
