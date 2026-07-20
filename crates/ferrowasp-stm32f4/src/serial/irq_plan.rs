use super::rx_state::{DetachedRxBuffer, RxDmaState, RxEventOutcome};
use ferrowasp_io_core::serial::{SerialFault, StreamGeneration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RxIrqFlags {
    pub observed_generation: StreamGeneration,
    pub dma_full: bool,
    pub dma_error: bool,
    pub idle: bool,
    pub received_len: usize,
}

impl RxIrqFlags {
    pub const fn none(observed_generation: StreamGeneration) -> Self {
        Self {
            observed_generation,
            dma_full: false,
            dma_error: false,
            idle: false,
            received_len: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxIrqAction {
    None,
    ClearIdle,
    Deliver(DetachedRxBuffer),
    Stale,
    Fault(SerialFault),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RxIrqPlanner {
    state: RxDmaState,
}

impl RxIrqPlanner {
    pub const fn new(buffer_capacity: usize) -> Self {
        Self {
            state: RxDmaState::new(buffer_capacity),
        }
    }

    pub const fn state(self) -> RxDmaState {
        self.state
    }

    pub fn handle(&mut self, flags: RxIrqFlags) -> RxIrqAction {
        if flags.dma_error {
            self.state.abort_generation();
            return RxIrqAction::Fault(SerialFault::DmaTransfer);
        }

        if flags.dma_full {
            return action_from_outcome(self.state.on_dma_full(flags.observed_generation));
        }

        if flags.idle {
            return action_from_outcome(
                self.state
                    .on_idle(flags.observed_generation, flags.received_len),
            );
        }

        RxIrqAction::None
    }
}

fn action_from_outcome(outcome: RxEventOutcome) -> RxIrqAction {
    match outcome {
        RxEventOutcome::Detached(buffer) => RxIrqAction::Deliver(buffer),
        RxEventOutcome::Stale => RxIrqAction::Stale,
        RxEventOutcome::EmptyIdle => RxIrqAction::ClearIdle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial::rx_state::{RxBufferId, RxDetachCause};

    #[test]
    fn no_flags_produce_no_action() {
        let mut planner = RxIrqPlanner::new(32);

        assert_eq!(
            planner.handle(RxIrqFlags::none(StreamGeneration(0))),
            RxIrqAction::None
        );
    }

    #[test]
    fn dma_full_is_delivered_before_idle_when_both_flags_are_seen() {
        let mut planner = RxIrqPlanner::new(32);
        let action = planner.handle(RxIrqFlags {
            observed_generation: StreamGeneration(0),
            dma_full: true,
            dma_error: false,
            idle: true,
            received_len: 7,
        });

        assert_eq!(
            action,
            RxIrqAction::Deliver(DetachedRxBuffer {
                buffer: RxBufferId::A,
                len: 32,
                generation: StreamGeneration(0),
                cause: RxDetachCause::Full,
            })
        );
    }

    #[test]
    fn idle_partial_delivery_records_received_length() {
        let mut planner = RxIrqPlanner::new(32);
        let action = planner.handle(RxIrqFlags {
            observed_generation: StreamGeneration(0),
            dma_full: false,
            dma_error: false,
            idle: true,
            received_len: 7,
        });

        assert_eq!(
            action,
            RxIrqAction::Deliver(DetachedRxBuffer {
                buffer: RxBufferId::A,
                len: 7,
                generation: StreamGeneration(0),
                cause: RxDetachCause::Idle,
            })
        );
    }

    #[test]
    fn dma_error_advances_generation_and_reports_fault() {
        let mut planner = RxIrqPlanner::new(32);

        assert_eq!(
            planner.handle(RxIrqFlags {
                observed_generation: StreamGeneration(0),
                dma_full: false,
                dma_error: true,
                idle: false,
                received_len: 0,
            }),
            RxIrqAction::Fault(SerialFault::DmaTransfer)
        );

        assert_eq!(planner.state().generation(), StreamGeneration(1));
        assert_eq!(
            planner.handle(RxIrqFlags {
                observed_generation: StreamGeneration(0),
                dma_full: true,
                dma_error: false,
                idle: false,
                received_len: 0,
            }),
            RxIrqAction::Stale
        );
    }
}
