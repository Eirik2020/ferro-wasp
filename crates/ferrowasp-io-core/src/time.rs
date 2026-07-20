#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct TimestampMicros(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Deadline {
    pub start: TimestampMicros,
    pub timeout_us: u32,
}

impl Deadline {
    pub const fn new(start: TimestampMicros, timeout_us: u32) -> Self {
        Self { start, timeout_us }
    }

    pub const fn expires_at(self) -> TimestampMicros {
        TimestampMicros(self.start.0.saturating_add(self.timeout_us as u64))
    }

    pub const fn is_expired_at(self, now: TimestampMicros) -> bool {
        now.0 >= self.expires_at().0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WrappingCounterExtender {
    period_ticks: u32,
    last_ticks: Option<u32>,
    elapsed_ticks: u64,
}

impl WrappingCounterExtender {
    pub const fn new(period_ticks: u32) -> Self {
        assert!(period_ticks > 0);
        Self {
            period_ticks,
            last_ticks: None,
            elapsed_ticks: 0,
        }
    }

    pub fn observe(&mut self, ticks: u32) -> u64 {
        let Some(last_ticks) = self.last_ticks else {
            self.last_ticks = Some(ticks);
            self.elapsed_ticks = u64::from(ticks);
            return self.elapsed_ticks;
        };

        let delta = if ticks >= last_ticks {
            ticks - last_ticks
        } else {
            self.period_ticks - last_ticks + ticks
        };

        self.last_ticks = Some(ticks);
        self.elapsed_ticks = self.elapsed_ticks.saturating_add(u64::from(delta));
        self.elapsed_ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_counter_extends_monotonic_ticks() {
        let mut extender = WrappingCounterExtender::new(100);

        assert_eq!(extender.observe(90), 90);
        assert_eq!(extender.observe(99), 99);
        assert_eq!(extender.observe(0), 100);
        assert_eq!(extender.observe(5), 105);
    }

    #[test]
    fn wrapping_counter_tracks_large_32_bit_period() {
        let mut extender = WrappingCounterExtender::new(u32::MAX);

        assert_eq!(extender.observe(u32::MAX - 2), u64::from(u32::MAX - 2));
        assert_eq!(extender.observe(1), u64::from(u32::MAX) + 1);
    }
}
