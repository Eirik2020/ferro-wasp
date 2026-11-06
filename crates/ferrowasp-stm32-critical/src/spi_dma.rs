//! TRANSITION: FerroHAL's SPI through FerroWasp's `SpiRxTransferExt` and
//! `SpiTxFrameDma`, so the shared owner, mailbox and recovery run unchanged. This goes
//! once the transports move to FerroHAL's own model.
//!
//! One FerroHAL driver does both directions, where FerroWasp's owner holds a receive
//! side and a transmit side, so the two share it through a critical-section cell.
//! Locks last as long as starting one transfer or copying one frame.
//!
//! Requirements: HW-SPI-001, HW-SPI-002, HW-SPI-003, HW-SPI-004
//! Safety-critical.

use core::cell::RefCell;

pub use crate::spi_common::*;
use critical_section::Mutex;
use embedded_hal::digital::OutputPin;
use ferrohal_stm32::spi::{self, Spi};

pub struct SpiCore {
    spi: Spi,
    /// The last transfer's end, until the owner has seen it.
    done: Option<Result<usize, spi::Error>>,
}

pub type SharedSpi = &'static Mutex<RefCell<Option<SpiCore>>>;

fn with<R>(shared: SharedSpi, default: R, f: impl FnOnce(&mut SpiCore) -> R) -> R {
    critical_section::with(|cs| match shared.borrow_ref_mut(cs).as_mut() {
        Some(core) => f(core),
        None => default,
    })
}

impl SpiCore {
    fn serve(&mut self) {
        if let Some(done) = self.spi.on_interrupt() {
            self.done = Some(done);
        }
    }
}

/// The receive side: FerroHAL's driver reads into its own buffer, and each end of a
/// transfer copies that into the buffer FerroWasp's owner rotates.
pub struct SpiRxTransfer {
    shared: SharedSpi,
    current: Option<SpiRxBuf>,
}

impl SpiRxTransferExt for SpiRxTransfer {
    fn spi_rx_has_dma_error(&self) -> bool {
        with(self.shared, true, |core| {
            core.serve();
            matches!(core.done, Some(Err(_)))
        })
    }

    fn spi_rx_is_complete(&self) -> bool {
        with(self.shared, false, |core| matches!(core.done, Some(Ok(_))))
    }

    fn clear_spi_rx_flags(&mut self) {
        with(self.shared, (), |core| core.done = None);
    }

    fn spi_rx_pause(&mut self) {
        with(self.shared, (), |core| {
            core.spi.abort();
            core.done = None;
        });
    }

    fn next_spi_rx_transfer(
        &mut self,
        fresh_buffer: SpiRxBuf,
    ) -> Result<SpiRxBuf, SpiRxRestartFailure> {
        let Some(filled) = self.current.take() else {
            return Err(SpiRxRestartFailure {
                buffer: fresh_buffer,
            });
        };
        self.current = Some(fresh_buffer);
        with(self.shared, (), |core| {
            let received = core.spi.received();
            let len = received.len().min(filled.len());
            if let (Some(to), Some(from)) = (filled.get_mut(..len), received.get(..len)) {
                to.copy_from_slice(from);
            }
        });
        Ok(filled)
    }
}

/// The transmit side, keeping the fixed-length frame the targets validated.
pub struct SpiPollerSide {
    shared: SharedSpi,
}

impl SpiTxFrameDma for SpiPollerSide {
    fn is_ready(&self) -> bool {
        with(self.shared, false, |core| !core.spi.is_busy())
    }

    fn start_frame<F>(&mut self, frame: &[u8], before_start: F) -> Result<(), SpiTxStartError>
    where
        F: FnOnce(),
    {
        let mut padded = [0; SPI_BUFFER_SIZE];
        let Some(head) = padded.get_mut(..frame.len()) else {
            return Err(SpiTxStartError::FrameTooLong);
        };
        head.copy_from_slice(frame);
        before_start();
        with(self.shared, Err(SpiTxStartError::Busy), |core| {
            core.done = None;
            core.spi.start(&padded).map_err(|_| SpiTxStartError::Busy)
        })
    }

    fn pause_frame(&mut self) {
        with(self.shared, (), |core| core.spi.abort());
    }
}

pub type SpiDmaOwner<CsT> = crate::spi_common::SpiDmaOwner<SpiRxTransfer, SpiPollerSide, CsT>;

pub struct SpiDmaParts {
    pub irq: SpiRxIrqSide<SpiRxTransfer>,
    pub poller: SpiPollerSide,
    pub parser: SpiRxParserSide,
    pub recovery_rx_buffer: SpiRxBuf,
}

/// Turns a configured driver into FerroWasp's two sides, sharing `cell`.
pub fn init_spi_dma(spi: Spi, cell: SharedSpi, storage: SpiDmaStorage) -> SpiDmaParts {
    let SpiDmaStorageParts {
        first_rx_buffer,
        recovery_rx_buffer,
        tx_buffer: _,
        free_consumer,
        filled_producer,
        parser,
    } = storage.split();
    critical_section::with(|cs| {
        cell.borrow_ref_mut(cs).replace(SpiCore { spi, done: None });
    });
    SpiDmaParts {
        irq: SpiRxIrqSide::new(
            SpiRxTransfer {
                shared: cell,
                current: Some(first_rx_buffer),
            },
            free_consumer,
            filled_producer,
        ),
        poller: SpiPollerSide { shared: cell },
        parser,
        recovery_rx_buffer,
    }
}

/// Builds the owner, with chip select `cs` high.
pub fn owner<CsT: OutputPin>(parts: SpiDmaParts, cs: CsT) -> (SpiDmaOwner<CsT>, SpiRxParserSide) {
    let owner = SpiDmaOwner::new(parts.irq, parts.poller, parts.recovery_rx_buffer, cs);
    (owner, parts.parser)
}
