use super::{SpiDeadlineUs, SpiFault, TransactionId};
use crate::time::{Deadline, TimestampMicros};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpiGeneration(pub u32);

impl SpiGeneration {
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpiJobState {
    #[default]
    Free,
    Prepared,
    Pending,
    Active,
    CancelRequested,
    Completing,
    Completed,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiJobToken {
    pub id: TransactionId,
    pub generation: SpiGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiCompletion {
    Completed,
    Faulted,
    Stale,
    Ignored,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiTransitionError {
    Busy,
    InvalidState,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpiTransactionState {
    state: SpiJobState,
    next_id: TransactionId,
    active_id: Option<TransactionId>,
    generation: SpiGeneration,
    deadline: Option<Deadline>,
    fault: Option<SpiFault>,
}

impl SpiTransactionState {
    pub const fn new() -> Self {
        Self {
            state: SpiJobState::Free,
            next_id: TransactionId(0),
            active_id: None,
            generation: SpiGeneration(0),
            deadline: None,
            fault: None,
        }
    }

    pub const fn state(&self) -> SpiJobState {
        self.state
    }

    pub const fn active_token(&self) -> Option<SpiJobToken> {
        if matches!(
            self.state,
            SpiJobState::Prepared
                | SpiJobState::Pending
                | SpiJobState::Active
                | SpiJobState::CancelRequested
                | SpiJobState::Completing
        ) {
            match self.active_id {
                Some(id) => Some(SpiJobToken {
                    id,
                    generation: self.generation,
                }),
                None => None,
            }
        } else {
            None
        }
    }

    pub fn prepare(
        &mut self,
        start: TimestampMicros,
        timeout: SpiDeadlineUs,
    ) -> Result<SpiJobToken, SpiTransitionError> {
        if self.state != SpiJobState::Free {
            return Err(SpiTransitionError::Busy);
        }

        let id = self.next_id;
        self.next_id = self.next_id.next();
        self.generation = self.generation.next();
        self.active_id = Some(id);
        self.deadline = Some(Deadline::new(start, timeout.0));
        self.fault = None;
        self.state = SpiJobState::Prepared;

        Ok(SpiJobToken {
            id,
            generation: self.generation,
        })
    }

    pub fn mark_pending(&mut self) -> Result<(), SpiTransitionError> {
        self.transition(SpiJobState::Prepared, SpiJobState::Pending)
    }

    pub fn activate(&mut self) -> Result<(), SpiTransitionError> {
        self.transition(SpiJobState::Pending, SpiJobState::Active)
    }

    pub fn activate_at(
        &mut self,
        start: TimestampMicros,
        timeout: SpiDeadlineUs,
    ) -> Result<(), SpiTransitionError> {
        self.activate()?;
        self.deadline = Some(Deadline::new(start, timeout.0));
        Ok(())
    }

    pub fn request_cancel(&mut self) -> bool {
        if matches!(
            self.state,
            SpiJobState::Prepared | SpiJobState::Pending | SpiJobState::Active
        ) {
            self.fault = Some(SpiFault::Cancelled);
            self.state = SpiJobState::CancelRequested;
            true
        } else {
            false
        }
    }

    pub fn check_timeout(&mut self, now: TimestampMicros) -> bool {
        let is_expired = self.deadline_expired(now);

        if is_expired
            && matches!(
                self.state,
                SpiJobState::Prepared | SpiJobState::Pending | SpiJobState::Active
            )
        {
            self.fault = Some(SpiFault::Timeout);
            self.state = SpiJobState::CancelRequested;
            true
        } else {
            false
        }
    }

    pub fn deadline_expired(&self, now: TimestampMicros) -> bool {
        self.deadline
            .is_some_and(|deadline| deadline.is_expired_at(now))
            && matches!(
                self.state,
                SpiJobState::Prepared | SpiJobState::Pending | SpiJobState::Active
            )
    }

    pub fn owner_abort(&mut self) -> Result<SpiFault, SpiTransitionError> {
        if self.state != SpiJobState::CancelRequested {
            return Err(SpiTransitionError::InvalidState);
        }

        let fault = self.fault.unwrap_or(SpiFault::Cancelled);
        self.deadline = None;
        self.generation = self.generation.next();
        self.state = SpiJobState::Faulted;
        Ok(fault)
    }

    pub fn complete(&mut self, observed: SpiGeneration) -> SpiCompletion {
        if observed != self.generation {
            return SpiCompletion::Stale;
        }

        if self.state != SpiJobState::Active {
            return SpiCompletion::Ignored;
        }

        self.state = SpiJobState::Completing;
        self.deadline = None;
        self.state = SpiJobState::Completed;
        SpiCompletion::Completed
    }

    pub fn dma_fault(&mut self, observed: SpiGeneration) -> SpiCompletion {
        if observed != self.generation {
            return SpiCompletion::Stale;
        }

        if self.state != SpiJobState::Active {
            return SpiCompletion::Ignored;
        }

        self.deadline = None;
        self.fault = Some(SpiFault::DmaTransfer);
        self.generation = self.generation.next();
        self.state = SpiJobState::Faulted;
        SpiCompletion::Faulted
    }

    pub fn owner_fault(&mut self, observed: SpiGeneration, fault: SpiFault) -> SpiCompletion {
        if observed != self.generation {
            return SpiCompletion::Stale;
        }

        if !matches!(
            self.state,
            SpiJobState::Prepared | SpiJobState::Pending | SpiJobState::Active
        ) {
            return SpiCompletion::Ignored;
        }

        self.deadline = None;
        self.fault = Some(fault);
        self.generation = self.generation.next();
        self.state = SpiJobState::Faulted;
        SpiCompletion::Faulted
    }

    pub fn take_result(&mut self) -> Option<Result<TransactionId, SpiFault>> {
        let result = match self.state {
            SpiJobState::Completed => Some(Ok(self.active_id?)),
            SpiJobState::Faulted => Some(Err(self.fault?)),
            _ => None,
        }?;

        self.active_id = None;
        self.deadline = None;
        self.fault = None;
        self.state = SpiJobState::Free;
        Some(result)
    }

    fn transition(
        &mut self,
        expected: SpiJobState,
        next: SpiJobState,
    ) -> Result<(), SpiTransitionError> {
        if self.state != expected {
            return Err(SpiTransitionError::InvalidState);
        }

        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMU_DEADLINE: SpiDeadlineUs = SpiDeadlineUs(250);

    #[test]
    fn successful_job_moves_through_owner_states_and_recycles() {
        let mut transaction = SpiTransactionState::new();
        let token = transaction
            .prepare(TimestampMicros(1_000), IMU_DEADLINE)
            .unwrap();

        assert_eq!(token.id, TransactionId(0));
        assert_eq!(transaction.state(), SpiJobState::Prepared);
        transaction.mark_pending().unwrap();
        transaction.activate().unwrap();
        assert_eq!(
            transaction.complete(token.generation),
            SpiCompletion::Completed
        );
        assert_eq!(transaction.take_result(), Some(Ok(TransactionId(0))));
        assert_eq!(transaction.state(), SpiJobState::Free);
    }

    #[test]
    fn occupied_slot_rejects_a_second_job() {
        let mut transaction = SpiTransactionState::new();
        transaction
            .prepare(TimestampMicros(0), IMU_DEADLINE)
            .unwrap();

        assert_eq!(
            transaction.prepare(TimestampMicros(1), IMU_DEADLINE),
            Err(SpiTransitionError::Busy)
        );
    }

    #[test]
    fn timeout_requests_owner_cancellation_and_rejects_late_irq() {
        let mut transaction = SpiTransactionState::new();
        let token = transaction
            .prepare(TimestampMicros(1_000), IMU_DEADLINE)
            .unwrap();
        transaction.mark_pending().unwrap();
        transaction.activate().unwrap();

        assert!(!transaction.check_timeout(TimestampMicros(1_249)));
        assert!(transaction.check_timeout(TimestampMicros(1_250)));
        assert_eq!(transaction.state(), SpiJobState::CancelRequested);
        assert_eq!(transaction.owner_abort(), Ok(SpiFault::Timeout));
        assert_eq!(transaction.complete(token.generation), SpiCompletion::Stale);
        assert_eq!(transaction.take_result(), Some(Err(SpiFault::Timeout)));
    }

    #[test]
    fn deadline_probe_does_not_change_owner_state() {
        let mut transaction = SpiTransactionState::new();
        transaction
            .prepare(TimestampMicros(1_000), IMU_DEADLINE)
            .unwrap();
        transaction.mark_pending().unwrap();
        transaction.activate().unwrap();

        assert!(transaction.deadline_expired(TimestampMicros(1_250)));
        assert_eq!(transaction.state(), SpiJobState::Active);
    }

    #[test]
    fn owner_activation_rebases_wire_deadline_after_pending_latency() {
        let mut transaction = SpiTransactionState::new();
        transaction
            .prepare(TimestampMicros(1_000), IMU_DEADLINE)
            .unwrap();
        transaction.mark_pending().unwrap();
        transaction
            .activate_at(TimestampMicros(1_200), IMU_DEADLINE)
            .unwrap();

        assert!(!transaction.deadline_expired(TimestampMicros(1_449)));
        assert!(transaction.deadline_expired(TimestampMicros(1_450)));
    }

    #[test]
    fn explicit_pending_cancel_never_needs_hardware_completion() {
        let mut transaction = SpiTransactionState::new();
        transaction
            .prepare(TimestampMicros(0), IMU_DEADLINE)
            .unwrap();
        transaction.mark_pending().unwrap();

        assert!(transaction.request_cancel());
        assert_eq!(transaction.owner_abort(), Ok(SpiFault::Cancelled));
        assert_eq!(transaction.take_result(), Some(Err(SpiFault::Cancelled)));
    }

    #[test]
    fn old_generation_cannot_complete_a_new_job() {
        let mut transaction = SpiTransactionState::new();
        let old = transaction
            .prepare(TimestampMicros(0), IMU_DEADLINE)
            .unwrap();
        transaction.mark_pending().unwrap();
        assert!(transaction.request_cancel());
        transaction.owner_abort().unwrap();
        let _ = transaction.take_result().unwrap();

        let current = transaction
            .prepare(TimestampMicros(500), IMU_DEADLINE)
            .unwrap();
        transaction.mark_pending().unwrap();
        transaction.activate().unwrap();

        assert_eq!(transaction.complete(old.generation), SpiCompletion::Stale);
        assert_eq!(transaction.state(), SpiJobState::Active);
        assert_eq!(
            transaction.complete(current.generation),
            SpiCompletion::Completed
        );
    }

    #[test]
    fn owner_start_fault_is_terminal_and_invalidates_generation() {
        let mut transaction = SpiTransactionState::new();
        let token = transaction
            .prepare(TimestampMicros(0), IMU_DEADLINE)
            .unwrap();
        transaction.mark_pending().unwrap();

        assert_eq!(
            transaction.owner_fault(token.generation, SpiFault::Unavailable),
            SpiCompletion::Faulted
        );
        assert_eq!(transaction.complete(token.generation), SpiCompletion::Stale);
        assert_eq!(transaction.take_result(), Some(Err(SpiFault::Unavailable)));
    }
}
