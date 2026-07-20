use ferrowasp_io_core::serial::StreamGeneration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxBufferId {
    A,
    B,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxDetachCause {
    Full,
    Idle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DetachedRxBuffer {
    pub buffer: RxBufferId,
    pub len: usize,
    pub generation: StreamGeneration,
    pub cause: RxDetachCause,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxEventOutcome {
    Detached(DetachedRxBuffer),
    Stale,
    EmptyIdle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RxDmaState {
    active: RxBufferId,
    generation: StreamGeneration,
    capacity: usize,
    last_detached_generation: StreamGeneration,
}

impl RxDmaState {
    pub const fn new(capacity: usize) -> Self {
        Self {
            active: RxBufferId::A,
            generation: StreamGeneration(0),
            capacity,
            last_detached_generation: StreamGeneration(u32::MAX),
        }
    }

    pub const fn active_buffer(self) -> RxBufferId {
        self.active
    }

    pub const fn generation(self) -> StreamGeneration {
        self.generation
    }

    pub fn abort_generation(&mut self) {
        self.last_detached_generation = self.generation;
        self.generation = self.generation.next();
    }

    pub fn on_dma_full(&mut self, observed_generation: StreamGeneration) -> RxEventOutcome {
        if observed_generation != self.generation {
            return RxEventOutcome::Stale;
        }

        self.detach(self.capacity, RxDetachCause::Full)
    }

    pub fn on_idle(
        &mut self,
        observed_generation: StreamGeneration,
        received_len: usize,
    ) -> RxEventOutcome {
        if observed_generation != self.generation {
            return RxEventOutcome::Stale;
        }

        if received_len == 0 {
            return RxEventOutcome::EmptyIdle;
        }

        self.detach(received_len.min(self.capacity), RxDetachCause::Idle)
    }

    fn detach(&mut self, len: usize, cause: RxDetachCause) -> RxEventOutcome {
        let detached = DetachedRxBuffer {
            buffer: self.active,
            len,
            generation: self.generation,
            cause,
        };

        self.last_detached_generation = self.generation;
        self.generation = self.generation.next();
        self.active = match self.active {
            RxBufferId::A => RxBufferId::B,
            RxBufferId::B => RxBufferId::A,
        };

        RxEventOutcome::Detached(detached)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_buffer_detaches_and_advances_generation() {
        let mut state = RxDmaState::new(64);
        let outcome = state.on_dma_full(StreamGeneration(0));

        assert_eq!(
            outcome,
            RxEventOutcome::Detached(DetachedRxBuffer {
                buffer: RxBufferId::A,
                len: 64,
                generation: StreamGeneration(0),
                cause: RxDetachCause::Full,
            })
        );
        assert_eq!(state.active_buffer(), RxBufferId::B);
        assert_eq!(state.generation(), StreamGeneration(1));
    }

    #[test]
    fn idle_detaches_partial_buffer_and_suppresses_late_full_irq() {
        let mut state = RxDmaState::new(64);
        let outcome = state.on_idle(StreamGeneration(0), 9);

        assert!(matches!(outcome, RxEventOutcome::Detached(_)));
        assert_eq!(state.generation(), StreamGeneration(1));
        assert_eq!(
            state.on_dma_full(StreamGeneration(0)),
            RxEventOutcome::Stale
        );
    }

    #[test]
    fn full_irq_suppresses_late_idle_for_old_generation() {
        let mut state = RxDmaState::new(64);
        let outcome = state.on_dma_full(StreamGeneration(0));

        assert!(matches!(outcome, RxEventOutcome::Detached(_)));
        assert_eq!(
            state.on_idle(StreamGeneration(0), 12),
            RxEventOutcome::Stale
        );
    }

    #[test]
    fn empty_idle_does_not_advance_generation() {
        let mut state = RxDmaState::new(64);

        assert_eq!(
            state.on_idle(StreamGeneration(0), 0),
            RxEventOutcome::EmptyIdle
        );
        assert_eq!(state.generation(), StreamGeneration(0));
        assert_eq!(state.active_buffer(), RxBufferId::A);
    }

    #[test]
    fn abort_generation_suppresses_late_events_without_flipping_active_buffer() {
        let mut state = RxDmaState::new(64);

        state.abort_generation();

        assert_eq!(state.generation(), StreamGeneration(1));
        assert_eq!(state.active_buffer(), RxBufferId::A);
        assert_eq!(
            state.on_dma_full(StreamGeneration(0)),
            RxEventOutcome::Stale
        );
    }

    #[test]
    fn partial_idle_len_is_clamped_to_capacity() {
        let mut state = RxDmaState::new(64);
        let outcome = state.on_idle(StreamGeneration(0), 100);

        assert_eq!(
            outcome,
            RxEventOutcome::Detached(DetachedRxBuffer {
                buffer: RxBufferId::A,
                len: 64,
                generation: StreamGeneration(0),
                cause: RxDetachCause::Idle,
            })
        );
    }
}
