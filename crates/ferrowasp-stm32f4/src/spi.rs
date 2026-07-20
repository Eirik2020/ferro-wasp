use ferrowasp_io_core::spi::{SpiFault, TransactionId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiRxIrqFlags {
    pub transfer_complete: bool,
    pub dma_error: bool,
}

impl SpiRxIrqFlags {
    pub const NONE: Self = Self {
        transfer_complete: false,
        dma_error: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiRxIrqAction {
    None,
    Deliver { transaction: TransactionId },
    Fault(SpiFault),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiRxIrqPlanner {
    next_transaction: TransactionId,
}

impl SpiRxIrqPlanner {
    pub const fn new() -> Self {
        Self {
            next_transaction: TransactionId(0),
        }
    }

    pub const fn next_transaction(self) -> TransactionId {
        self.next_transaction
    }

    pub fn handle(&mut self, flags: SpiRxIrqFlags) -> SpiRxIrqAction {
        if flags.dma_error {
            return SpiRxIrqAction::Fault(SpiFault::DmaTransfer);
        }

        if flags.transfer_complete {
            let transaction = self.next_transaction;
            self.next_transaction = self.next_transaction.next();
            return SpiRxIrqAction::Deliver { transaction };
        }

        SpiRxIrqAction::None
    }
}

impl Default for SpiRxIrqPlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_flags_have_no_action() {
        let mut planner = SpiRxIrqPlanner::new();

        assert_eq!(planner.handle(SpiRxIrqFlags::NONE), SpiRxIrqAction::None);
        assert_eq!(planner.next_transaction(), TransactionId(0));
    }

    #[test]
    fn transfer_complete_delivers_and_advances_transaction() {
        let mut planner = SpiRxIrqPlanner::new();

        assert_eq!(
            planner.handle(SpiRxIrqFlags {
                transfer_complete: true,
                dma_error: false,
            }),
            SpiRxIrqAction::Deliver {
                transaction: TransactionId(0)
            }
        );
        assert_eq!(planner.next_transaction(), TransactionId(1));
    }

    #[test]
    fn dma_error_reports_fault_without_advancing_transaction() {
        let mut planner = SpiRxIrqPlanner::new();

        assert_eq!(
            planner.handle(SpiRxIrqFlags {
                transfer_complete: true,
                dma_error: true,
            }),
            SpiRxIrqAction::Fault(SpiFault::DmaTransfer)
        );
        assert_eq!(planner.next_transaction(), TransactionId(0));
    }
}
