#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TxDmaLifecycle {
    in_flight: bool,
    completed_chunks: u32,
    dma_errors: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxDmaBeginError {
    Busy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxDmaFinish {
    Ignored,
    Completed,
    DmaError,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TxDmaIrqFlags {
    pub transfer_complete: bool,
    pub transfer_error: bool,
    pub direct_mode_error: bool,
    pub fifo_error: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxDmaIrqTerminal {
    None,
    Completed,
    TransferError,
    DirectModeError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TxDmaIrqPlan {
    pub clear_fifo_error: bool,
    pub terminal: TxDmaIrqTerminal,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TxDmaLifecycleStats {
    pub completed_chunks: u32,
    pub dma_errors: u32,
}

impl TxDmaLifecycle {
    pub const fn new() -> Self {
        Self {
            in_flight: false,
            completed_chunks: 0,
            dma_errors: 0,
        }
    }

    pub const fn is_in_flight(&self) -> bool {
        self.in_flight
    }

    pub fn begin(&mut self) -> Result<(), TxDmaBeginError> {
        if self.in_flight {
            return Err(TxDmaBeginError::Busy);
        }
        self.in_flight = true;
        Ok(())
    }

    pub fn complete(&mut self) -> TxDmaFinish {
        if !self.in_flight {
            return TxDmaFinish::Ignored;
        }
        self.in_flight = false;
        self.completed_chunks = self.completed_chunks.saturating_add(1);
        TxDmaFinish::Completed
    }

    pub fn dma_error(&mut self) -> TxDmaFinish {
        if !self.in_flight {
            return TxDmaFinish::Ignored;
        }
        self.in_flight = false;
        self.dma_errors = self.dma_errors.saturating_add(1);
        TxDmaFinish::DmaError
    }

    pub const fn stats(&self) -> TxDmaLifecycleStats {
        TxDmaLifecycleStats {
            completed_chunks: self.completed_chunks,
            dma_errors: self.dma_errors,
        }
    }
}

pub const fn plan_tx_dma_irq(flags: TxDmaIrqFlags) -> TxDmaIrqPlan {
    let terminal = if flags.transfer_error {
        TxDmaIrqTerminal::TransferError
    } else if flags.direct_mode_error {
        TxDmaIrqTerminal::DirectModeError
    } else if flags.transfer_complete {
        TxDmaIrqTerminal::Completed
    } else {
        TxDmaIrqTerminal::None
    };

    TxDmaIrqPlan {
        clear_fifo_error: flags.fifo_error,
        terminal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_transfer_has_one_terminal_completion() {
        let mut lifecycle = TxDmaLifecycle::new();

        assert_eq!(lifecycle.begin(), Ok(()));
        assert!(lifecycle.is_in_flight());
        assert_eq!(lifecycle.complete(), TxDmaFinish::Completed);
        assert!(!lifecycle.is_in_flight());
        assert_eq!(lifecycle.complete(), TxDmaFinish::Ignored);
        assert_eq!(lifecycle.stats().completed_chunks, 1);
    }

    #[test]
    fn overlapping_transfer_is_rejected() {
        let mut lifecycle = TxDmaLifecycle::new();

        assert_eq!(lifecycle.begin(), Ok(()));
        assert_eq!(lifecycle.begin(), Err(TxDmaBeginError::Busy));
        assert_eq!(lifecycle.dma_error(), TxDmaFinish::DmaError);
        assert_eq!(lifecycle.stats().dma_errors, 1);
    }

    #[test]
    fn stale_error_is_ignored_and_does_not_poison_next_transfer() {
        let mut lifecycle = TxDmaLifecycle::new();

        assert_eq!(lifecycle.dma_error(), TxDmaFinish::Ignored);
        assert_eq!(lifecycle.stats().dma_errors, 0);
        assert_eq!(lifecycle.begin(), Ok(()));
        assert_eq!(lifecycle.complete(), TxDmaFinish::Completed);
    }

    #[test]
    fn fifo_error_is_recoverable_and_does_not_finish_transfer() {
        assert_eq!(
            plan_tx_dma_irq(TxDmaIrqFlags {
                fifo_error: true,
                ..TxDmaIrqFlags::default()
            }),
            TxDmaIrqPlan {
                clear_fifo_error: true,
                terminal: TxDmaIrqTerminal::None,
            }
        );
    }

    #[test]
    fn fifo_flag_does_not_hide_simultaneous_completion() {
        assert_eq!(
            plan_tx_dma_irq(TxDmaIrqFlags {
                transfer_complete: true,
                fifo_error: true,
                ..TxDmaIrqFlags::default()
            }),
            TxDmaIrqPlan {
                clear_fifo_error: true,
                terminal: TxDmaIrqTerminal::Completed,
            }
        );
    }

    #[test]
    fn transfer_error_preempts_completion() {
        assert_eq!(
            plan_tx_dma_irq(TxDmaIrqFlags {
                transfer_complete: true,
                transfer_error: true,
                ..TxDmaIrqFlags::default()
            })
            .terminal,
            TxDmaIrqTerminal::TransferError
        );
    }
}
