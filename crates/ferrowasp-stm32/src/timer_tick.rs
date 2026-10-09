//! A periodic timer whose update tick a handler acknowledges by clearing its
//! flags - the control scheduler and the I/O watchdog alike.

/// Acknowledge one tick, whichever timer a board uses. What a shared task
/// definition bounds on, so a board's timer choice stays in the board.
pub trait TimerTick {
    fn acknowledge_tick(&mut self);
}
