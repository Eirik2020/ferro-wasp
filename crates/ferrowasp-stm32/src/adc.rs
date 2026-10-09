#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdcDmaFault {
    Transfer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdcDmaIrqFlags {
    pub transfer_complete: bool,
    pub dma_error: bool,
}

impl AdcDmaIrqFlags {
    pub const NONE: Self = Self {
        transfer_complete: false,
        dma_error: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdcDmaIrqAction {
    None,
    SampleReady,
    Fault(AdcDmaFault),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdcDmaIrqStats {
    pub samples: u32,
    pub dma_errors: u32,
    pub ignored_events: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdcDmaIrqPlanner {
    stats: AdcDmaIrqStats,
}

impl AdcDmaIrqPlanner {
    pub const fn new() -> Self {
        Self {
            stats: AdcDmaIrqStats {
                samples: 0,
                dma_errors: 0,
                ignored_events: 0,
            },
        }
    }

    pub const fn stats(self) -> AdcDmaIrqStats {
        self.stats
    }

    pub fn handle(&mut self, flags: AdcDmaIrqFlags) -> AdcDmaIrqAction {
        let action = if flags.dma_error {
            AdcDmaIrqAction::Fault(AdcDmaFault::Transfer)
        } else if flags.transfer_complete {
            AdcDmaIrqAction::SampleReady
        } else {
            AdcDmaIrqAction::None
        };

        match action {
            AdcDmaIrqAction::SampleReady => {
                self.stats.samples = self.stats.samples.saturating_add(1);
            }
            AdcDmaIrqAction::Fault(_) => {
                self.stats.dma_errors = self.stats.dma_errors.saturating_add(1);
            }
            AdcDmaIrqAction::None => {
                self.stats.ignored_events = self.stats.ignored_events.saturating_add(1);
            }
        }

        action
    }
}

/// ADC1's DMA sample buffer. Which conversion lands in which word is the
/// backend's; `Adc1Sample` carries the converted readings.
pub type Adc1SampleBuffer = &'static mut [u16; 3];

pub struct Adc1Sample {
    pub buffer: Adc1SampleBuffer,
    pub voltage_mv: u16,
    pub current_mv: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdcDmaDeliveryError {
    DmaFault,
    NoSpareBuffer,
    TransferNotReady,
}

/// ADC1's observation transfer, whichever DMA stream a board gives it. What a
/// shared task definition bounds on; each backend forwards to its own
/// transfer.
pub trait Adc1ObservationDma {
    /// Start one conversion sequence into the current buffer.
    fn start_conversion(&mut self);

    fn take_completed_sample(
        &mut self,
        spare_buffer: &mut Option<Adc1SampleBuffer>,
        planner: &mut AdcDmaIrqPlanner,
    ) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_flags_are_ignored() {
        let mut planner = AdcDmaIrqPlanner::new();

        assert_eq!(planner.handle(AdcDmaIrqFlags::NONE), AdcDmaIrqAction::None);
        assert_eq!(planner.stats().ignored_events, 1);
    }

    #[test]
    fn transfer_complete_produces_sample_ready() {
        let mut planner = AdcDmaIrqPlanner::new();

        assert_eq!(
            planner.handle(AdcDmaIrqFlags {
                transfer_complete: true,
                dma_error: false,
            }),
            AdcDmaIrqAction::SampleReady
        );
        assert_eq!(planner.stats().samples, 1);
    }

    #[test]
    fn dma_error_preempts_sample_ready() {
        let mut planner = AdcDmaIrqPlanner::new();

        assert_eq!(
            planner.handle(AdcDmaIrqFlags {
                transfer_complete: true,
                dma_error: true,
            }),
            AdcDmaIrqAction::Fault(AdcDmaFault::Transfer)
        );
        assert_eq!(planner.stats().dma_errors, 1);
        assert_eq!(planner.stats().samples, 0);
    }
}
