//! ADC1 battery observation on the STM32H7, under the shared
//! `Adc1ObservationDma` trait.
//!
//! The HAL drives one regular channel at a time, so each observation reads
//! voltage then current as two blocking one-shot conversions of about 20 us
//! together, then pends ADC1's interrupt so the completion runs where the
//! F4's DMA completion does. No DMA stream is used.

use cortex_m::peripheral::NVIC;
use embedded_hal_02::adc::OneShot;
pub use ferrowasp_stm32::adc::{
    Adc1ObservationDma, Adc1Sample, Adc1SampleBuffer, AdcDmaDeliveryError, AdcDmaIrqAction,
    AdcDmaIrqFlags, AdcDmaIrqPlanner,
};
use stm32h7xx_hal::{
    adc::{Adc, AdcSampleTime, Enabled, Resolution},
    gpio::{Analog, PC0, PC1},
    pac::{ADC1, Interrupt},
    prelude::*,
    rcc::{CoreClocks, rec},
};

use crate::eh1::CycleDelay;

/// The ADC reference on the Lucid: VDDA from the 3.3 V rail. Not
/// calibrated against VREFINT.
pub const ADC_REFERENCE_MV: u32 = 3_300;

pub struct Adc1BatteryResources {
    pub adc: ADC1,
    pub prec: rec::Adc12,
    pub voltage_pin: PC0,
    pub current_pin: PC1,
}

/// ADC1 with the battery voltage and current pins, and the buffer the next
/// observation lands in.
pub struct Adc1Observation {
    adc: Adc<ADC1, Enabled>,
    voltage_pin: PC0<Analog>,
    current_pin: PC1<Analog>,
    buffer: Option<Adc1SampleBuffer>,
    sample_ready: bool,
    max_sample: u32,
}

pub struct Adc1ObservationParts {
    pub transfer: Adc1Observation,
    pub spare_buffer: Adc1SampleBuffer,
}

pub fn init_adc1_observation(
    resources: Adc1BatteryResources,
    delay: &mut CycleDelay,
    clocks: &CoreClocks,
    primary: Adc1SampleBuffer,
    spare: Adc1SampleBuffer,
) -> Adc1ObservationParts {
    let mut adc = Adc::adc1(resources.adc, 4.MHz(), delay, resources.prec, clocks).enable();
    adc.set_resolution(Resolution::SixteenBit);
    adc.set_sample_time(AdcSampleTime::T_64);
    let max_sample = adc.slope() - 1;

    Adc1ObservationParts {
        transfer: Adc1Observation {
            adc,
            voltage_pin: resources.voltage_pin.into_analog(),
            current_pin: resources.current_pin.into_analog(),
            buffer: Some(primary),
            sample_ready: false,
            max_sample,
        },
        spare_buffer: spare,
    }
}

impl Adc1Observation {
    fn to_millivolts(&self, sample: u16) -> u16 {
        (u32::from(sample) * ADC_REFERENCE_MV / self.max_sample.max(1)) as u16
    }
}

impl Adc1ObservationDma for Adc1Observation {
    fn start_conversion(&mut self) {
        let Some(buffer) = self.buffer.as_deref_mut() else {
            return;
        };
        let voltage: Result<u32, _> = self.adc.read(&mut self.voltage_pin);
        let current: Result<u32, _> = self.adc.read(&mut self.current_pin);
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
