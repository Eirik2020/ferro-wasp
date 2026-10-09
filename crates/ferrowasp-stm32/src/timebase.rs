use ferrowasp_io_core::time::TimestampMicros;

pub const MICROSECOND_TIMEBASE_HZ: u32 = 1_000_000;
pub const MICROSECOND_TIMEBASE_PERIOD_TICKS: u32 = u32::MAX;

/// A microsecond clock, whichever timer a board drives it from. What a shared
/// task definition bounds on, so a board's timer choice stays in the board.
pub trait Timebase {
    fn now(&mut self) -> TimestampMicros;
}
