//! What the observer publishes on the ledger (ADR-0056), and how a reader
//! tells that a topic has stopped advancing (section 19).
//!
//! A topic whose value is its message's is published as that message, with
//! its producer's sequence and timestamp. A topic the observer derives has
//! its own type here, so a new derived value changes only the observer and
//! its readers, never a producer.

use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::{Deadline, TimestampMicros};

/// Facts fixed at boot, published once.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Identity {
    pub imu_ready: bool,
    /// The detected IMU, as the board numbers its kinds; 0 for none.
    pub imu_kind: u8,
    /// The log flash's JEDEC manufacturer, memory type and capacity code.
    pub flash_jedec: [u8; 3],
    pub flash_capacity_bytes: u32,
}

/// The observer's own state, stamped with its run count, so a reader sees
/// the observer itself stop.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObserverStatus {
    /// Control ticks missing from the sequence the observer read: refused by
    /// a full control link.
    pub control_ticks_lost: u32,
    /// Control ticks the blackbox stream refused.
    pub stream_refused: u32,
}

/// A reader's watch on one topic.
#[derive(Clone, Copy, Debug, Default)]
pub struct TopicWatch {
    seq: u32,
    changed_at: Option<TimestampMicros>,
}

impl TopicWatch {
    pub const fn new() -> Self {
        Self {
            seq: 0,
            changed_at: None,
        }
    }

    /// Note `topic`'s sequence at `now`, by the reader's clock. True while
    /// it has changed within the last `limit_us`. The first look counts as
    /// a change, so a topic never published reads as stalled once the limit
    /// has passed.
    pub fn advancing<T>(
        &mut self,
        topic: &Stamped<T>,
        now: TimestampMicros,
        limit_us: u32,
    ) -> bool {
        let changed_at = match self.changed_at {
            Some(changed_at) if topic.seq == self.seq => changed_at,
            _ => {
                self.seq = topic.seq;
                self.changed_at = Some(now);
                now
            }
        };
        !Deadline::new(changed_at, limit_us).is_expired_at(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic(seq: u32) -> Stamped<()> {
        Stamped {
            seq,
            at: TimestampMicros(0),
            value: (),
        }
    }

    #[test]
    fn a_topic_that_keeps_advancing_is_not_stalled() {
        let mut watch = TopicWatch::new();

        assert!(watch.advancing(&topic(1), TimestampMicros(0), 500));
        assert!(watch.advancing(&topic(2), TimestampMicros(400), 500));
        assert!(watch.advancing(&topic(3), TimestampMicros(800), 500));
    }

    #[test]
    fn a_topic_stalls_once_its_sequence_holds_past_the_limit() {
        let mut watch = TopicWatch::new();

        assert!(watch.advancing(&topic(7), TimestampMicros(1_000), 500));
        assert!(watch.advancing(&topic(7), TimestampMicros(1_499), 500));
        assert!(!watch.advancing(&topic(7), TimestampMicros(1_500), 500));
        assert!(watch.advancing(&topic(8), TimestampMicros(1_600), 500));
    }

    #[test]
    fn a_topic_never_published_stalls_after_the_limit() {
        let mut watch = TopicWatch::new();

        assert!(watch.advancing(&topic(0), TimestampMicros(10_000), 500));
        assert!(!watch.advancing(&topic(0), TimestampMicros(10_500), 500));
    }
}
