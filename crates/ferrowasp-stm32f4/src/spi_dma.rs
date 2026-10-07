pub use crate::spi_common::*;
use stm32f4xx_hal::{
    dma::{
        ChannelX, MemoryToPeripheral, PeripheralToMemory, Stream2, Stream3, Transfer,
        config::DmaConfig,
        traits::{Channel, DMASet, Stream},
    },
    gpio::{Input, Output, PA4, PA5, PA6, PA7, PushPull},
    pac::{DMA2, SPI1},
    prelude::*,
    rcc::Rcc,
    spi::{self, Spi},
};

pub type SpiRxTransfer<RxStream, SpiT, const CHANNEL: u8> =
    Transfer<RxStream, CHANNEL, spi::Rx<SpiT>, PeripheralToMemory, SpiRxBuf>;

pub type SpiTxTransfer<TxStream, SpiT, const CHANNEL: u8> =
    Transfer<TxStream, CHANNEL, spi::Tx<SpiT>, MemoryToPeripheral, SpiTxBuf>;

pub type Spi1RxTransfer = SpiRxTransfer<Stream2<DMA2>, SPI1, 3>;
pub type Spi1TxTransfer = SpiTxTransfer<Stream3<DMA2>, SPI1, 3>;
pub type Spi1ImuCs = PA4<Output<PushPull>>;
pub type Spi1ImuBus = Spi<SPI1>;
pub type Spi1Mpu6500Cs = Spi1ImuCs;
pub type Spi1Mpu6500Bus = Spi1ImuBus;

/// An STM32F4 SPI DMA owner: the shared owner over the HAL's transfers.
pub type SpiDmaOwner<RxTransferT, TxTransferT, CsT> =
    crate::spi_common::SpiDmaOwner<RxTransferT, SpiPollerSide<TxTransferT, DmaConfig>, CsT>;
pub type Spi1Mpu6500Owner = SpiDmaOwner<Spi1RxTransfer, Spi1TxTransfer, Spi1Mpu6500Cs>;

pub struct Spi1Mpu6500Resources {
    pub cs_pin: PA4<Input>,
    pub sck_pin: PA5<Input>,
    pub miso_pin: PA6<Input>,
    pub mosi_pin: PA7<Input>,
    pub spi: SPI1,
    pub rx_dma: Stream2<DMA2>,
    pub tx_dma: Stream3<DMA2>,
}

pub fn init_spi1_imu_cs(cs_pin: PA4<Input>) -> Spi1ImuCs {
    let mut cs = cs_pin.into_push_pull_output();
    cs.set_high();
    cs
}

pub fn init_spi1_mpu6500_cs(cs_pin: PA4<Input>) -> Spi1Mpu6500Cs {
    init_spi1_imu_cs(cs_pin)
}

pub fn init_spi1_mpu6500_bus(
    spi: SPI1,
    sck_pin: PA5<Input>,
    miso_pin: PA6<Input>,
    mosi_pin: PA7<Input>,
    clocks: &mut Rcc,
) -> Spi1Mpu6500Bus {
    let mode = spi::Mode {
        polarity: spi::Polarity::IdleLow,
        phase: spi::Phase::CaptureOnFirstTransition,
    };

    init_spi1_bus(spi, sck_pin, miso_pin, mosi_pin, mode, 1_000_000, clocks)
}

pub fn init_spi1_bus(
    spi: SPI1,
    sck_pin: PA5<Input>,
    miso_pin: PA6<Input>,
    mosi_pin: PA7<Input>,
    mode: spi::Mode,
    frequency_hz: u32,
    clocks: &mut Rcc,
) -> Spi1ImuBus {
    Spi::new(
        spi,
        (
            Some(sck_pin.into_alternate()),
            Some(miso_pin.into_alternate()),
            Some(mosi_pin.into_alternate()),
        ),
        mode,
        frequency_hz.Hz(),
        clocks,
    )
}

pub struct SpiPollerSide<TxTransferT, DmaConfigT> {
    pub tx_transfer: Option<TxTransferT>,
    pub dma_config: DmaConfigT,
}

impl<TxStream, SpiT, const CHANNEL: u8>
    SpiPollerSide<SpiTxTransfer<TxStream, SpiT, CHANNEL>, DmaConfig>
where
    TxStream: Stream,
    SpiT: spi::Instance,
    ChannelX<CHANNEL>: Channel,
    spi::Tx<SpiT>: DMASet<TxStream, CHANNEL, MemoryToPeripheral>,
{
    pub fn start_frame<F>(&mut self, frame: &[u8], before_start: F) -> Result<(), SpiTxStartError>
    where
        F: FnOnce(),
    {
        let Some(old_transfer) = self.tx_transfer.take() else {
            return Err(SpiTxStartError::Busy);
        };

        let (stream, tx, primary_buffer, _secondary_buffer) = old_transfer.release();
        if frame.len() > primary_buffer.len() {
            self.tx_transfer = Some(Transfer::init_memory_to_peripheral(
                stream,
                tx,
                primary_buffer,
                None,
                self.dma_config,
            ));
            return Err(SpiTxStartError::FrameTooLong);
        }

        primary_buffer.fill(0);
        primary_buffer[..frame.len()].copy_from_slice(frame);

        before_start();

        let mut new_transfer =
            Transfer::init_memory_to_peripheral(stream, tx, primary_buffer, None, self.dma_config);
        new_transfer.start(|_| {});
        self.tx_transfer = Some(new_transfer);
        Ok(())
    }

    pub fn pause_frame(&mut self) {
        if let Some(transfer) = self.tx_transfer.as_mut() {
            transfer.pause(|_| {});
        }
    }
}

impl<TxStream, SpiT, const CHANNEL: u8> SpiTxFrameDma
    for SpiPollerSide<SpiTxTransfer<TxStream, SpiT, CHANNEL>, DmaConfig>
where
    TxStream: Stream,
    SpiT: spi::Instance,
    ChannelX<CHANNEL>: Channel,
    spi::Tx<SpiT>: DMASet<TxStream, CHANNEL, MemoryToPeripheral>,
{
    fn is_ready(&self) -> bool {
        self.tx_transfer.is_some()
    }

    fn start_frame<F>(&mut self, frame: &[u8], before_start: F) -> Result<(), SpiTxStartError>
    where
        F: FnOnce(),
    {
        SpiPollerSide::start_frame(self, frame, before_start)
    }

    fn pause_frame(&mut self) {
        SpiPollerSide::pause_frame(self);
    }
}

impl<RxStream, SpiT, const CHANNEL: u8> SpiRxTransferExt for SpiRxTransfer<RxStream, SpiT, CHANNEL>
where
    RxStream: Stream,
    SpiT: spi::Instance,
    ChannelX<CHANNEL>: Channel,
    spi::Rx<SpiT>: DMASet<RxStream, CHANNEL, PeripheralToMemory>,
{
    fn spi_rx_has_dma_error(&self) -> bool {
        let flags = self.flags();
        flags.is_transfer_error() || flags.is_direct_mode_error() || flags.is_fifo_error()
    }

    fn spi_rx_is_complete(&self) -> bool {
        self.flags().is_transfer_complete()
    }

    fn clear_spi_rx_flags(&mut self) {
        self.clear_all_flags();
    }

    fn spi_rx_pause(&mut self) {
        self.pause(|_| {});
    }

    fn next_spi_rx_transfer(
        &mut self,
        fresh_buffer: SpiRxBuf,
    ) -> Result<SpiRxBuf, SpiRxRestartFailure> {
        self.next_transfer(fresh_buffer)
            .map(|(raw_buffer, _)| raw_buffer)
            .map_err(|error| SpiRxRestartFailure {
                buffer: match error {
                    stm32f4xx_hal::dma::DMAError::NotReady(buffer)
                    | stm32f4xx_hal::dma::DMAError::SmallBuffer(buffer)
                    | stm32f4xx_hal::dma::DMAError::Overrun(buffer) => buffer,
                },
            })
    }
}

pub struct SpiDmaParts<RxTransferT, TxTransferT> {
    pub irq: SpiRxIrqSide<RxTransferT>,
    pub poller: SpiPollerSide<TxTransferT, DmaConfig>,
    pub parser: SpiRxParserSide,
    pub recovery_rx_buffer: SpiRxBuf,
}

pub fn spi_rx_dma_config() -> DmaConfig {
    DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .fifo_error_interrupt(true)
        .transfer_complete_interrupt(true)
        .double_buffer(false)
}

pub fn spi_tx_dma_config() -> DmaConfig {
    DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .fifo_error_interrupt(true)
}

pub fn init_spi_dma<SpiT, RxStream, TxStream, const RX_CHANNEL: u8, const TX_CHANNEL: u8>(
    spi: Spi<SpiT>,
    rx_stream: RxStream,
    tx_stream: TxStream,
    storage: SpiDmaStorage,
) -> SpiDmaParts<SpiRxTransfer<RxStream, SpiT, RX_CHANNEL>, SpiTxTransfer<TxStream, SpiT, TX_CHANNEL>>
where
    SpiT: spi::Instance,
    RxStream: Stream,
    TxStream: Stream,
    ChannelX<RX_CHANNEL>: Channel,
    ChannelX<TX_CHANNEL>: Channel,
    spi::Rx<SpiT>: DMASet<RxStream, RX_CHANNEL, PeripheralToMemory>,
    spi::Tx<SpiT>: DMASet<TxStream, TX_CHANNEL, MemoryToPeripheral>,
{
    let SpiDmaStorageParts {
        first_rx_buffer,
        recovery_rx_buffer,
        tx_buffer,
        free_consumer,
        filled_producer,
        parser,
    } = storage.split();

    let (spi_tx, spi_rx) = spi.use_dma().txrx();

    let mut rx_transfer = Transfer::init_peripheral_to_memory(
        rx_stream,
        spi_rx,
        first_rx_buffer,
        None,
        spi_rx_dma_config(),
    );

    rx_transfer.start(|_| {});

    let tx_dma_config = spi_tx_dma_config();

    let tx_transfer =
        Transfer::init_memory_to_peripheral(tx_stream, spi_tx, tx_buffer, None, tx_dma_config);

    SpiDmaParts {
        irq: SpiRxIrqSide::new(rx_transfer, free_consumer, filled_producer),
        poller: SpiPollerSide {
            tx_transfer: Some(tx_transfer),
            dma_config: tx_dma_config,
        },
        parser,
        recovery_rx_buffer,
    }
}
