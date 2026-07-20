use core::{
    cell::RefCell,
    future::poll_fn,
    task::{Context, Poll, Waker},
};

use critical_section::Mutex;
use embedded_io::ErrorType;
use embedded_io_async::Read;
use heapless::Deque;

use super::{Discontinuity, DiscontinuityRecord, RxChunk, SerialFault, StreamGeneration};
use crate::time::TimestampMicros;

struct SerialRxMailbox<const N: usize, const DEPTH: usize> {
    chunks: Deque<RxChunk<N>, DEPTH>,
    waiter: Option<Waker>,
    enabled: bool,
    discontinuity_pending: bool,
    discontinuity_count: u32,
    overflow_count: u32,
    latest_discontinuity: Option<DiscontinuityRecord>,
}

impl<const N: usize, const DEPTH: usize> SerialRxMailbox<N, DEPTH> {
    const fn new() -> Self {
        Self {
            chunks: Deque::new(),
            waiter: None,
            enabled: true,
            discontinuity_pending: false,
            discontinuity_count: 0,
            overflow_count: 0,
            latest_discontinuity: None,
        }
    }

    fn record_discontinuity(
        &mut self,
        cause: Discontinuity,
        generation: StreamGeneration,
        observed_at: TimestampMicros,
    ) {
        self.discontinuity_count = self.discontinuity_count.wrapping_add(1);
        self.discontinuity_pending = true;
        self.latest_discontinuity = Some(DiscontinuityRecord {
            sequence: self.discontinuity_count,
            cause,
            generation,
            observed_at,
        });
    }

    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Result<RxChunk<N>, SerialFault>> {
        if let Some(chunk) = self.chunks.pop_front() {
            return Poll::Ready(Ok(chunk));
        }
        if !self.enabled {
            return Poll::Ready(Err(SerialFault::Disabled));
        }
        if self
            .waiter
            .as_ref()
            .is_none_or(|waiter| !waiter.will_wake(cx.waker()))
        {
            self.waiter = Some(cx.waker().clone());
        }
        Poll::Pending
    }
}

type SharedSerialRxMailbox<const N: usize, const DEPTH: usize> =
    Mutex<RefCell<SerialRxMailbox<N, DEPTH>>>;

/// Static owner for one bounded serial receive stream.
///
/// Calling `split` requires unique access, so one channel produces one
/// producer, one byte reader, and one discontinuity observer.
pub struct SerialRxChannel<const N: usize, const DEPTH: usize> {
    mailbox: SharedSerialRxMailbox<N, DEPTH>,
}

impl<const N: usize, const DEPTH: usize> SerialRxChannel<N, DEPTH> {
    pub const fn new() -> Self {
        Self {
            mailbox: Mutex::new(RefCell::new(SerialRxMailbox::new())),
        }
    }

    pub fn split(
        &mut self,
    ) -> (
        SerialRxProducer<'_, N, DEPTH>,
        SerialReader<'_, N, DEPTH>,
        DiscontinuityReader<'_, N, DEPTH>,
    ) {
        let mailbox = &self.mailbox;
        (
            SerialRxProducer { mailbox },
            SerialReader {
                mailbox,
                current: None,
                offset: 0,
            },
            DiscontinuityReader {
                mailbox,
                observed_sequence: 0,
            },
        )
    }
}

impl<const N: usize, const DEPTH: usize> Default for SerialRxChannel<N, DEPTH> {
    fn default() -> Self {
        Self::new()
    }
}

/// The single producer side used by a UART backend after DMA storage detaches.
pub struct SerialRxProducer<'a, const N: usize, const DEPTH: usize> {
    mailbox: &'a SharedSerialRxMailbox<N, DEPTH>,
}

impl<const N: usize, const DEPTH: usize> SerialRxProducer<'_, N, DEPTH> {
    pub fn try_send(&mut self, mut chunk: RxChunk<N>) -> Result<(), SerialFault> {
        let (result, waiter) = critical_section::with(|cs| {
            let mut mailbox = self.mailbox.borrow_ref_mut(cs);
            if !mailbox.enabled {
                return (Err(SerialFault::Disabled), None);
            }

            chunk.discontinuity_before |= mailbox.discontinuity_pending;
            let generation = chunk.generation;
            let timestamp = chunk.timestamp;
            match mailbox.chunks.push_back(chunk) {
                Ok(()) => {
                    mailbox.discontinuity_pending = false;
                    (Ok(()), mailbox.waiter.take())
                }
                Err(_) => {
                    mailbox.overflow_count = mailbox.overflow_count.saturating_add(1);
                    mailbox.record_discontinuity(
                        Discontinuity::QueueOverflow,
                        generation,
                        timestamp,
                    );
                    (Err(SerialFault::QueueOverflow), None)
                }
            }
        });

        if let Some(waiter) = waiter {
            waiter.wake();
        }
        result
    }

    pub fn record_discontinuity(
        &mut self,
        cause: Discontinuity,
        generation: StreamGeneration,
        observed_at: TimestampMicros,
    ) {
        critical_section::with(|cs| {
            self.mailbox
                .borrow_ref_mut(cs)
                .record_discontinuity(cause, generation, observed_at);
        });
    }

    pub fn disable(&mut self) {
        let waiter = critical_section::with(|cs| {
            let mut mailbox = self.mailbox.borrow_ref_mut(cs);
            mailbox.enabled = false;
            mailbox.waiter.take()
        });
        if let Some(waiter) = waiter {
            waiter.wake();
        }
    }

    pub fn enable(&mut self) {
        critical_section::with(|cs| {
            self.mailbox.borrow_ref_mut(cs).enabled = true;
        });
    }
}

/// Bounded byte-stream view over owned UART DMA chunks.
///
/// Reads consume at most one chunk per call. Dropping a pending read is safe:
/// no caller buffer is retained and the next read replaces the stored waker.
pub struct SerialReader<'a, const N: usize, const DEPTH: usize> {
    mailbox: &'a SharedSerialRxMailbox<N, DEPTH>,
    current: Option<RxChunk<N>>,
    offset: usize,
}

impl<const N: usize, const DEPTH: usize> ErrorType for SerialReader<'_, N, DEPTH> {
    type Error = SerialFault;
}

impl<const N: usize, const DEPTH: usize> Read for SerialReader<'_, N, DEPTH> {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize, Self::Error> {
        if output.is_empty() {
            return Ok(0);
        }

        poll_fn(|cx| self.poll_read(cx, output)).await
    }
}

impl<const N: usize, const DEPTH: usize> SerialReader<'_, N, DEPTH> {
    pub fn current_chunk(&self) -> Option<&RxChunk<N>> {
        self.current.as_ref()
    }

    fn poll_read(
        &mut self,
        cx: &mut Context<'_>,
        output: &mut [u8],
    ) -> Poll<Result<usize, SerialFault>> {
        loop {
            if let Some(chunk) = self.current.as_ref() {
                let len = usize::from(chunk.len);
                if len > N {
                    self.current = None;
                    self.offset = 0;
                    return Poll::Ready(Err(SerialFault::InvalidChunk));
                }
                if self.offset < len {
                    let count = output.len().min(len - self.offset);
                    output[..count].copy_from_slice(&chunk.bytes[self.offset..self.offset + count]);
                    self.offset += count;
                    return Poll::Ready(Ok(count));
                }
                self.current = None;
                self.offset = 0;
            }

            match critical_section::with(|cs| self.mailbox.borrow_ref_mut(cs).poll_chunk(cx)) {
                Poll::Ready(Ok(chunk)) => self.current = Some(chunk),
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialRxStatus {
    pub enabled: bool,
    pub pending_chunks: usize,
    pub discontinuity_count: u32,
    pub overflow_count: u32,
    pub latest_discontinuity: Option<DiscontinuityRecord>,
}

/// Out-of-band continuity and health view for safety-aware consumers.
pub struct DiscontinuityReader<'a, const N: usize, const DEPTH: usize> {
    mailbox: &'a SharedSerialRxMailbox<N, DEPTH>,
    observed_sequence: u32,
}

impl<const N: usize, const DEPTH: usize> DiscontinuityReader<'_, N, DEPTH> {
    pub fn status(&self) -> SerialRxStatus {
        critical_section::with(|cs| {
            let mailbox = self.mailbox.borrow_ref(cs);
            SerialRxStatus {
                enabled: mailbox.enabled,
                pending_chunks: mailbox.chunks.len(),
                discontinuity_count: mailbox.discontinuity_count,
                overflow_count: mailbox.overflow_count,
                latest_discontinuity: mailbox.latest_discontinuity,
            }
        })
    }

    pub fn take_new(&mut self) -> Option<DiscontinuityRecord> {
        let latest = self.status().latest_discontinuity?;
        if latest.sequence == self.observed_sequence {
            return None;
        }
        self.observed_sequence = latest.sequence;
        Some(latest)
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
    use crate::serial::{RxChunkError, RxCompletion};

    struct CountWake(AtomicUsize);

    impl Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn chunk<const N: usize>(bytes: &[u8], generation: u32) -> RxChunk<N> {
        RxChunk::from_slice(
            bytes,
            TimestampMicros(u64::from(generation) * 100),
            RxCompletion::Idle,
            StreamGeneration(generation),
            false,
        )
        .unwrap()
    }

    fn poll_once<F: Future>(future: Pin<&mut F>, waker: &Waker) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(waker))
    }

    fn ready_read<R: Read>(reader: &mut R, output: &mut [u8]) -> Result<usize, R::Error> {
        let waker = Waker::noop();
        let mut future = Box::pin(reader.read(output));
        match poll_once(future.as_mut(), waker) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("read unexpectedly pending"),
        }
    }

    #[test]
    fn owned_chunk_validates_and_copies_input() {
        let source = [1, 2, 3];
        let chunk = RxChunk::<4>::from_slice(
            &source,
            TimestampMicros(50),
            RxCompletion::DmaFull,
            StreamGeneration(7),
            true,
        )
        .unwrap();

        assert_eq!(chunk.as_slice(), Ok(source.as_slice()));
        assert_eq!(chunk.timestamp, TimestampMicros(50));
        assert_eq!(chunk.completion, RxCompletion::DmaFull);
        assert_eq!(chunk.generation, StreamGeneration(7));
        assert!(chunk.uart_error_seen);
        assert_eq!(
            RxChunk::<2>::from_slice(
                &source,
                TimestampMicros(0),
                RxCompletion::Idle,
                StreamGeneration(0),
                false,
            ),
            Err(RxChunkError::CapacityExceeded)
        );
    }

    #[test]
    fn reads_split_one_owned_chunk_without_losing_bytes() {
        let mut channel = SerialRxChannel::<8, 2>::new();
        let (mut producer, mut reader, _) = channel.split();
        producer.try_send(chunk(b"abcdef", 1)).unwrap();

        let mut first = [0; 2];
        let mut second = [0; 4];
        assert_eq!(ready_read(&mut reader, &mut first), Ok(2));
        assert_eq!(ready_read(&mut reader, &mut second), Ok(4));
        assert_eq!(&first, b"ab");
        assert_eq!(&second, b"cdef");
    }

    #[test]
    fn one_read_does_not_treat_adjacent_chunks_as_one_frame() {
        let mut channel = SerialRxChannel::<8, 2>::new();
        let (mut producer, mut reader, _) = channel.split();
        producer.try_send(chunk(b"abc", 1)).unwrap();
        producer.try_send(chunk(b"def", 2)).unwrap();

        let mut output = [0; 8];
        assert_eq!(ready_read(&mut reader, &mut output), Ok(3));
        assert_eq!(&output[..3], b"abc");
        assert_eq!(ready_read(&mut reader, &mut output), Ok(3));
        assert_eq!(&output[..3], b"def");
    }

    #[test]
    fn overflow_records_discontinuity_and_marks_next_chunk() {
        let mut channel = SerialRxChannel::<8, 1>::new();
        let (mut producer, mut reader, mut discontinuities) = channel.split();
        producer.try_send(chunk(b"first", 1)).unwrap();
        assert_eq!(
            producer.try_send(chunk(b"lost", 2)),
            Err(SerialFault::QueueOverflow)
        );

        let event = discontinuities.take_new().unwrap();
        assert_eq!(event.cause, Discontinuity::QueueOverflow);
        assert_eq!(event.generation, StreamGeneration(2));
        assert_eq!(discontinuities.take_new(), None);
        assert_eq!(discontinuities.status().overflow_count, 1);

        let mut output = [0; 8];
        assert_eq!(ready_read(&mut reader, &mut output), Ok(5));
        producer.try_send(chunk(b"next", 3)).unwrap();
        assert_eq!(ready_read(&mut reader, &mut output), Ok(4));
        assert!(reader.current.as_ref().unwrap().discontinuity_before);
    }

    #[test]
    fn dropping_pending_read_does_not_consume_future_data() {
        let mut channel = SerialRxChannel::<8, 2>::new();
        let (mut producer, mut reader, _) = channel.split();
        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut first_output = [0; 4];
        let mut pending = Box::pin(reader.read(&mut first_output));

        assert_eq!(poll_once(pending.as_mut(), &waker), Poll::Pending);
        drop(pending);
        producer.try_send(chunk(b"data", 1)).unwrap();
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);

        let mut second_output = [0; 4];
        assert_eq!(ready_read(&mut reader, &mut second_output), Ok(4));
        assert_eq!(&second_output, b"data");
    }

    #[test]
    fn disabled_empty_stream_returns_structured_error() {
        let mut channel = SerialRxChannel::<8, 2>::new();
        let (mut producer, mut reader, control) = channel.split();
        producer.disable();

        let mut output = [0; 4];
        assert_eq!(
            ready_read(&mut reader, &mut output),
            Err(SerialFault::Disabled)
        );
        assert!(!control.status().enabled);
    }

    #[test]
    fn serial_reader_implements_workspace_async_read() {
        fn assert_async_read<T: embedded_io_async::Read>() {}
        assert_async_read::<SerialReader<'static, 8, 2>>();
    }
}
