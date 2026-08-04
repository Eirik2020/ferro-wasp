//! Exclusive, bounded transfer of authoritative safety-path data.

use heapless::spsc::{Consumer, Producer, Queue};

/// Private storage for one bounded single-producer, single-consumer channel.
///
/// `N` is the backing queue length used by [`heapless`]. The usable message
/// capacity is `N - 1`, so `N` must be at least two.
pub struct SafetyChannel<T, const N: usize> {
    queue: Queue<T, N>,
}

impl<T, const N: usize> SafetyChannel<T, N> {
    /// Creates an empty safety channel.
    pub const fn new() -> Self {
        assert!(N >= 2, "a safety channel requires at least one usable slot");
        Self {
            queue: Queue::new(),
        }
    }

    /// Splits this storage into its sole producer and consumer handles.
    pub fn split(&mut self) -> (SafetyProducer<'_, T>, SafetyConsumer<'_, T>) {
        let (producer, consumer) = self.queue.split();
        (SafetyProducer { producer }, SafetyConsumer { consumer })
    }

    /// Returns the maximum number of messages the channel can hold.
    pub const fn capacity(&self) -> usize {
        N - 1
    }
}

impl<T, const N: usize> Default for SafetyChannel<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Exclusive authority to append values to a safety channel.
///
/// This handle intentionally does not implement [`Clone`] or [`Copy`].
pub struct SafetyProducer<'a, T> {
    producer: Producer<'a, T>,
}

impl<T> SafetyProducer<'_, T> {
    /// Attempts to append one value without blocking.
    ///
    /// The original value is returned when the bounded channel is full. No
    /// value is overwritten or silently discarded.
    pub fn try_send(&mut self, value: T) -> Result<(), T> {
        self.producer.enqueue(value)
    }
}

/// Exclusive authority to consume values from a safety channel.
///
/// This handle intentionally does not implement [`Clone`] or [`Copy`].
pub struct SafetyConsumer<'a, T> {
    consumer: Consumer<'a, T>,
}

impl<T> SafetyConsumer<'_, T> {
    /// Removes and returns the oldest queued value, or `None` when empty.
    pub fn try_receive(&mut self) -> Option<T> {
        self.consumer.dequeue()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_consumed_in_fifo_order() {
        let mut channel = SafetyChannel::<u32, 4>::new();
        let (mut producer, mut consumer) = channel.split();

        producer.try_send(1).unwrap();
        producer.try_send(2).unwrap();

        assert_eq!(consumer.try_receive(), Some(1));
        assert_eq!(consumer.try_receive(), Some(2));
        assert_eq!(consumer.try_receive(), None);
    }

    #[test]
    fn receiving_consumes_each_message_once() {
        let mut channel = SafetyChannel::<u32, 2>::new();
        let (mut producer, mut consumer) = channel.split();

        producer.try_send(42).unwrap();

        assert_eq!(consumer.try_receive(), Some(42));
        assert_eq!(consumer.try_receive(), None);
    }

    #[test]
    fn full_channel_returns_the_unsent_value() {
        let mut channel = SafetyChannel::<u32, 3>::new();
        let (mut producer, mut consumer) = channel.split();

        producer.try_send(1).unwrap();
        producer.try_send(2).unwrap();

        assert_eq!(producer.try_send(3), Err(3));
        assert_eq!(consumer.try_receive(), Some(1));
        assert_eq!(consumer.try_receive(), Some(2));
    }

    #[test]
    fn reported_capacity_matches_the_usable_queue_capacity() {
        let channel = SafetyChannel::<u32, 5>::new();

        assert_eq!(channel.capacity(), 4);
    }
}
