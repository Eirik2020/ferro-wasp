//! TRANSITION: FerroHAL's UART through FerroWasp's `UartRxDmaTransfer` and
//! `UartTxDmaTransfer`, so the shared receive and transmit logic runs unchanged.
//! This goes once the transports move to FerroHAL's own model.
//!
//! One FerroHAL driver owns both directions, where FerroWasp has a receive side and a
//! transmit side in separate resources, so the two share it through a
//! critical-section cell. Locks last as long as copying one chunk or starting one
//! frame; they're uncontended, since every UART task on a port runs at one priority.
//!
//! The receive side copies from FerroHAL's ring into the buffer FerroWasp's planner
//! expects, and reports the end of a chunk as the old stream's idle or full. A line
//! error ends a chunk like an idle line, as the old driver delivered those bytes too,
//! for the parsers to judge. Bytes lost to an overrun or a DMA fault are reported as
//! a DMA error, which the shared logic turns into a discontinuity.
//!
//! Requirements: HW-UART-001, HW-UART-002, HW-UART-003, HW-UART-004
//! Safety-critical.

use core::cell::RefCell;

use critical_section::Mutex;
use ferrohal_stm32::{
    rcc::Rcc,
    uart::{
        self, Buffers, ChunkEnd, Config, Parity, RxWiring, StopBits, Uart, UartIrq, UartRx, UartTx,
        Wiring,
    },
};
// FerroHAL keeps these under each IP version's own name.
#[cfg(feature = "stm32f405")]
use ferrohal_stm32::usart_v2::{Instance, RxDma, RxPin, TxDma, TxPin};
#[cfg(feature = "stm32h743")]
use ferrohal_stm32::usart_v4::{Instance, RxDma, RxPin, TxDma, TxPin};
use ferrowasp_serial::{
    LogicalSerialPort, MSP_V1_MAX_FRAME_LEN, ResolvedBindings, SerialFunctionSlots,
    SerialProtocol as Mode,
};
pub use ferrowasp_stm32_critical::uart_common::*;
use ferrowasp_stm32_critical::uart_port::{
    SerialPortEndpoint, UartRxPort, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
    place_endpoint, rx_port, tx_port,
};

/// The frame each protocol runs on.
pub const fn config_for(mode: Mode) -> Config {
    match mode {
        Mode::Sbus => Config::new(100_000)
            .with_parity(Parity::Even)
            .with_stop_bits(StopBits::Two),
        Mode::Crsf => Config::new(420_000),
        Mode::Mavlink => Config::new(57_600),
        Mode::Msp | Mode::EscTelemetry | Mode::Cli | Mode::Disabled => Config::new(115_200),
    }
}

/// How long FerroHAL's receive ring is for one of FerroWasp's buffers: the ring's
/// half-way and end interrupts then fall at a buffer's length.
pub const RX_RING_BYTES: usize = 2 * UART_RX_BUFFER_SIZE;

/// What each port's board gives FerroHAL to receive into.
pub type UartRing = &'static mut [u8; RX_RING_BYTES];

/// One port's driver and the state of the chunk being gathered.
pub struct UartCore {
    irq: UartIrq,
    rx: UartRx,
    tx: Option<UartTx>,
    /// The buffer being filled, as the old stream's target.
    current: Option<UartRxBuf>,
    filled: usize,
    idle: bool,
    full: bool,
    rx_error: bool,
    tx_error: bool,
}

pub type SharedUart = &'static Mutex<RefCell<Option<UartCore>>>;

impl UartCore {
    /// Serves the driver, then gathers what has arrived into the current buffer.
    fn pump(&mut self) {
        self.irq.on_interrupt();
        if self.idle || self.full || self.rx_error {
            return;
        }
        let Some(buffer) = self.current.as_mut() else {
            return;
        };
        let Some(room) = buffer.get_mut(self.filled..) else {
            self.rx_error = true;
            return;
        };
        match self.rx.read_chunk(room) {
            Ok(None) => {}
            Ok(Some(chunk)) => {
                self.filled += chunk.len;
                match chunk.end {
                    ChunkEnd::Idle => self.idle = true,
                    ChunkEnd::Full => self.full = true,
                    ChunkEnd::Gap(error) => match error {
                        uart::Error::Framing | uart::Error::Parity | uart::Error::Noise => {
                            self.idle = true;
                        }
                        _ => self.rx_error = true,
                    },
                }
            }
            Err(_) => self.rx_error = true,
        }
        self.full |= self.filled >= UART_RX_BUFFER_SIZE;
    }
}

fn with<R>(shared: SharedUart, default: R, f: impl FnOnce(&mut UartCore) -> R) -> R {
    critical_section::with(|cs| match shared.borrow_ref_mut(cs).as_mut() {
        Some(core) => f(core),
        None => default,
    })
}

/// The receive side, as FerroWasp's shared logic drives a transfer.
pub struct UartRxTransfer(SharedUart);

impl UartRxDmaTransfer for UartRxTransfer {
    fn dma_error(&self) -> bool {
        with(self.0, true, |core| {
            core.pump();
            core.rx_error
        })
    }

    fn transfer_complete(&self) -> bool {
        with(self.0, false, |core| core.full)
    }

    fn clear_all_flags(&mut self) {
        with(self.0, (), |core| {
            core.rx_error = false;
            core.full = false;
        });
    }

    fn is_idle(&self) -> bool {
        with(self.0, false, |core| {
            core.pump();
            core.idle || core.full
        })
    }

    fn clear_idle_interrupt(&mut self) {
        with(self.0, (), |core| {
            core.idle = false;
            core.full = false;
        });
    }

    fn number_of_transfers(&self) -> u16 {
        with(self.0, 0, |core| {
            UART_RX_BUFFER_SIZE.saturating_sub(core.filled) as u16
        })
    }

    fn next_transfer(&mut self, fresh: UartRxBuf) -> Result<UartRxBuf, UartRxRestartError> {
        with(self.0, Err(UartRxRestartError), |core| {
            let filled = core.current.replace(fresh).ok_or(UartRxRestartError)?;
            core.filled = 0;
            Ok(filled)
        })
    }
}

/// The transmit side, as FerroWasp's shared logic drives a transfer. It keeps the
/// fixed-length wire transfer the current targets validated.
pub struct UartTxTransfer(SharedUart);

impl UartTxDmaTransfer for UartTxTransfer {
    fn start_padded(&mut self, bytes: &[u8]) {
        let mut padded = [0; MSP_V1_MAX_FRAME_LEN];
        if let Some(head) = padded.get_mut(..bytes.len()) {
            head.copy_from_slice(bytes);
        }
        with(self.0, (), |core| {
            let sent = core.tx.as_mut().map(|tx| tx.try_send(&padded));
            core.tx_error |= !matches!(sent, Some(Ok(())));
        });
    }

    fn flags(&self) -> UartTxDmaFlags {
        with(self.0, UartTxDmaFlags::default(), |core| {
            core.irq.on_tx_dma();
            let idle = core.tx.as_mut().map(UartTx::is_idle);
            UartTxDmaFlags {
                transfer_complete: matches!(idle, Some(Ok(true))),
                transfer_error: core.tx_error || matches!(idle, Some(Err(_))),
                direct_mode_error: false,
                fifo_error: false,
            }
        })
    }

    fn clear_all_flags(&mut self) {
        with(self.0, (), |core| core.tx_error = false);
    }

    fn clear_fifo_error(&mut self) {}

    fn pause(&mut self) {}
}

pub type UartRxIrq = UartRxIrqSide<UartRxTransfer>;
pub type UartTxSide = UartTxDmaSide<UartTxTransfer>;

fn rx_side(
    shared: SharedUart,
    irq: UartIrq,
    rx: UartRx,
    tx: Option<UartTx>,
    mode: Mode,
    storage: UartRxStorage,
) -> UartRxParts<UartRxTransfer> {
    let (first, free_consumer, filled_producer, parser) = split_uart_rx_storage(storage);
    critical_section::with(|cs| {
        shared.borrow_ref_mut(cs).replace(UartCore {
            irq,
            rx,
            tx,
            current: Some(first),
            filled: 0,
            idle: false,
            full: false,
            rx_error: false,
            tx_error: false,
        });
    });
    uart_rx_parts(
        mode,
        UartRxTransfer(shared),
        free_consumer,
        filled_producer,
        parser,
    )
}

/// One port's memory beside FerroWasp's own: the driver's cell and its receive ring.
pub struct PortMemory {
    pub core: SharedUart,
    pub ring: UartRing,
}

/// Start a port that can transmit, if `bindings` gives it a function. An unbound
/// port is never enabled. `invert_rx` is the board's, for a receiver wired inverted.
#[allow(clippy::too_many_arguments)]
pub fn start_rx_tx_port<U, TxP, RxP, TxD, RxD>(
    logical: LogicalSerialPort,
    wiring: Wiring<U, TxP, RxP, TxD, RxD>,
    rcc: &mut Rcc,
    storage: UartRxTxPortStorage,
    memory: PortMemory,
    invert_rx: bool,
    bindings: &ResolvedBindings,
    functions: &mut SerialFunctionSlots<SerialPortEndpoint>,
) -> (Option<UartRxPort<UartRxIrq>>, UartTxPort<UartTxSide>)
where
    U: Instance,
    TxP: TxPin<U>,
    RxP: RxPin<U>,
    TxD: TxDma<U>,
    RxD: RxDma<U>,
{
    let Some(profile) = bindings.profile(logical) else {
        return (None, UartTxPort::unbound());
    };
    let config = config_for(profile.protocol).with_inverted_rx(invert_rx);
    let buffers = Buffers {
        tx: storage.tx_buffer,
        rx: memory.ring,
    };
    let Ok((uart, irq)) = Uart::new(rcc, wiring, &config, buffers) else {
        defmt::warn!("UART config refused; the port stays off");
        return (None, UartTxPort::unbound());
    };
    let (tx, rx) = Uart::split(uart);
    let needs_tx = profile.protocol.needs_tx();
    let parts = rx_side(
        memory.core,
        irq,
        rx,
        needs_tx.then_some(tx),
        profile.protocol,
        storage.rx.into_backend(),
    );
    let (writer, tx_side) = if needs_tx {
        let (tx_port_side, writer) = tx_port(
            UartTxDmaSide::new(UartTxTransfer(memory.core)),
            storage.tx_stream,
        );
        (Some(writer), tx_port_side)
    } else {
        (None, UartTxPort::unbound())
    };
    let (port, endpoint) = rx_port(logical, parts.irq, parts.parser, storage.stream, writer);
    place_endpoint(functions, bindings, logical, endpoint);
    (Some(port), tx_side)
}

/// Start a receive-only port, if `bindings` gives it a function.
pub fn start_rx_port<U, RxP, RxD>(
    logical: LogicalSerialPort,
    wiring: RxWiring<U, RxP, RxD>,
    rcc: &mut Rcc,
    storage: UartRxPortStorage,
    memory: PortMemory,
    invert_rx: bool,
    bindings: &ResolvedBindings,
    functions: &mut SerialFunctionSlots<SerialPortEndpoint>,
) -> Option<UartRxPort<UartRxIrq>>
where
    U: Instance,
    RxP: RxPin<U>,
    RxD: RxDma<U>,
{
    let profile = bindings.profile(logical)?;
    let config = config_for(profile.protocol).with_inverted_rx(invert_rx);
    let Ok((rx, irq)) = UartRx::new(rcc, wiring, &config, memory.ring) else {
        defmt::warn!("UART config refused; the port stays off");
        return None;
    };
    let parts = rx_side(
        memory.core,
        irq,
        rx,
        None,
        profile.protocol,
        storage.rx.into_backend(),
    );
    let (port, endpoint) = rx_port(logical, parts.irq, parts.parser, storage.stream, None);
    place_endpoint(functions, bindings, logical, endpoint);
    Some(port)
}
