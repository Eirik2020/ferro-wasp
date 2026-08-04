//! Non-authoritative, non-consuming latest-value observation.

use core::cell::RefCell;

use critical_section::Mutex;

/// Private storage for the latest value published to observers.
///
/// Splitting requires exclusive access and produces exactly one publisher and
/// one reader handle. The handles do not implement [`Clone`] or [`Copy`].
pub struct ObserverChannel<T: Copy> {
    latest: Mutex<RefCell<Option<T>>>,
}

impl<T: Copy> ObserverChannel<T> {
    /// Creates an observer channel with no published value.
    pub const fn new() -> Self {
        Self {
            latest: Mutex::new(RefCell::new(None)),
        }
    }

    /// Splits this storage into its sole publisher and reader handles.
    pub fn split(&mut self) -> (ObserverPublisher<'_, T>, ObserverReader<'_, T>) {
        let latest = &self.latest;
        (ObserverPublisher { latest }, ObserverReader { latest })
    }
}

impl<T: Copy> Default for ObserverChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Exclusive authority to replace an observer channel's latest value.
pub struct ObserverPublisher<'a, T: Copy> {
    latest: &'a Mutex<RefCell<Option<T>>>,
}

impl<T: Copy> ObserverPublisher<'_, T> {
    /// Atomically replaces the previously published observer value.
    pub fn publish(&mut self, value: T) {
        critical_section::with(|cs| {
            *self.latest.borrow_ref_mut(cs) = Some(value);
        });
    }
}

/// Non-consuming access to the latest observer value.
pub struct ObserverReader<'a, T: Copy> {
    latest: &'a Mutex<RefCell<Option<T>>>,
}

impl<T: Copy> ObserverReader<'_, T> {
    /// Copies the latest publication without removing or modifying it.
    pub fn latest(&self) -> Option<T> {
        critical_section::with(|cs| *self.latest.borrow_ref(cs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_is_empty_before_first_publication() {
        let mut channel = ObserverChannel::<u32>::new();
        let (_publisher, reader) = channel.split();

        assert_eq!(reader.latest(), None);
    }

    #[test]
    fn reads_copy_without_consuming_latest_value() {
        let mut channel = ObserverChannel::new();
        let (mut publisher, reader) = channel.split();

        publisher.publish(42_u32);

        assert_eq!(reader.latest(), Some(42));
        assert_eq!(reader.latest(), Some(42));
    }

    #[test]
    fn publication_replaces_previous_value() {
        let mut channel = ObserverChannel::new();
        let (mut publisher, reader) = channel.split();

        publisher.publish(1_u32);
        publisher.publish(2_u32);

        assert_eq!(reader.latest(), Some(2));
    }

    #[test]
    fn one_reader_handle_can_serve_multiple_non_consuming_observers() {
        fn observe(reader: &ObserverReader<'_, u32>) -> Option<u32> {
            reader.latest()
        }

        let mut channel = ObserverChannel::new();
        let (mut publisher, reader) = channel.split();
        publisher.publish(7_u32);

        assert_eq!(observe(&reader), Some(7));
        assert_eq!(observe(&reader), Some(7));
    }
}
