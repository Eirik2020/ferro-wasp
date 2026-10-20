use core::{
    cell::RefCell,
    future::poll_fn,
    task::{Context, Poll, Waker},
};

use critical_section::Mutex;
use embedded_io::ErrorType;
use embedded_io_async::Write;
use heapless::Deque;

use super::{SerialFault, TxChunk};

struct SerialTxMailbox<const N: usize, const DEPTH: usize> {
    chunks: Deque<TxChunk<N>, DEPTH>,
    capacity_waiter: Option<Waker>,
    owner_waiter: Option<Waker>,
    completion_waiter: Option<Waker>,
    flush_waiter: Option<Waker>,
    enabled: bool,
    in_flight: bool,
    completion: Option<Result<(), SerialFault>>,
    fault: Option<SerialFault>,
    completed_chunks: u32,
}

impl<const N: usize, const DEPTH: usize> SerialTxMailbox<N, DEPTH> {
    const fn new() -> Self {
        Self {
            chunks: Deque::new(),
            capacity_waiter: None,
            owner_waiter: None,
            completion_waiter: None,
            flush_waiter: None,
            enabled: true,
            in_flight: false,
            completion: None,
            fault: None,
            completed_chunks: 0,
        }
    }

    fn terminal_error(&self) -> Option<SerialFault> {
        self.fault
            .or((!self.enabled).then_some(SerialFault::Disabled))
    }

    fn register(waiter: &mut Option<Waker>, cx: &Context<'_>) {
        if waiter
            .as_ref()
            .is_none_or(|waiter| !waiter.will_wake(cx.waker()))
        {
            *waiter = Some(cx.waker().clone());
        }
    }

    fn clear_chunks(&mut self) {
        while self.chunks.pop_front().is_some() {}
    }
}

type SharedSerialTxMailbox<const N: usize, const DEPTH: usize> =
    Mutex<RefCell<SerialTxMailbox<N, DEPTH>>>;

/// Static owner for one bounded serial transmit stream.
pub struct SerialTxChannel<const N: usize, const DEPTH: usize> {
    mailbox: SharedSerialTxMailbox<N, DEPTH>,
}

impl<const N: usize, const DEPTH: usize> SerialTxChannel<N, DEPTH> {
    pub const fn new() -> Self {
        const {
            assert!(N > 0);
        }
        Self {
            mailbox: Mutex::new(RefCell::new(SerialTxMailbox::new())),
        }
    }

    pub fn split(
        &mut self,
    ) -> (
        SerialWriter<'_, N, DEPTH>,
        SerialTxOwner<'_, N, DEPTH>,
        SerialTxCompletion<'_, N, DEPTH>,
    ) {
        let mailbox = &self.mailbox;
        (
            SerialWriter { mailbox },
            SerialTxOwner { mailbox },
            SerialTxCompletion { mailbox },
        )
    }
}

impl<const N: usize, const DEPTH: usize> Default for SerialTxChannel<N, DEPTH> {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard async byte writer backed by bounded owned chunks.
///
/// A pending `write` has not enqueued any bytes, so dropping that future is
/// side-effect-free. `write_all` follows the trait's standard partial-progress
/// cancellation semantics.
pub struct SerialWriter<'a, const N: usize, const DEPTH: usize> {
    mailbox: &'a SharedSerialTxMailbox<N, DEPTH>,
}

impl<const N: usize, const DEPTH: usize> ErrorType for SerialWriter<'_, N, DEPTH> {
    type Error = SerialFault;
}

impl<const N: usize, const DEPTH: usize> Write for SerialWriter<'_, N, DEPTH> {
    async fn write(&mut self, bytes: &[u8]) -> Result<usize, Self::Error> {
        if bytes.is_empty() {
            return Ok(0);
        }

        let count = bytes.len().min(N);
        let mut chunk =
            Some(TxChunk::from_slice(&bytes[..count]).map_err(|_| SerialFault::InvalidChunk)?);

        poll_fn(|cx| {
            let (poll, owner_waiter) = critical_section::with(|cs| {
                let mut mailbox = self.mailbox.borrow_ref_mut(cs);
                if let Some(error) = mailbox.terminal_error() {
                    return (Poll::Ready(Err(error)), None);
                }

                if mailbox.chunks.is_full() {
                    SerialTxMailbox::<N, DEPTH>::register(&mut mailbox.capacity_waiter, cx);
                    return (Poll::Pending, None);
                }

                let Some(chunk) = chunk.take() else {
                    return (Poll::Ready(Err(SerialFault::InvalidState)), None);
                };
                if mailbox.chunks.push_back(chunk).is_err() {
                    return (Poll::Ready(Err(SerialFault::QueueOverflow)), None);
                }
                (Poll::Ready(Ok(count)), mailbox.owner_waiter.take())
            });

            if let Some(waiter) = owner_waiter {
                waiter.wake();
            }
            poll
        })
        .await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        poll_fn(|cx| {
            critical_section::with(|cs| {
                let mut mailbox = self.mailbox.borrow_ref_mut(cs);
                if let Some(error) = mailbox.terminal_error() {
                    return Poll::Ready(Err(error));
                }
                if mailbox.chunks.is_empty() && !mailbox.in_flight {
                    return Poll::Ready(Ok(()));
                }

                SerialTxMailbox::<N, DEPTH>::register(&mut mailbox.flush_waiter, cx);
                Poll::Pending
            })
        })
        .await
    }
}

/// Peripheral-owner side of a serial TX channel.
///
/// The owner awaits one chunk, starts DMA using only that owned chunk, then
/// awaits the result published by the IRQ-side completion handle.
pub struct SerialTxOwner<'a, const N: usize, const DEPTH: usize> {
    mailbox: &'a SharedSerialTxMailbox<N, DEPTH>,
}

impl<const N: usize, const DEPTH: usize> SerialTxOwner<'_, N, DEPTH> {
    pub async fn next_chunk(&mut self) -> Result<TxChunk<N>, SerialFault> {
        poll_fn(|cx| {
            let (poll, capacity_waiter) = critical_section::with(|cs| {
                let mut mailbox = self.mailbox.borrow_ref_mut(cs);
                if let Some(error) = mailbox.terminal_error() {
                    return (Poll::Ready(Err(error)), None);
                }
                if mailbox.in_flight {
                    return (Poll::Ready(Err(SerialFault::InvalidState)), None);
                }
                if mailbox.completion.is_some() {
                    return (Poll::Ready(Err(SerialFault::InvalidState)), None);
                }
                if let Some(chunk) = mailbox.chunks.pop_front() {
                    mailbox.in_flight = true;
                    mailbox.completion = None;
                    return (Poll::Ready(Ok(chunk)), mailbox.capacity_waiter.take());
                }

                SerialTxMailbox::<N, DEPTH>::register(&mut mailbox.owner_waiter, cx);
                (Poll::Pending, None)
            });

            if let Some(waiter) = capacity_waiter {
                waiter.wake();
            }
            poll
        })
        .await
    }

    pub async fn wait_completion(&mut self) -> Result<(), SerialFault> {
        poll_fn(|cx| {
            critical_section::with(|cs| {
                let mut mailbox = self.mailbox.borrow_ref_mut(cs);
                if let Some(completion) = mailbox.completion.take() {
                    return Poll::Ready(completion);
                }
                if let Some(error) = mailbox.terminal_error() {
                    return Poll::Ready(Err(error));
                }
                if !mailbox.in_flight {
                    return Poll::Ready(Err(SerialFault::InvalidState));
                }

                SerialTxMailbox::<N, DEPTH>::register(&mut mailbox.completion_waiter, cx);
                Poll::Pending
            })
        })
        .await
    }

    pub fn fail(&mut self, fault: SerialFault) {
        let waiters =
            critical_section::with(|cs| fail_mailbox(&mut self.mailbox.borrow_ref_mut(cs), fault));
        wake_all(waiters);
    }
}

/// IRQ-side completion handle for one serialized TX owner.
pub struct SerialTxCompletion<'a, const N: usize, const DEPTH: usize> {
    mailbox: &'a SharedSerialTxMailbox<N, DEPTH>,
}

impl<const N: usize, const DEPTH: usize> SerialTxCompletion<'_, N, DEPTH> {
    pub fn complete(&mut self) -> Result<(), SerialFault> {
        let waiters = critical_section::with(|cs| {
            let mut mailbox = self.mailbox.borrow_ref_mut(cs);
            if !mailbox.in_flight || mailbox.completion.is_some() {
                return Err(SerialFault::InvalidState);
            }

            mailbox.in_flight = false;
            mailbox.completion = Some(Ok(()));
            mailbox.completed_chunks = mailbox.completed_chunks.saturating_add(1);
            Ok((
                mailbox.completion_waiter.take(),
                (mailbox.chunks.is_empty())
                    .then(|| mailbox.flush_waiter.take())
                    .flatten(),
            ))
        })?;

        if let Some(waiter) = waiters.0 {
            waiter.wake();
        }
        if let Some(waiter) = waiters.1 {
            waiter.wake();
        }
        Ok(())
    }

    pub fn fail(&mut self, fault: SerialFault) {
        let waiters =
            critical_section::with(|cs| fail_mailbox(&mut self.mailbox.borrow_ref_mut(cs), fault));
        wake_all(waiters);
    }

    pub fn disable(&mut self) {
        let waiters = critical_section::with(|cs| {
            let mut mailbox = self.mailbox.borrow_ref_mut(cs);
            let had_in_flight = mailbox.in_flight;
            mailbox.enabled = false;
            mailbox.in_flight = false;
            mailbox.completion = had_in_flight.then_some(Err(SerialFault::Disabled));
            mailbox.clear_chunks();
            (
                mailbox.capacity_waiter.take(),
                mailbox.owner_waiter.take(),
                mailbox.completion_waiter.take(),
                mailbox.flush_waiter.take(),
            )
        });
        wake_all(waiters);
    }

    pub fn recover(&mut self) {
        critical_section::with(|cs| {
            let mut mailbox = self.mailbox.borrow_ref_mut(cs);
            mailbox.fault = None;
            mailbox.enabled = true;
            mailbox.completion = None;
        });
    }

    pub fn status(&self) -> SerialTxStatus {
        critical_section::with(|cs| {
            let mailbox = self.mailbox.borrow_ref(cs);
            SerialTxStatus {
                enabled: mailbox.enabled,
                pending_chunks: mailbox.chunks.len(),
                in_flight: mailbox.in_flight,
                fault: mailbox.fault,
                completed_chunks: mailbox.completed_chunks,
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialTxStatus {
    pub enabled: bool,
    pub pending_chunks: usize,
    pub in_flight: bool,
    pub fault: Option<SerialFault>,
    pub completed_chunks: u32,
}

fn fail_mailbox<const N: usize, const DEPTH: usize>(
    mailbox: &mut SerialTxMailbox<N, DEPTH>,
    fault: SerialFault,
) -> (Option<Waker>, Option<Waker>, Option<Waker>, Option<Waker>) {
    let had_in_flight = mailbox.in_flight;
    mailbox.fault = Some(fault);
    mailbox.in_flight = false;
    mailbox.completion = had_in_flight.then_some(Err(fault));
    mailbox.clear_chunks();
    (
        mailbox.capacity_waiter.take(),
        mailbox.owner_waiter.take(),
        mailbox.completion_waiter.take(),
        mailbox.flush_waiter.take(),
    )
}

fn wake_all(waiters: (Option<Waker>, Option<Waker>, Option<Waker>, Option<Waker>)) {
    if let Some(waiter) = waiters.0 {
        waiter.wake();
    }
    if let Some(waiter) = waiters.1 {
        waiter.wake();
    }
    if let Some(waiter) = waiters.2 {
        waiter.wake();
    }
    if let Some(waiter) = waiters.3 {
        waiter.wake();
    }
}

#[cfg(test)]
mod tests {
    use core::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll, Waker},
    };
    use std::{boxed::Box, sync::Arc, task::Wake};

    use super::*;

    struct CountWake(AtomicUsize);

    impl Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn poll_once<F: Future>(future: Pin<&mut F>, waker: &Waker) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(waker))
    }

    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        match poll_once(future.as_mut(), Waker::noop()) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("future unexpectedly pending"),
        }
    }

    #[test]
    fn write_copies_at_most_one_owned_chunk() {
        let mut channel = SerialTxChannel::<4, 2>::new();
        let (mut writer, mut owner, mut completion) = channel.split();

        assert_eq!(ready(writer.write(b"abcdef")), Ok(4));
        let chunk = ready(owner.next_chunk()).unwrap();
        assert_eq!(chunk.as_slice(), Ok(b"abcd".as_slice()));
        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(ready(owner.wait_completion()), Ok(()));
    }

    #[test]
    fn write_all_splits_bytes_into_ordered_bounded_chunks() {
        let mut channel = SerialTxChannel::<4, 2>::new();
        let (mut writer, mut owner, mut completion) = channel.split();

        assert_eq!(ready(writer.write_all(b"abcdef")), Ok(()));
        let first = ready(owner.next_chunk()).unwrap();
        assert_eq!(first.as_slice(), Ok(b"abcd".as_slice()));
        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(ready(owner.wait_completion()), Ok(()));
        let second = ready(owner.next_chunk()).unwrap();
        assert_eq!(second.as_slice(), Ok(b"ef".as_slice()));
        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(ready(owner.wait_completion()), Ok(()));
    }

    #[test]
    fn full_queue_write_waits_for_owner_capacity() {
        let mut channel = SerialTxChannel::<4, 1>::new();
        let (mut writer, mut owner, mut completion) = channel.split();
        assert_eq!(ready(writer.write(b"one")), Ok(3));

        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut pending = Box::pin(writer.write(b"two"));
        assert_eq!(poll_once(pending.as_mut(), &waker), Poll::Pending);

        let first = ready(owner.next_chunk()).unwrap();
        assert_eq!(first.as_slice(), Ok(b"one".as_slice()));
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(poll_once(pending.as_mut(), &waker), Poll::Ready(Ok(3)));
        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(ready(owner.wait_completion()), Ok(()));

        let second = ready(owner.next_chunk()).unwrap();
        assert_eq!(second.as_slice(), Ok(b"two".as_slice()));
        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(ready(owner.wait_completion()), Ok(()));
    }

    #[test]
    fn flush_waits_for_queue_and_hardware_completion() {
        let mut channel = SerialTxChannel::<8, 2>::new();
        let (mut writer, mut owner, mut completion) = channel.split();
        assert_eq!(ready(writer.write(b"frame")), Ok(5));

        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut flush = Box::pin(writer.flush());
        assert_eq!(poll_once(flush.as_mut(), &waker), Poll::Pending);

        let frame = ready(owner.next_chunk()).unwrap();
        assert_eq!(frame.as_slice(), Ok(b"frame".as_slice()));
        assert_eq!(poll_once(flush.as_mut(), &waker), Poll::Pending);
        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(poll_once(flush.as_mut(), &waker), Poll::Ready(Ok(())));
        assert_eq!(ready(owner.wait_completion()), Ok(()));
    }

    #[test]
    fn irq_completion_wakes_owner_waiting_for_hardware() {
        let mut channel = SerialTxChannel::<8, 2>::new();
        let (mut writer, mut owner, mut completion) = channel.split();
        assert_eq!(ready(writer.write(b"frame")), Ok(5));
        let _frame = ready(owner.next_chunk()).unwrap();

        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut wait_completion = Box::pin(owner.wait_completion());
        assert_eq!(poll_once(wait_completion.as_mut(), &waker), Poll::Pending);

        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(
            poll_once(wait_completion.as_mut(), &waker),
            Poll::Ready(Ok(()))
        );
    }

    #[test]
    fn dropping_pending_write_does_not_enqueue_its_bytes() {
        let mut channel = SerialTxChannel::<8, 1>::new();
        let (mut writer, mut owner, mut completion) = channel.split();
        assert_eq!(ready(writer.write(b"first")), Ok(5));

        let mut pending = Box::pin(writer.write(b"cancelled"));
        assert_eq!(poll_once(pending.as_mut(), Waker::noop()), Poll::Pending);
        drop(pending);

        let first = ready(owner.next_chunk()).unwrap();
        assert_eq!(first.as_slice(), Ok(b"first".as_slice()));
        assert_eq!(completion.complete(), Ok(()));
        assert_eq!(ready(owner.wait_completion()), Ok(()));
        assert_eq!(completion.status().pending_chunks, 0);
    }

    #[test]
    fn owner_fault_wakes_flush_and_clears_queued_chunks() {
        let mut channel = SerialTxChannel::<8, 2>::new();
        let (mut writer, mut owner, completion) = channel.split();
        assert_eq!(ready(writer.write(b"frame")), Ok(5));

        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut flush = Box::pin(writer.flush());
        assert_eq!(poll_once(flush.as_mut(), &waker), Poll::Pending);

        owner.fail(SerialFault::DmaTransfer);
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(
            poll_once(flush.as_mut(), &waker),
            Poll::Ready(Err(SerialFault::DmaTransfer))
        );
        assert_eq!(completion.status().pending_chunks, 0);
    }

    #[test]
    fn owner_waiter_is_woken_when_writer_enqueues() {
        let mut channel = SerialTxChannel::<8, 2>::new();
        let (mut writer, mut owner, _completion) = channel.split();
        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut next = Box::pin(owner.next_chunk());
        assert_eq!(poll_once(next.as_mut(), &waker), Poll::Pending);

        assert_eq!(ready(writer.write(b"ready")), Ok(5));
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        let chunk = match poll_once(next.as_mut(), &waker) {
            Poll::Ready(Ok(chunk)) => chunk,
            result => panic!("unexpected owner poll: {result:?}"),
        };
        assert_eq!(chunk.as_slice(), Ok(b"ready".as_slice()));
    }

    #[test]
    fn empty_write_is_immediately_successful() {
        let mut channel = SerialTxChannel::<8, 2>::new();
        let (mut writer, _owner, completion) = channel.split();

        assert_eq!(ready(writer.write(&[])), Ok(0));
        assert_eq!(completion.status().pending_chunks, 0);
    }

    #[test]
    fn serial_writer_implements_workspace_async_write() {
        fn assert_async_write<T: embedded_io_async::Write>() {}
        assert_async_write::<SerialWriter<'static, 8, 2>>();
    }
}
