//! ADC1 battery and current observation through FerroHAL's sequence over DMA, under
//! the chip-neutral planner and sample types.

use ferrohal_stm32::adc::{AdcScan, Instance};
pub use ferrowasp_stm32::adc::*;

/// The sequence, and what its samples are worth.
pub struct Adc1ObservationTransfer<A: Instance> {
    scan: AdcScan<A>,
    /// Where each role sits in the sequence.
    voltage: usize,
    current: usize,
    vdda_mv: u32,
    max_sample: u32,
}

impl<A: Instance> Adc1ObservationTransfer<A> {
    pub const fn new(
        scan: AdcScan<A>,
        voltage: usize,
        current: usize,
        vdda_mv: u32,
        max_sample: u16,
    ) -> Self {
        Self {
            scan,
            voltage,
            current,
            vdda_mv,
            max_sample: max_sample as u32,
        }
    }

    fn millivolts(&self, sample: u16) -> u16 {
        let mv = u32::from(sample) * self.vdda_mv / self.max_sample.max(1);
        u16::try_from(mv).unwrap_or(u16::MAX)
    }
}

impl<A: Instance> Adc1ObservationDma for Adc1ObservationTransfer<A> {
    fn start_conversion(&mut self) {
        // A sequence still running is a late poll; the next one starts on time.
        let _ = self.scan.start();
    }

    fn take_completed_sample(
        &mut self,
        spare_buffer: &mut Option<Adc1SampleBuffer>,
        planner: &mut AdcDmaIrqPlanner,
    ) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError> {
        let (complete, failed) = match self.scan.on_interrupt() {
            None => (false, false),
            Some(Ok(_)) => (true, false),
            Some(Err(_)) => (false, true),
        };
        match planner.handle(AdcDmaIrqFlags {
            transfer_complete: complete,
            dma_error: failed,
        }) {
            AdcDmaIrqAction::None => Ok(None),
            AdcDmaIrqAction::Fault(_) => Err(AdcDmaDeliveryError::DmaFault),
            AdcDmaIrqAction::SampleReady => {
                let Some(buffer) = spare_buffer.take() else {
                    return Err(AdcDmaDeliveryError::NoSpareBuffer);
                };
                let samples = self.scan.samples();
                for (to, from) in buffer.iter_mut().zip(samples) {
                    *to = *from;
                }
                let at = |index: usize| samples.get(index).copied().unwrap_or(0);
                let (voltage, current) = (at(self.voltage), at(self.current));
                Ok(Some(Adc1Sample {
                    buffer,
                    voltage_mv: self.millivolts(voltage),
                    current_mv: self.millivolts(current),
                }))
            }
        }
    }
}
