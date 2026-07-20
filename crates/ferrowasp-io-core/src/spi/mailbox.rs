use core::task::{Context, Poll, Waker};

use super::{
    OwnedSpiJob, SpiCompletion, SpiDeadlineUs, SpiFault, SpiJobError, SpiJobState, SpiJobToken,
    SpiOperationRef, SpiTransactionState, SpiTransitionError,
};
use crate::time::TimestampMicros;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiMailboxError {
    Job(SpiJobError),
    Lifecycle(SpiTransitionError),
    StaleToken,
    NotCompleted,
    Fault(SpiFault),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiMailboxOwnerAction {
    Idle,
    Start(SpiJobToken),
    Abort(SpiJobToken),
}

/// A bounded, single-request SPI mailbox.
///
/// The surrounding backend must serialize access, pend its owner context after
/// submission or cancellation, and perform all peripheral/DMA operations from
/// that owner. This type contains only owned job data and an owned waiter.
pub struct SpiRequestMailbox<const MAX_OPS: usize, const MAX_BYTES: usize> {
    lifecycle: SpiTransactionState,
    job: OwnedSpiJob<MAX_OPS, MAX_BYTES>,
    submitted: Option<SpiJobToken>,
    timeout: Option<SpiDeadlineUs>,
    completion: Option<Result<(), SpiFault>>,
    waiter: Option<Waker>,
    abandoned: bool,
}

impl<const MAX_OPS: usize, const MAX_BYTES: usize> SpiRequestMailbox<MAX_OPS, MAX_BYTES> {
    pub const fn new() -> Self {
        Self {
            lifecycle: SpiTransactionState::new(),
            job: OwnedSpiJob::new(),
            submitted: None,
            timeout: None,
            completion: None,
            waiter: None,
            abandoned: false,
        }
    }

    pub const fn lifecycle(&self) -> &SpiTransactionState {
        &self.lifecycle
    }

    pub const fn submitted_token(&self) -> Option<SpiJobToken> {
        self.submitted
    }

    pub fn submit(
        &mut self,
        start: TimestampMicros,
        timeout: SpiDeadlineUs,
        operations: &[SpiOperationRef<'_>],
    ) -> Result<SpiJobToken, SpiMailboxError> {
        let token = self
            .lifecycle
            .prepare(start, timeout)
            .map_err(SpiMailboxError::Lifecycle)?;

        if let Err(error) = self.job.pack(token.id, operations) {
            self.recycle_prepared();
            return Err(SpiMailboxError::Job(error));
        }

        if let Err(error) = self.lifecycle.mark_pending() {
            self.recycle_prepared();
            return Err(SpiMailboxError::Lifecycle(error));
        }

        self.submitted = Some(token);
        self.timeout = Some(timeout);
        self.completion = None;
        self.waiter = None;
        self.abandoned = false;
        Ok(token)
    }

    pub const fn owner_action(&self) -> SpiMailboxOwnerAction {
        let Some(token) = self.submitted else {
            return SpiMailboxOwnerAction::Idle;
        };

        match self.lifecycle.state() {
            SpiJobState::Pending => SpiMailboxOwnerAction::Start(token),
            SpiJobState::CancelRequested => SpiMailboxOwnerAction::Abort(token),
            _ => SpiMailboxOwnerAction::Idle,
        }
    }

    pub fn activate(&mut self, token: SpiJobToken) -> Result<(), SpiMailboxError> {
        self.validate_token(token)?;
        self.lifecycle
            .activate()
            .map_err(SpiMailboxError::Lifecycle)
    }

    pub fn activate_at(
        &mut self,
        token: SpiJobToken,
        start: TimestampMicros,
    ) -> Result<(), SpiMailboxError> {
        self.validate_token(token)?;
        let timeout = self
            .timeout
            .ok_or(SpiMailboxError::Lifecycle(SpiTransitionError::InvalidState))?;
        self.lifecycle
            .activate_at(start, timeout)
            .map_err(SpiMailboxError::Lifecycle)
    }

    pub fn job(
        &self,
        token: SpiJobToken,
    ) -> Result<&OwnedSpiJob<MAX_OPS, MAX_BYTES>, SpiMailboxError> {
        self.validate_token(token)?;
        Ok(&self.job)
    }

    pub fn job_mut(
        &mut self,
        token: SpiJobToken,
    ) -> Result<&mut OwnedSpiJob<MAX_OPS, MAX_BYTES>, SpiMailboxError> {
        self.validate_token(token)?;
        Ok(&mut self.job)
    }

    pub fn complete(&mut self, token: SpiJobToken) -> SpiCompletion {
        if self.submitted != Some(token) {
            return SpiCompletion::Stale;
        }

        let outcome = self.lifecycle.complete(token.generation);
        if outcome == SpiCompletion::Completed {
            self.publish_completion(Ok(()));
        }
        outcome
    }

    pub fn dma_fault(&mut self, token: SpiJobToken) -> SpiCompletion {
        if self.submitted != Some(token) {
            return SpiCompletion::Stale;
        }

        let outcome = self.lifecycle.dma_fault(token.generation);
        if outcome == SpiCompletion::Faulted {
            self.publish_completion(Err(SpiFault::DmaTransfer));
        }
        outcome
    }

    pub fn owner_fault(&mut self, token: SpiJobToken, fault: SpiFault) -> SpiCompletion {
        if self.submitted != Some(token) {
            return SpiCompletion::Stale;
        }

        let outcome = self.lifecycle.owner_fault(token.generation, fault);
        if outcome == SpiCompletion::Faulted {
            self.publish_completion(Err(fault));
        }
        outcome
    }

    pub fn request_cancel(&mut self, token: SpiJobToken) -> bool {
        if self.submitted != Some(token) {
            return false;
        }

        self.abandoned = true;
        self.waiter = None;

        if self.lifecycle.request_cancel() {
            return true;
        }

        if matches!(
            self.lifecycle.state(),
            SpiJobState::Completed | SpiJobState::Faulted
        ) {
            self.recycle_terminal();
            return true;
        }

        false
    }

    pub fn check_timeout(&mut self, now: TimestampMicros) -> bool {
        self.lifecycle.check_timeout(now)
    }

    pub fn owner_abort(&mut self, token: SpiJobToken) -> Result<SpiFault, SpiMailboxError> {
        self.validate_token(token)?;
        let fault = self
            .lifecycle
            .owner_abort()
            .map_err(SpiMailboxError::Lifecycle)?;
        self.publish_completion(Err(fault));
        Ok(fault)
    }

    pub fn poll_completion(
        &mut self,
        token: SpiJobToken,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), SpiMailboxError>> {
        if self.submitted != Some(token) {
            return Poll::Ready(Err(SpiMailboxError::StaleToken));
        }

        match self.completion {
            Some(Ok(())) => Poll::Ready(Ok(())),
            Some(Err(fault)) => {
                self.recycle_terminal();
                Poll::Ready(Err(SpiMailboxError::Fault(fault)))
            }
            None => {
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
    }

    pub fn copy_results(
        &mut self,
        token: SpiJobToken,
        operations: &mut [SpiOperationRef<'_>],
    ) -> Result<(), SpiMailboxError> {
        self.validate_token(token)?;
        if self.completion != Some(Ok(())) {
            return Err(SpiMailboxError::NotCompleted);
        }

        let result = self
            .job
            .copy_results(operations)
            .map_err(SpiMailboxError::Job);
        self.recycle_terminal();
        result
    }

    fn validate_token(&self, token: SpiJobToken) -> Result<(), SpiMailboxError> {
        if self.submitted == Some(token) {
            Ok(())
        } else {
            Err(SpiMailboxError::StaleToken)
        }
    }

    fn publish_completion(&mut self, result: Result<(), SpiFault>) {
        self.completion = Some(result);

        if self.abandoned {
            self.recycle_terminal();
        } else if let Some(waiter) = self.waiter.take() {
            waiter.wake();
        }
    }

    fn recycle_prepared(&mut self) {
        let _ = self.lifecycle.request_cancel();
        let _ = self.lifecycle.owner_abort();
        let _ = self.lifecycle.take_result();
    }

    fn recycle_terminal(&mut self) {
        let _ = self.lifecycle.take_result();
        self.submitted = None;
        self.timeout = None;
        self.completion = None;
        self.waiter = None;
        self.abandoned = false;
    }
}

impl<const MAX_OPS: usize, const MAX_BYTES: usize> Default
    for SpiRequestMailbox<MAX_OPS, MAX_BYTES>
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use core::{
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Waker},
    };
    use std::{sync::Arc, task::Wake};

    use super::*;

    const DEADLINE: SpiDeadlineUs = SpiDeadlineUs(250);

    struct CountWake(AtomicUsize);

    impl Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn counting_waker() -> (Arc<CountWake>, Waker) {
        let counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        (counter, waker)
    }

    #[test]
    fn successful_completion_wakes_and_retains_rx_until_copyback() {
        let mut mailbox = SpiRequestMailbox::<4, 16>::new();
        let mut read = [0; 3];
        let write = [0x80, 0, 0];
        let operations = [SpiOperationRef::Transfer {
            read: &mut read,
            write: &write,
        }];
        let token = mailbox
            .submit(TimestampMicros(10), DEADLINE, &operations)
            .unwrap();

        assert_eq!(mailbox.owner_action(), SpiMailboxOwnerAction::Start(token));
        assert_eq!(mailbox.job(token).unwrap().tx_bytes(), &write);
        mailbox.activate(token).unwrap();

        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(mailbox.poll_completion(token, &mut cx), Poll::Pending);

        mailbox
            .job_mut(token)
            .unwrap()
            .rx_bytes_mut()
            .copy_from_slice(&[7, 8, 9]);
        assert_eq!(mailbox.complete(token), SpiCompletion::Completed);
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(mailbox.poll_completion(token, &mut cx), Poll::Ready(Ok(())));
        assert_eq!(mailbox.lifecycle().state(), SpiJobState::Completed);

        let mut copyback = [SpiOperationRef::Transfer {
            read: &mut read,
            write: &write,
        }];
        mailbox.copy_results(token, &mut copyback).unwrap();

        assert_eq!(read, [7, 8, 9]);
        assert_eq!(mailbox.lifecycle().state(), SpiJobState::Free);
        assert_eq!(mailbox.submitted_token(), None);
    }

    #[test]
    fn dropped_waiter_cancels_and_late_generation_cannot_complete_reused_slot() {
        let mut mailbox = SpiRequestMailbox::<2, 8>::new();
        let write = [1, 2];
        let old = mailbox
            .submit(
                TimestampMicros(0),
                DEADLINE,
                &[SpiOperationRef::Write(&write)],
            )
            .unwrap();
        mailbox.activate(old).unwrap();

        assert!(mailbox.request_cancel(old));
        assert_eq!(mailbox.owner_action(), SpiMailboxOwnerAction::Abort(old));
        assert_eq!(mailbox.owner_abort(old), Ok(SpiFault::Cancelled));
        assert_eq!(mailbox.lifecycle().state(), SpiJobState::Free);

        let current = mailbox
            .submit(
                TimestampMicros(500),
                DEADLINE,
                &[SpiOperationRef::Write(&write)],
            )
            .unwrap();
        mailbox.activate(current).unwrap();

        assert_eq!(mailbox.complete(old), SpiCompletion::Stale);
        assert_eq!(mailbox.lifecycle().state(), SpiJobState::Active);
        assert_eq!(mailbox.complete(current), SpiCompletion::Completed);
    }

    #[test]
    fn timeout_is_delivered_to_existing_waiter_and_recycles_on_poll() {
        let mut mailbox = SpiRequestMailbox::<2, 8>::new();
        let write = [1];
        let token = mailbox
            .submit(
                TimestampMicros(1_000),
                DEADLINE,
                &[SpiOperationRef::Write(&write)],
            )
            .unwrap();
        mailbox.activate(token).unwrap();

        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(mailbox.poll_completion(token, &mut cx), Poll::Pending);
        assert!(mailbox.check_timeout(TimestampMicros(1_250)));
        assert_eq!(mailbox.owner_abort(token), Ok(SpiFault::Timeout));
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(
            mailbox.poll_completion(token, &mut cx),
            Poll::Ready(Err(SpiMailboxError::Fault(SpiFault::Timeout)))
        );
        assert_eq!(mailbox.lifecycle().state(), SpiJobState::Free);
    }

    #[test]
    fn owner_activation_rebases_mailbox_wire_deadline() {
        let mut mailbox = SpiRequestMailbox::<2, 8>::new();
        let write = [1];
        let token = mailbox
            .submit(
                TimestampMicros(1_000),
                DEADLINE,
                &[SpiOperationRef::Write(&write)],
            )
            .unwrap();
        mailbox.activate_at(token, TimestampMicros(1_200)).unwrap();

        assert!(!mailbox.check_timeout(TimestampMicros(1_449)));
        assert!(mailbox.check_timeout(TimestampMicros(1_450)));
    }

    #[test]
    fn packing_failure_leaves_slot_reusable() {
        let mut mailbox = SpiRequestMailbox::<1, 2>::new();
        let oversized = [0; 3];

        assert_eq!(
            mailbox.submit(
                TimestampMicros(0),
                DEADLINE,
                &[SpiOperationRef::Write(&oversized)]
            ),
            Err(SpiMailboxError::Job(SpiJobError::TxCapacityExceeded))
        );
        assert_eq!(mailbox.lifecycle().state(), SpiJobState::Free);

        let valid = [1, 2];
        assert!(
            mailbox
                .submit(
                    TimestampMicros(1),
                    DEADLINE,
                    &[SpiOperationRef::Write(&valid)]
                )
                .is_ok()
        );
    }
}
