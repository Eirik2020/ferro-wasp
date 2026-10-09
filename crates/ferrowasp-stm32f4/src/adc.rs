//! ADC1 battery and current observation on the STM32F4: the HAL transfer
//! under the chip-neutral planner and sample types.

pub use ferrowasp_stm32::adc::*;
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

/// ADC1's observation DMA transfer. A type of this crate's own, so it can
/// carry the chip-neutral `Adc1ObservationDma`.
pub struct Adc1ObservationTransferFor<StreamT, const CHANNEL: u8>(
    pub Transfer<StreamT, CHANNEL, Adc<ADC1>, PeripheralToMemory, Adc1SampleBuffer>,
)
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    Adc<ADC1>: DMASet<StreamT, CHANNEL, PeripheralToMemory>;
pub type Adc1ObservationTransfer = Adc1ObservationTransferFor<Stream0<DMA2>, 0>;

pub struct Adc1BatteryResources {
    pub adc: ADC1,
    pub voltage_pin: PC0<Input>,
    pub current_pin: PC1<Input>,
    pub dma: Stream0<DMA2>,
}

pub struct Adc1ObservationPartsFor<StreamT, const CHANNEL: u8>
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    Adc<ADC1>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    pub transfer: Adc1ObservationTransferFor<StreamT, CHANNEL>,
    pub spare_buffer: Adc1SampleBuffer,
}

pub type Adc1ObservationParts = Adc1ObservationPartsFor<Stream0<DMA2>, 0>;

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
    let transfer = Adc1ObservationTransferFor(Transfer::init_peripheral_to_memory(
        dma,
        adc,
        primary_buffer,
        None,
        dma_config,
    ));

    Adc1ObservationPartsFor {
        transfer,
        spare_buffer,
    }
}

pub fn take_completed_adc1_sample(
    transfer: &mut Adc1ObservationTransfer,
    spare_buffer: &mut Option<Adc1SampleBuffer>,
    planner: &mut AdcDmaIrqPlanner,
) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError> {
    take_completed_adc1_sample_for(transfer, spare_buffer, planner)
}

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
    let transfer = &mut transfer.0;
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

impl<StreamT, const CHANNEL: u8> Adc1ObservationDma for Adc1ObservationTransferFor<StreamT, CHANNEL>
where
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    Adc<ADC1>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    fn start_conversion(&mut self) {
        Transfer::start(&mut self.0, |adc| {
            adc.start_conversion();
        })
    }

    fn take_completed_sample(
        &mut self,
        spare_buffer: &mut Option<Adc1SampleBuffer>,
        planner: &mut AdcDmaIrqPlanner,
    ) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError> {
        take_completed_adc1_sample_for(self, spare_buffer, planner)
    }
}
