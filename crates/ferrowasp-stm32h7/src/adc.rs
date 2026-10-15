//! ADC1 battery observation on the STM32H7, under the shared
//! `Adc1ObservationDma` trait.
//!
//! The HAL drives one regular channel at a time, so each observation reads
//! voltage then current as two blocking one-shot conversions of about 20 us
//! together, then pends ADC1's interrupt so the completion runs where the
//! F4's DMA completion does. No DMA stream is used.

use cortex_m::peripheral::NVIC;
use embedded_hal_02::{self as hal_02, adc::OneShot};
pub use ferrowasp_stm32::adc::{
    Adc1ObservationDma, Adc1Sample, Adc1SampleBuffer, AdcDmaDeliveryError, AdcDmaIrqAction,
    AdcDmaIrqFlags, AdcDmaIrqPlanner,
};
use stm32h7xx_hal::{
    adc::{Adc, AdcSampleTime, Enabled, Resolution},
    pac::{ADC1, Interrupt},
    prelude::*,
    rcc::{CoreClocks, rec},
};

use crate::eh1::CycleDelay;

/// ADC1 with the board's battery voltage and current channels (pins in
/// analog mode), and the buffer the next observation lands in.
pub struct Adc1Observation<VoltageT, CurrentT> {
    adc: Adc<ADC1, Enabled>,
    voltage: VoltageT,
    current: CurrentT,
    reference_mv: u32,
    buffer: Option<Adc1SampleBuffer>,
    sample_ready: bool,
    max_sample: u32,
}

pub struct Adc1ObservationParts<VoltageT, CurrentT> {
    pub transfer: Adc1Observation<VoltageT, CurrentT>,
    pub spare_buffer: Adc1SampleBuffer,
}

/// Start ADC1 reading the board's voltage and current channels. The
/// reference voltage is the board's: what feeds VDDA or VREF+.
#[allow(clippy::too_many_arguments)]
pub fn init_adc1_observation<VoltageT, CurrentT>(
    adc: ADC1,
    prec: rec::Adc12,
    voltage: VoltageT,
    current: CurrentT,
    reference_mv: u32,
    delay: &mut CycleDelay,
    clocks: &CoreClocks,
    primary: Adc1SampleBuffer,
    spare: Adc1SampleBuffer,
) -> Adc1ObservationParts<VoltageT, CurrentT>
where
    VoltageT: hal_02::adc::Channel<ADC1, ID = u8>,
    CurrentT: hal_02::adc::Channel<ADC1, ID = u8>,
{
    let mut adc = Adc::adc1(adc, 4.MHz(), delay, prec, clocks).enable();
    adc.set_resolution(Resolution::SixteenBit);
    adc.set_sample_time(AdcSampleTime::T_64);
    let max_sample = adc.slope() - 1;

    Adc1ObservationParts {
        transfer: Adc1Observation {
            adc,
            voltage,
            current,
            reference_mv,
            buffer: Some(primary),
            sample_ready: false,
            max_sample,
        },
        spare_buffer: spare,
    }
}

impl<VoltageT, CurrentT> Adc1Observation<VoltageT, CurrentT> {
    fn to_millivolts(&self, sample: u16) -> u16 {
        (u32::from(sample) * self.reference_mv / self.max_sample.max(1)) as u16
    }
}

impl<VoltageT, CurrentT> Adc1ObservationDma for Adc1Observation<VoltageT, CurrentT>
where
    VoltageT: hal_02::adc::Channel<ADC1, ID = u8>,
    CurrentT: hal_02::adc::Channel<ADC1, ID = u8>,
{
    fn start_conversion(&mut self) {
        let Some(buffer) = self.buffer.as_deref_mut() else {
            return;
        };
        let voltage: Result<u32, _> = self.adc.read(&mut self.voltage);
        let current: Result<u32, _> = self.adc.read(&mut self.current);
        let (Ok(voltage), Ok(current)) = (voltage, current) else {
            return;
        };
        // Same layout as the F4's scan: word 0 is unused here.
        buffer[0] = 0;
        buffer[1] = voltage as u16;
        buffer[2] = current as u16;
        self.sample_ready = true;
        NVIC::pend(Interrupt::ADC1_2);
    }

    fn take_completed_sample(
        &mut self,
        spare_buffer: &mut Option<Adc1SampleBuffer>,
        planner: &mut AdcDmaIrqPlanner,
    ) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError> {
        let action = planner.handle(AdcDmaIrqFlags {
            transfer_complete: self.sample_ready,
            dma_error: false,
        });
        match action {
            AdcDmaIrqAction::None => Ok(None),
            AdcDmaIrqAction::Fault(_) => Err(AdcDmaDeliveryError::DmaFault),
            AdcDmaIrqAction::SampleReady => {
                let Some(next_buffer) = spare_buffer.take() else {
                    return Err(AdcDmaDeliveryError::NoSpareBuffer);
                };
                self.sample_ready = false;
                let Some(buffer) = self.buffer.replace(next_buffer) else {
                    return Err(AdcDmaDeliveryError::TransferNotReady);
                };
                let voltage_mv = self.to_millivolts(buffer[1]);
                let current_mv = self.to_millivolts(buffer[2]);
                Ok(Some(Adc1Sample {
                    buffer,
                    voltage_mv,
                    current_mv,
                }))
            }
        }
    }
}
