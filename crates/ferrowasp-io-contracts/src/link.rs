//! The envelope every message carries and the queue every link is
//! (design §9): a producer that never blocks, and a consumer that reads in
//! order or takes the newest.
//!
//! Requirements: IO-003, IO-009, IO-010, IO-011
//! Safety-critical.

use heapless::spsc::{Consumer, Producer, Queue};

use crate::time::TimestampMicros;

/// A message with its producer's sequence and timestamp.
///
/// The producer numbers every message it tries to send, refused ones
/// included, so a gap in the sequence a consumer reads is the number of
/// messages it lost. Sequences start at 1 and skip 0 when they wrap: 0 means
/// nothing has been sent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stamped<T> {
    pub seq: u32,
    pub at: TimestampMicros,
    pub value: T,
}

/// Whether a link took a message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pushed {
    Sent,
    /// The queue was full: the message was dropped and counted, and the
    /// queued ones kept.
    Refused,
}

/// The sending end of a link, the one local resource of its producer task.
pub struct LinkProducer<'a, T> {
    queue: Producer<'a, Stamped<T>>,
    seq: u32,
    refused: u32,
}

/// The receiving end of a link, the one local resource of its consumer task.
pub struct LinkConsumer<'a, T> {
    queue: Consumer<'a, Stamped<T>>,
}

/// Split a queue, once, into its link's two ends.
pub fn link<T, const N: usize>(
    queue: &mut Queue<Stamped<T>, N>,
) -> (LinkProducer<'_, T>, LinkConsumer<'_, T>) {
    let (producer, consumer) = queue.split();
    (
        LinkProducer {
            queue: producer,
            seq: 0,
            refused: 0,
        },
        LinkConsumer { queue: consumer },
    )
}

impl<T> LinkProducer<'_, T> {
    /// Send `value`, stamped with the next sequence and `at`. Never blocks.
    pub fn push(&mut self, at: TimestampMicros, value: T) -> Pushed {
        self.seq = next_sequence(self.seq);
        self.forward(Stamped {
            seq: self.seq,
            at,
            value,
        })
    }

    /// Pass on a message stamped upstream, keeping its sequence and
    /// timestamp, so its consumer sees the original producer's gaps. Never
    /// blocks. A producer either pushes or forwards, never both.
    pub fn forward(&mut self, message: Stamped<T>) -> Pushed {
        match self.queue.enqueue(message) {
            Ok(()) => Pushed::Sent,
            Err(_) => {
                self.refused = self.refused.wrapping_add(1);
                Pushed::Refused
            }
        }
    }

    /// Messages refused since boot.
    pub fn refused(&self) -> u32 {
        self.refused
    }
}

impl<T> LinkConsumer<'_, T> {
    /// The oldest message not yet read, for an `event` link; `None` when
    /// nothing new has arrived.
    pub fn oldest(&mut self) -> Option<Stamped<T>> {
        self.queue.dequeue()
    }

    /// The newest message, for a `state` link, dropping older ones; `None`
    /// when nothing new has arrived. Takes at most the queue's capacity, so
    /// a producer that keeps sending cannot hold its consumer here.
    pub fn newest(&mut self) -> Option<Stamped<T>> {
        let mut newest = None;
        for _ in 0..self.queue.capacity() {
            match self.queue.dequeue() {
                Some(message) => newest = Some(message),
                None => break,
            }
        }
        newest
    }

    /// The most messages the link holds.
    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }
}

const fn next_sequence(seq: u32) -> u32 {
    match seq.wrapping_add(1) {
        0 => 1,
        next => next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_link_refuses_the_new_message_and_keeps_the_queued_ones() {
        let mut queue = Queue::<Stamped<u8>, 3>::new();
        let (mut producer, mut consumer) = link(&mut queue);

        assert_eq!(producer.push(TimestampMicros(10), 1), Pushed::Sent);
        assert_eq!(producer.push(TimestampMicros(20), 2), Pushed::Sent);
        assert_eq!(producer.push(TimestampMicros(30), 3), Pushed::Refused);
        assert_eq!(producer.refused(), 1);

        assert_eq!(consumer.oldest().map(|message| message.value), Some(1));
        assert_eq!(consumer.oldest().map(|message| message.value), Some(2));
        assert_eq!(consumer.oldest(), None);
    }

    #[test]
    fn a_refused_message_leaves_a_gap_in_the_sequence() {
        let mut queue = Queue::<Stamped<u8>, 2>::new();
        let (mut producer, mut consumer) = link(&mut queue);

        producer.push(TimestampMicros(10), 1);
        producer.push(TimestampMicros(20), 2);
        let first = consumer.oldest().unwrap();
        producer.push(TimestampMicros(30), 3);
        let after_gap = consumer.oldest().unwrap();

        assert_eq!(first.seq, 1);
        assert_eq!(after_gap.seq, 3);
        assert_eq!(after_gap.at, TimestampMicros(30));
    }

    #[test]
    fn newest_drains_to_the_latest_and_empty_means_nothing_new() {
        let mut queue = Queue::<Stamped<u8>, 4>::new();
        let (mut producer, mut consumer) = link(&mut queue);

        assert_eq!(consumer.newest(), None);
        producer.push(TimestampMicros(10), 1);
        producer.push(TimestampMicros(20), 2);
        producer.push(TimestampMicros(30), 3);

        assert_eq!(consumer.newest().map(|message| message.value), Some(3));
        assert_eq!(consumer.newest(), None);
        assert_eq!(consumer.capacity(), 3);
    }

    #[test]
    fn forward_keeps_the_upstream_stamp() {
        let mut queue = Queue::<Stamped<u8>, 2>::new();
        let (mut producer, mut consumer) = link(&mut queue);
        let upstream = Stamped {
            seq: 41,
            at: TimestampMicros(7),
            value: 9,
        };

        assert_eq!(producer.forward(upstream), Pushed::Sent);
        assert_eq!(consumer.oldest(), Some(upstream));
    }

    #[test]
    fn the_sequence_skips_zero_when_it_wraps() {
        assert_eq!(next_sequence(0), 1);
        assert_eq!(next_sequence(u32::MAX), 1);
    }
}
