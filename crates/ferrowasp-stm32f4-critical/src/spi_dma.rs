//! Requirements: HW-SPI-001
//! Safety-critical.

pub use ferrowasp_stm32_critical::spi_common::*;
use stm32f4xx_hal::{
    dma::{
        ChannelX, MemoryToPeripheral, PeripheralToMemory, Transfer,
        config::DmaConfig,
        traits::{Channel, DMASet, Stream},
    },
    prelude::*,
    spi::{self, Spi},
};

/// An SPI receive DMA transfer. A type of this crate's own, so it can carry
/// the chip-neutral `SpiRxTransferExt`.
pub struct SpiRxTransfer<RxStream, SpiT, const CHANNEL: u8>(
    pub Transfer<RxStream, CHANNEL, spi::Rx<SpiT>, PeripheralToMemory, SpiRxBuf>,
)
where
    RxStream: Stream,
    SpiT: spi::Instance,
    ChannelX<CHANNEL>: Channel,
    spi::Rx<SpiT>: DMASet<RxStream, CHANNEL, PeripheralToMemory>;

pub type SpiTxTransfer<TxStream, SpiT, const CHANNEL: u8> =
    Transfer<TxStream, CHANNEL, spi::Tx<SpiT>, MemoryToPeripheral, SpiTxBuf>;

/// An STM32F4 SPI DMA owner: the shared owner over the HAL's transfers.
pub type SpiDmaOwner<RxTransferT, TxTransferT, CsT> =
    ferrowasp_stm32_critical::spi_common::SpiDmaOwner<
        RxTransferT,
        SpiPollerSide<TxTransferT, DmaConfig>,
        CsT,
    >;
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
        let flags = self.0.flags();
        flags.is_transfer_error() || flags.is_direct_mode_error() || flags.is_fifo_error()
    }

    fn spi_rx_is_complete(&self) -> bool {
        self.0.flags().is_transfer_complete()
    }

    fn clear_spi_rx_flags(&mut self) {
        self.0.clear_all_flags();
    }

    fn spi_rx_pause(&mut self) {
        self.0.pause(|_| {});
    }

    fn next_spi_rx_transfer(
        &mut self,
        fresh_buffer: SpiRxBuf,
    ) -> Result<SpiRxBuf, SpiRxRestartFailure> {
        self.0
            .next_transfer(fresh_buffer)
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
        irq: SpiRxIrqSide::new(SpiRxTransfer(rx_transfer), free_consumer, filled_producer),
        poller: SpiPollerSide {
            tx_transfer: Some(tx_transfer),
            dma_config: tx_dma_config,
        },
        parser,
        recovery_rx_buffer,
    }
}
