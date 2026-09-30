//! SPI1 DMA on the STM32H7, under the shared SPI owner in
//! `ferrowasp_stm32f4::spi_common`.
//!
//! The H7 SPI counts each transfer in hardware: the poller disables the SPI,
//! loads the frame length, arms the transmit stream, and starts the transfer;
//! the receive stream stays armed between frames and is re-armed by the
//! receive-complete interrupt, as on the F4. The HAL configures the bus for
//! blocking bring-up and is then released, so the poller owns SPI1 itself.

use crate::dma_endpoints::{Spi1RxEndpoint, Spi1TxEndpoint, StreamErrors, spi1_endpoints};
use crate::eh1;
pub use ferrowasp_stm32f4::spi_common::*;
use stm32h7xx_hal::{
    dma::{
        DBTransfer, MemoryToPeripheral, PeripheralToMemory, Transfer,
        dma::DmaConfig,
        traits::{DoubleBufferedStream, Stream},
    },
    gpio::{PA5, PA6, PC15, PD7},
    pac::SPI1,
    prelude::*,
    rcc::{CoreClocks, rec},
    spi::{self, Enabled, HalDisabledSpi, HalEnabledSpi, Spi},
};

pub type Spi1RxStreamTransfer<S> =
    Transfer<S, Spi1RxEndpoint, PeripheralToMemory, SpiRxBuf, DBTransfer>;
pub type Spi1TxStreamTransfer<S> =
    Transfer<S, Spi1TxEndpoint, MemoryToPeripheral, SpiTxBuf, DBTransfer>;

pub type Spi1ImuCs = eh1::OutputPin<'C', 15>;
/// The blocking bus the IMU is brought up on before DMA takes it over.
pub type Spi1ImuBus = eh1::SpiBus<Spi<SPI1, Enabled, u8>>;

/// An H7 SPI1 DMA owner: the shared owner over this module's two sides.
pub type Spi1DmaOwner<RxS, TxS, CsT> = SpiDmaOwner<Spi1RxDma<RxS>, Spi1Poller<TxS>, CsT>;

pub struct Spi1BusPins {
    pub sck: PA5,
    pub miso: PA6,
    pub mosi: PD7,
}

pub fn init_spi1_imu_cs(cs_pin: PC15) -> Spi1ImuCs {
    let mut cs = cs_pin.into_push_pull_output();
    cs.set_high();
    eh1::OutputPin::new(cs)
}

/// SPI1 as a blocking master for bring-up: 8-bit frames, software chip
/// select.
pub fn init_spi1_bus(
    spi: SPI1,
    pins: Spi1BusPins,
    mode: spi::Mode,
    frequency_hz: u32,
    prec: rec::Spi1,
    clocks: &CoreClocks,
) -> Spi1ImuBus {
    let spi: Spi<SPI1, Enabled, u8> = spi.spi(
        (
            pins.sck.into_alternate::<5>(),
            pins.miso.into_alternate::<5>(),
            pins.mosi.into_alternate::<5>(),
        ),
        mode,
        frequency_hz.Hz(),
        prec,
        clocks,
    );
    eh1::SpiBus::new(spi)
}

/// The receive stream. It is armed before each frame starts and re-armed
/// by the owner when the frame completes.
pub struct Spi1RxDma<S>
where
    S: Stream,
{
    transfer: Spi1RxStreamTransfer<S>,
}

impl<S> SpiRxTransferExt for Spi1RxDma<S>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    fn spi_rx_has_dma_error(&self) -> bool {
        S::error_flags().any()
    }

    fn spi_rx_is_complete(&self) -> bool {
        self.transfer.get_transfer_complete_flag()
    }

    fn clear_spi_rx_flags(&mut self) {
        self.transfer.clear_interrupts();
    }

    fn spi_rx_pause(&mut self) {
        self.transfer.pause(|_| {});
    }

    fn next_spi_rx_transfer(
        &mut self,
        fresh_buffer: SpiRxBuf,
    ) -> Result<SpiRxBuf, SpiRxRestartFailure> {
        // Single-buffer mode: the HAL stops the stream, hands back the buffer
        // it filled, and re-arms it into `fresh_buffer`. That cannot fail;
        // the error arm exists because double-buffer mode can.
        let mut fresh = Some(fresh_buffer);
        let filled = self
            .transfer
            .next_transfer_with(|filled, _, _| match fresh.take() {
                Some(fresh) => (fresh, Some(filled)),
                None => (filled, None),
            });
        match (filled, fresh) {
            (Ok(Some(filled)), _) => Ok(filled),
            (_, Some(buffer)) => Err(SpiRxRestartFailure { buffer }),
            (Ok(None) | Err(_), None) => {
                unreachable!("the stream took the fresh buffer and returned the filled one")
            }
        }
    }
}

/// The transmit side: SPI1 itself and its transmit stream.
pub struct Spi1Poller<S>
where
    S: Stream,
{
    spi: SPI1,
    transfer: Spi1TxStreamTransfer<S>,
}

impl<S> Spi1Poller<S>
where
    S: Stream,
{
    /// Stop the SPI. Configuration writes need it stopped, and stopping
    /// ends any transfer still counting.
    fn stop_spi(&mut self) {
        self.spi.cr1.modify(|_, w| w.spe().disabled());
        self.spi.cfg1.modify(|_, w| w.txdmaen().disabled());
        self.spi.ifcr.write(|w| {
            w.eotc()
                .clear()
                .txtfc()
                .clear()
                .ovrc()
                .clear()
                .udrc()
                .clear()
                .modfc()
                .clear()
        });
    }
}

impl<S> SpiTxFrameDma for Spi1Poller<S>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    fn is_ready(&self) -> bool {
        true
    }

    fn start_frame<F>(&mut self, frame: &[u8], before_start: F) -> Result<(), SpiTxStartError>
    where
        F: FnOnce(),
    {
        if frame.len() > SPI_BUFFER_SIZE {
            return Err(SpiTxStartError::FrameTooLong);
        }

        self.stop_spi();
        // The whole padded buffer goes out, as on the F4, so the receive
        // stream sees exactly one buffer.
        self.spi
            .cr2
            .write(|w| w.tsize().bits(SPI_BUFFER_SIZE as u16));

        self.transfer.clear_interrupts();
        let _ = self.transfer.next_transfer_with(|buffer, _, _| {
            buffer.fill(0);
            buffer[..frame.len()].copy_from_slice(frame);
            (buffer, ())
        });

        before_start();

        // RM0433 "Communication using DMA": receive requests, both streams,
        // transmit requests, SPI enable, then start.
        self.spi.cfg1.modify(|_, w| w.txdmaen().enabled());
        self.spi.cr1.modify(|_, w| w.spe().enabled());
        self.spi.cr1.modify(|_, w| w.cstart().started());
        Ok(())
    }

    fn pause_frame(&mut self) {
        self.transfer.pause(|_| {});
        self.stop_spi();
    }
}

pub struct Spi1DmaParts<RxS, TxS>
where
    RxS: Stream,
    TxS: Stream,
{
    pub irq: SpiRxIrqSide<Spi1RxDma<RxS>>,
    pub poller: Spi1Poller<TxS>,
    pub parser: SpiRxParserSide,
    pub recovery_rx_buffer: SpiRxBuf,
}

fn spi_rx_dma_config() -> DmaConfig {
    DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .fifo_error_interrupt(true)
        .transfer_error_interrupt(true)
        .transfer_complete_interrupt(true)
}

/// No interrupts: the frame ends when the receive stream completes.
fn spi_tx_dma_config() -> DmaConfig {
    DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
}

/// Hand the brought-up bus to DMA. SPI1 stays stopped until the first frame.
pub fn init_spi1_dma<RxS, TxS>(
    bus: Spi1ImuBus,
    rx_stream: RxS,
    tx_stream: TxS,
    storage: SpiDmaStorage,
) -> Spi1DmaParts<RxS, TxS>
where
    RxS: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    TxS: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    let SpiDmaStorageParts {
        first_rx_buffer,
        recovery_rx_buffer,
        tx_buffer,
        free_consumer,
        filled_producer,
        parser,
    } = storage.split();

    let (spi, _prec) = bus.into_inner().disable().free();
    let (rx_endpoint, tx_endpoint) = spi1_endpoints(&spi);
    // Keep the pins driven while SPE is clear between frames, so SCK holds
    // its idle level with the chip select low.
    spi.cfg2.modify(|_, w| w.afcntr().controlled());
    spi.cfg1
        .modify(|_, w| w.rxdmaen().enabled().txdmaen().disabled());

    let mut rx_transfer = Transfer::init(
        rx_stream,
        rx_endpoint,
        first_rx_buffer,
        None,
        spi_rx_dma_config(),
    );
    rx_transfer.start(|_| {});
    let tx_transfer = Transfer::init(tx_stream, tx_endpoint, tx_buffer, None, spi_tx_dma_config());

    Spi1DmaParts {
        irq: SpiRxIrqSide::new(
            Spi1RxDma {
                transfer: rx_transfer,
            },
            free_consumer,
            filled_producer,
        ),
        poller: Spi1Poller {
            spi,
            transfer: tx_transfer,
        },
        parser,
        recovery_rx_buffer,
    }
}
