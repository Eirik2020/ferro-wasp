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

#[cfg(target_arch = "arm")]
use stm32f4xx_hal::{
    ClearFlags, ReadFlags,
    adc::{
        Adc, Temperature,
        config::{AdcConfig, Dma, SampleTime, Scan, Sequence},
    },
    dma::{
        ChannelX, PeripheralToMemory, Stream0, Transfer,
        config::DmaConfig,
        traits::{Channel, DMASet, DmaFlagExt, Stream},
    },
    gpio::{Input, PC0, PC1},
    pac::{ADC1, DMA2},
    rcc::Rcc,
};

#[cfg(target_arch = "arm")]
pub type Adc1SampleBuffer = &'static mut [u16; 3];
#[cfg(target_arch = "arm")]
pub type Adc1ObservationTransferFor<StreamT, const CHANNEL: u8> =
    Transfer<StreamT, CHANNEL, Adc<ADC1>, PeripheralToMemory, Adc1SampleBuffer>;
#[cfg(target_arch = "arm")]
pub type Adc1ObservationTransfer = Adc1ObservationTransferFor<Stream0<DMA2>, 0>;

#[cfg(target_arch = "arm")]
pub struct Adc1Sample {
    pub buffer: Adc1SampleBuffer,
    pub voltage_mv: u16,
    pub current_mv: u16,
}

#[cfg(target_arch = "arm")]
pub struct Adc1BatteryResources {
    pub adc: ADC1,
    pub voltage_pin: PC0<Input>,
    pub current_pin: PC1<Input>,
    pub dma: Stream0<DMA2>,
}

#[cfg(target_arch = "arm")]
pub struct Adc1ObservationPartsFor<StreamT, const CHANNEL: u8>
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    Adc<ADC1>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    pub transfer: Adc1ObservationTransferFor<StreamT, CHANNEL>,
    pub spare_buffer: Adc1SampleBuffer,
}

#[cfg(target_arch = "arm")]
pub type Adc1ObservationParts = Adc1ObservationPartsFor<Stream0<DMA2>, 0>;

#[cfg(target_arch = "arm")]
pub fn init_adc1_observation(
    adc: ADC1,
    voltage_pin: PC0<Input>,
    current_pin: PC1<Input>,
    dma: Stream0<DMA2>,
    rcc: &mut Rcc,
    primary_buffer: Adc1SampleBuffer,
    spare_buffer: Adc1SampleBuffer,
) -> Adc1ObservationParts {
    init_adc1_observation_for::<_, 0>(
        adc,
        voltage_pin,
        current_pin,
        dma,
        rcc,
        primary_buffer,
        spare_buffer,
    )
}

#[cfg(target_arch = "arm")]
pub fn init_adc1_observation_for<StreamT, const CHANNEL: u8>(
    adc: ADC1,
    voltage_pin: PC0<Input>,
    current_pin: PC1<Input>,
    dma: StreamT,
    rcc: &mut Rcc,
    primary_buffer: Adc1SampleBuffer,
    spare_buffer: Adc1SampleBuffer,
) -> Adc1ObservationPartsFor<StreamT, CHANNEL>
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    Adc<ADC1>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    let adc_config = AdcConfig::default().dma(Dma::Single).scan(Scan::Enabled);
    let mut adc = Adc::new(adc, true, adc_config, rcc);
    let voltage = voltage_pin.into_analog();
    let current = current_pin.into_analog();

    adc.configure_channel(&Temperature, Sequence::One, SampleTime::Cycles_480);
    adc.configure_channel(&voltage, Sequence::Two, SampleTime::Cycles_480);
    adc.configure_channel(&current, Sequence::Three, SampleTime::Cycles_480);
    adc.enable_temperature_and_vref();

    let dma_config = DmaConfig::default()
        .transfer_complete_interrupt(true)
        .memory_increment(true)
        .double_buffer(false);
    let transfer = Transfer::init_peripheral_to_memory(dma, adc, primary_buffer, None, dma_config);

    Adc1ObservationPartsFor {
        transfer,
        spare_buffer,
    }
}

#[cfg(target_arch = "arm")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdcDmaDeliveryError {
    DmaFault,
    NoSpareBuffer,
    TransferNotReady,
}

#[cfg(target_arch = "arm")]
pub fn take_completed_adc1_sample(
    transfer: &mut Adc1ObservationTransfer,
    spare_buffer: &mut Option<Adc1SampleBuffer>,
    planner: &mut AdcDmaIrqPlanner,
) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError> {
    take_completed_adc1_sample_for(transfer, spare_buffer, planner)
}

#[cfg(target_arch = "arm")]
pub fn take_completed_adc1_sample_for<StreamT, const CHANNEL: u8>(
    transfer: &mut Adc1ObservationTransferFor<StreamT, CHANNEL>,
    spare_buffer: &mut Option<Adc1SampleBuffer>,
    planner: &mut AdcDmaIrqPlanner,
) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError>
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    Adc<ADC1>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    let flags = transfer.flags();
    let action = planner.handle(AdcDmaIrqFlags {
        transfer_complete: flags.is_transfer_complete(),
        dma_error: flags.is_transfer_error()
            || flags.is_direct_mode_error()
            || flags.is_fifo_error(),
    });

    match action {
        AdcDmaIrqAction::None => Ok(None),
        AdcDmaIrqAction::Fault(_) => {
            transfer.clear_all_flags();
            Err(AdcDmaDeliveryError::DmaFault)
        }
        AdcDmaIrqAction::SampleReady => {
            let Some(next_buffer) = spare_buffer.take() else {
                transfer.clear_all_flags();
                return Err(AdcDmaDeliveryError::NoSpareBuffer);
            };

            let (buffer, _) = transfer
                .next_transfer(next_buffer)
                .map_err(|_| AdcDmaDeliveryError::TransferNotReady)?;

            let sample_to_millivolts = transfer.peripheral().make_sample_to_millivolts();
            let voltage_mv = sample_to_millivolts(buffer[1]);
            let current_mv = sample_to_millivolts(buffer[2]);
            transfer.clear_all_flags();
            Ok(Some(Adc1Sample {
                buffer,
                voltage_mv,
                current_mv,
            }))
        }
    }
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
