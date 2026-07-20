use core::{
    cell::RefCell,
    future::poll_fn,
    task::{Context, Poll},
};

use critical_section::Mutex;
use embedded_hal::spi::{Error, ErrorKind, ErrorType, Operation};
use embedded_hal_async::spi::SpiDevice;
use heapless::Vec;

use super::{
    SpiDeadlineUs, SpiJobError, SpiJobToken, SpiMailboxError, SpiOperationRef, SpiRequestMailbox,
};
use crate::time::TimestampMicros;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiDeviceError {
    Busy,
    TooManyOperations,
    TxCapacityExceeded,
    RxCapacityExceeded,
    CopybackShapeMismatch,
    DmaTransfer,
    Timeout,
    Cancelled,
    Unavailable,
    InvalidState,
    StaleTransaction,
    Backend,
}

impl Error for SpiDeviceError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

impl From<SpiJobError> for SpiDeviceError {
    fn from(error: SpiJobError) -> Self {
        match error {
            SpiJobError::TooManyOperations => Self::TooManyOperations,
            SpiJobError::TxCapacityExceeded => Self::TxCapacityExceeded,
            SpiJobError::RxCapacityExceeded => Self::RxCapacityExceeded,
            SpiJobError::CopybackShapeMismatch => Self::CopybackShapeMismatch,
        }
    }
}

impl From<SpiMailboxError> for SpiDeviceError {
    fn from(error: SpiMailboxError) -> Self {
        use super::{SpiFault, SpiTransitionError};

        match error {
            SpiMailboxError::Job(error) => error.into(),
            SpiMailboxError::Lifecycle(SpiTransitionError::Busy) => Self::Busy,
            SpiMailboxError::Lifecycle(SpiTransitionError::InvalidState)
            | SpiMailboxError::NotCompleted => Self::InvalidState,
            SpiMailboxError::StaleToken => Self::StaleTransaction,
            SpiMailboxError::Fault(SpiFault::Busy) => Self::Busy,
            SpiMailboxError::Fault(SpiFault::Oversize) => Self::TxCapacityExceeded,
            SpiMailboxError::Fault(SpiFault::DmaTransfer) => Self::DmaTransfer,
            SpiMailboxError::Fault(SpiFault::Timeout) => Self::Timeout,
            SpiMailboxError::Fault(SpiFault::Cancelled) => Self::Cancelled,
            SpiMailboxError::Fault(SpiFault::Unavailable) => Self::Unavailable,
            SpiMailboxError::Fault(SpiFault::Backend) => Self::Backend,
        }
    }
}

/// Backend contract for one uniquely owned asynchronous SPI device.
///
/// `submit` must copy all caller-owned TX bytes and operation metadata before
/// returning. The backend may retain RX data until `copy_results` is called,
/// but it must never retain any `SpiOperationRef` borrow.
pub trait AsyncSpiExecutor<const MAX_OPS: usize, const MAX_BYTES: usize> {
    fn submit(&mut self, operations: &[SpiOperationRef<'_>])
    -> Result<SpiJobToken, SpiDeviceError>;

    fn poll_completion(
        &mut self,
        token: SpiJobToken,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), SpiDeviceError>>;

    fn copy_results(
        &mut self,
        token: SpiJobToken,
        operations: &mut [SpiOperationRef<'_>],
    ) -> Result<(), SpiDeviceError>;

    fn request_cancel(&mut self, token: SpiJobToken);
}

pub type SharedSpiRequestMailbox<const MAX_OPS: usize, const MAX_BYTES: usize> =
    Mutex<RefCell<SpiRequestMailbox<MAX_OPS, MAX_BYTES>>>;

/// A short-critical-section adapter between one async task and an IRQ owner.
///
/// The owner callback must only pend the owner context; it must not touch the
/// mailbox or hardware inline.
pub struct CriticalSectionSpiExecutor<'a, const MAX_OPS: usize, const MAX_BYTES: usize> {
    mailbox: &'a SharedSpiRequestMailbox<MAX_OPS, MAX_BYTES>,
    start: TimestampMicros,
    timeout: SpiDeadlineUs,
    pend_owner: fn(),
}

impl<'a, const MAX_OPS: usize, const MAX_BYTES: usize>
    CriticalSectionSpiExecutor<'a, MAX_OPS, MAX_BYTES>
{
    pub const fn new(
        mailbox: &'a SharedSpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        timeout: SpiDeadlineUs,
        pend_owner: fn(),
    ) -> Self {
        Self {
            mailbox,
            start: TimestampMicros(0),
            timeout,
            pend_owner,
        }
    }

    pub fn set_start(&mut self, start: TimestampMicros) {
        self.start = start;
    }
}

impl<const MAX_OPS: usize, const MAX_BYTES: usize> AsyncSpiExecutor<MAX_OPS, MAX_BYTES>
    for CriticalSectionSpiExecutor<'_, MAX_OPS, MAX_BYTES>
{
    fn submit(
        &mut self,
        operations: &[SpiOperationRef<'_>],
    ) -> Result<SpiJobToken, SpiDeviceError> {
        let result = critical_section::with(|cs| {
            self.mailbox
                .borrow_ref_mut(cs)
                .submit(self.start, self.timeout, operations)
        })
        .map_err(Into::into);

        if result.is_ok() {
            (self.pend_owner)();
        }
        result
    }

    fn poll_completion(
        &mut self,
        token: SpiJobToken,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), SpiDeviceError>> {
        critical_section::with(|cs| {
            self.mailbox
                .borrow_ref_mut(cs)
                .poll_completion(token, cx)
                .map(|result| result.map_err(Into::into))
        })
    }

    fn copy_results(
        &mut self,
        token: SpiJobToken,
        operations: &mut [SpiOperationRef<'_>],
    ) -> Result<(), SpiDeviceError> {
        critical_section::with(|cs| {
            self.mailbox
                .borrow_ref_mut(cs)
                .copy_results(token, operations)
                .map_err(Into::into)
        })
    }

    fn request_cancel(&mut self, token: SpiJobToken) {
        let requested =
            critical_section::with(|cs| self.mailbox.borrow_ref_mut(cs).request_cancel(token));
        if requested {
            (self.pend_owner)();
        }
    }
}

/// A non-cloneable `embedded-hal-async` SPI device backed by one executor.
pub struct AsyncSpiDevice<E, const MAX_OPS: usize, const MAX_BYTES: usize> {
    executor: E,
}

impl<E, const MAX_OPS: usize, const MAX_BYTES: usize> AsyncSpiDevice<E, MAX_OPS, MAX_BYTES> {
    pub const fn new(executor: E) -> Self {
        Self { executor }
    }

    pub const fn executor(&self) -> &E {
        &self.executor
    }

    pub fn executor_mut(&mut self) -> &mut E {
        &mut self.executor
    }

    pub fn into_inner(self) -> E {
        self.executor
    }
}

impl<E, const MAX_OPS: usize, const MAX_BYTES: usize> ErrorType
    for AsyncSpiDevice<E, MAX_OPS, MAX_BYTES>
{
    type Error = SpiDeviceError;
}

impl<E, const MAX_OPS: usize, const MAX_BYTES: usize> SpiDevice<u8>
    for AsyncSpiDevice<E, MAX_OPS, MAX_BYTES>
where
    E: AsyncSpiExecutor<MAX_OPS, MAX_BYTES>,
{
    async fn transaction(
        &mut self,
        operations: &mut [Operation<'_, u8>],
    ) -> Result<(), Self::Error> {
        let mut borrowed = borrow_operations::<MAX_OPS>(operations)?;
        let token = self.executor.submit(borrowed.as_slice())?;
        let mut submitted = SubmittedTransaction {
            executor: &mut self.executor,
            token,
            terminal: false,
        };

        poll_fn(|cx| submitted.poll_completion(cx)).await?;
        submitted.copy_results(borrowed.as_mut_slice())
    }
}

struct SubmittedTransaction<'a, E, const MAX_OPS: usize, const MAX_BYTES: usize>
where
    E: AsyncSpiExecutor<MAX_OPS, MAX_BYTES>,
{
    executor: &'a mut E,
    token: SpiJobToken,
    terminal: bool,
}

impl<E, const MAX_OPS: usize, const MAX_BYTES: usize>
    SubmittedTransaction<'_, E, MAX_OPS, MAX_BYTES>
where
    E: AsyncSpiExecutor<MAX_OPS, MAX_BYTES>,
{
    fn poll_completion(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SpiDeviceError>> {
        let result = self.executor.poll_completion(self.token, cx);
        if result.is_ready() {
            self.terminal = true;
        }
        result
    }

    fn copy_results(
        &mut self,
        operations: &mut [SpiOperationRef<'_>],
    ) -> Result<(), SpiDeviceError> {
        self.executor.copy_results(self.token, operations)
    }
}

impl<E, const MAX_OPS: usize, const MAX_BYTES: usize> Drop
    for SubmittedTransaction<'_, E, MAX_OPS, MAX_BYTES>
where
    E: AsyncSpiExecutor<MAX_OPS, MAX_BYTES>,
{
    fn drop(&mut self) {
        if !self.terminal {
            self.executor.request_cancel(self.token);
        }
    }
}

fn borrow_operations<'operations, 'buffers, const MAX_OPS: usize>(
    operations: &'operations mut [Operation<'buffers, u8>],
) -> Result<Vec<SpiOperationRef<'operations>, MAX_OPS>, SpiDeviceError>
where
    'buffers: 'operations,
{
    let mut borrowed = Vec::new();

    for operation in operations {
        let operation = match operation {
            Operation::Read(read) => SpiOperationRef::Read(read),
            Operation::Write(write) => SpiOperationRef::Write(write),
            Operation::Transfer(read, write) => SpiOperationRef::Transfer { read, write },
            Operation::TransferInPlace(data) => SpiOperationRef::TransferInPlace(data),
            Operation::DelayNs(ns) => SpiOperationRef::DelayNs(*ns),
        };
        borrowed
            .push(operation)
            .map_err(|_| SpiDeviceError::TooManyOperations)?;
    }

    Ok(borrowed)
}

#[cfg(test)]
mod tests {
    use core::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll, Waker},
    };
    use std::{
        boxed::Box,
        task::{Poll::Pending, Poll::Ready},
    };

    use super::*;
    use crate::{
        spi::{SpiCompletion, SpiDeadlineUs, SpiMailboxOwnerAction, SpiRequestMailbox},
        time::TimestampMicros,
    };

    const DEADLINE: SpiDeadlineUs = SpiDeadlineUs(250);
    static SHARED_EXECUTOR_MAILBOX: SharedSpiRequestMailbox<2, 8> =
        Mutex::new(RefCell::new(SpiRequestMailbox::new()));
    static OWNER_PENDS: AtomicUsize = AtomicUsize::new(0);

    fn count_owner_pend() {
        OWNER_PENDS.fetch_add(1, Ordering::Relaxed);
    }

    fn test_waker() -> Waker {
        Waker::noop().clone()
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = test_waker();
        let mut cx = Context::from_waker(&waker);
        let mut future = Box::pin(future);

        loop {
            match future.as_mut().poll(&mut cx) {
                Ready(output) => return output,
                Pending => std::thread::yield_now(),
            }
        }
    }

    struct TestExecutor<const MAX_OPS: usize, const MAX_BYTES: usize> {
        mailbox: SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        auto_complete: bool,
        fail_dma: bool,
        cancellations: usize,
    }

    impl<const MAX_OPS: usize, const MAX_BYTES: usize> TestExecutor<MAX_OPS, MAX_BYTES> {
        const fn new(auto_complete: bool) -> Self {
            Self {
                mailbox: SpiRequestMailbox::new(),
                auto_complete,
                fail_dma: false,
                cancellations: 0,
            }
        }

        fn service_owner(&mut self, token: SpiJobToken) {
            if self.mailbox.owner_action() != SpiMailboxOwnerAction::Start(token) {
                return;
            }

            self.mailbox.activate(token).unwrap();
            if self.fail_dma {
                assert_eq!(self.mailbox.dma_fault(token), SpiCompletion::Faulted);
                return;
            }

            for (index, byte) in self
                .mailbox
                .job_mut(token)
                .unwrap()
                .rx_bytes_mut()
                .iter_mut()
                .enumerate()
            {
                *byte = 0x40 + index as u8;
            }
            assert_eq!(self.mailbox.complete(token), SpiCompletion::Completed);
        }
    }

    impl<const MAX_OPS: usize, const MAX_BYTES: usize> AsyncSpiExecutor<MAX_OPS, MAX_BYTES>
        for TestExecutor<MAX_OPS, MAX_BYTES>
    {
        fn submit(
            &mut self,
            operations: &[SpiOperationRef<'_>],
        ) -> Result<SpiJobToken, SpiDeviceError> {
            self.mailbox
                .submit(TimestampMicros(0), DEADLINE, operations)
                .map_err(Into::into)
        }

        fn poll_completion(
            &mut self,
            token: SpiJobToken,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), SpiDeviceError>> {
            if self.auto_complete {
                self.service_owner(token);
            }
            self.mailbox
                .poll_completion(token, cx)
                .map(|result| result.map_err(Into::into))
        }

        fn copy_results(
            &mut self,
            token: SpiJobToken,
            operations: &mut [SpiOperationRef<'_>],
        ) -> Result<(), SpiDeviceError> {
            self.mailbox
                .copy_results(token, operations)
                .map_err(Into::into)
        }

        fn request_cancel(&mut self, token: SpiJobToken) {
            self.cancellations += 1;
            let _ = self.mailbox.request_cancel(token);
        }
    }

    #[test]
    fn standard_mixed_transaction_copies_only_rx_results_back() {
        let executor = TestExecutor::<8, 32>::new(true);
        let mut device = AsyncSpiDevice::new(executor);
        let write = [1, 2];
        let mut read = [0; 2];
        let transfer_write = [0x80, 0, 0];
        let mut transfer_read = [0; 3];
        let mut in_place = [9, 8];
        let mut operations = [
            Operation::Write(&write),
            Operation::Read(&mut read),
            Operation::Transfer(&mut transfer_read, &transfer_write),
            Operation::TransferInPlace(&mut in_place),
            Operation::DelayNs(100),
        ];

        assert_eq!(block_on(device.transaction(&mut operations)), Ok(()));
        assert_eq!(read, [0x40, 0x41]);
        assert_eq!(transfer_read, [0x42, 0x43, 0x44]);
        assert_eq!(in_place, [0x45, 0x46]);
        assert_eq!(write, [1, 2]);
        assert_eq!(device.executor().mailbox.submitted_token(), None);
    }

    #[test]
    fn operation_limit_is_rejected_before_executor_submission() {
        let executor = TestExecutor::<1, 8>::new(true);
        let mut device = AsyncSpiDevice::new(executor);
        let first = [1];
        let second = [2];
        let mut operations = [Operation::Write(&first), Operation::Write(&second)];

        assert_eq!(
            block_on(device.transaction(&mut operations)),
            Err(SpiDeviceError::TooManyOperations)
        );
        assert_eq!(device.executor().mailbox.submitted_token(), None);
    }

    #[test]
    fn dropping_pending_transaction_requests_generation_scoped_cancel() {
        let executor = TestExecutor::<2, 8>::new(false);
        let mut device = AsyncSpiDevice::new(executor);
        let write = [1, 2];
        let mut operations = [Operation::Write(&write)];
        let waker = test_waker();
        let mut cx = Context::from_waker(&waker);

        let mut future = Box::pin(device.transaction(&mut operations));
        assert_eq!(Pin::as_mut(&mut future).poll(&mut cx), Poll::Pending);
        drop(future);

        assert_eq!(device.executor().cancellations, 1);
        let token = device.executor().mailbox.submitted_token().unwrap();
        assert_eq!(
            device.executor().mailbox.owner_action(),
            SpiMailboxOwnerAction::Abort(token)
        );
    }

    #[test]
    fn terminal_dma_error_does_not_copy_rx_or_request_cancel() {
        let mut executor = TestExecutor::<2, 8>::new(true);
        executor.fail_dma = true;
        let mut device = AsyncSpiDevice::new(executor);
        let mut read = [0xaa; 2];
        let mut operations = [Operation::Read(&mut read)];

        assert_eq!(
            block_on(device.transaction(&mut operations)),
            Err(SpiDeviceError::DmaTransfer)
        );
        assert_eq!(read, [0xaa; 2]);
        assert_eq!(device.executor().cancellations, 0);
        assert_eq!(device.executor().mailbox.submitted_token(), None);
    }

    #[test]
    fn critical_section_executor_pends_on_submit_and_cancel() {
        OWNER_PENDS.store(0, Ordering::Relaxed);
        let mut executor =
            CriticalSectionSpiExecutor::new(&SHARED_EXECUTOR_MAILBOX, DEADLINE, count_owner_pend);
        executor.set_start(TimestampMicros(100));
        let write = [1, 2];
        let token = executor.submit(&[SpiOperationRef::Write(&write)]).unwrap();

        assert_eq!(OWNER_PENDS.load(Ordering::Relaxed), 1);
        critical_section::with(|cs| {
            assert_eq!(
                SHARED_EXECUTOR_MAILBOX.borrow_ref(cs).owner_action(),
                SpiMailboxOwnerAction::Start(token)
            );
        });

        executor.request_cancel(token);
        assert_eq!(OWNER_PENDS.load(Ordering::Relaxed), 2);
        critical_section::with(|cs| {
            let mut mailbox = SHARED_EXECUTOR_MAILBOX.borrow_ref_mut(cs);
            assert_eq!(mailbox.owner_action(), SpiMailboxOwnerAction::Abort(token));
            mailbox.owner_abort(token).unwrap();
        });
    }
}
