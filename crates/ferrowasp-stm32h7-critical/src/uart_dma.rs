//! UART receive and transmit DMA on the STM32H7, under the shared UART
//! mechanisms in `ferrowasp_stm32_critical::uart_common`.
//!
//! The HAL configures the UART and is then released: the receive side keeps
//! the UART for its idle-line flag, and both sides move bytes through a HAL
//! `Transfer` over a data-register endpoint from `dma_endpoints`.

use crate::dma_endpoints::{
    StreamErrors, UartDmaPeripheral, UartRxEndpoint, UartTxEndpoint, uart_endpoints,
};
use ferrowasp_serial::{
    LogicalSerialPort, ResolvedBindings, SerialFunctionSlots, SerialProtocol as Mode,
};
pub use ferrowasp_stm32_critical::uart_common::*;
use ferrowasp_stm32_critical::uart_port::{
    SerialPortEndpoint, UartRxPort, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
    place_endpoint, rx_port, tx_port,
};
use stm32h7xx_hal::{
    dma::{
        DBTransfer, MemoryToPeripheral, PeripheralToMemory, Transfer,
        dma::DmaConfig,
        traits::{DoubleBufferedStream, Stream},
    },
    prelude::*,
    serial::config::{Config, StopBits},
};

/// A UART's receive stream: a HAL transfer into the rotating buffers.
pub type UartRxStreamTransfer<S, U> =
    Transfer<S, UartRxEndpoint<U>, PeripheralToMemory, UartRxBuf, DBTransfer>;

/// A UART's transmit stream: a HAL transfer from the fixed MSP buffer.
pub type UartTxStreamTransfer<S, U> =
    Transfer<S, UartTxEndpoint<U>, MemoryToPeripheral, UartTxBuf, DBTransfer>;

/// The receive stream and the UART whose idle line ends a chunk.
pub struct UartRxDma<S, U>
where
    S: Stream,
    UartRxEndpoint<U>: stm32h7xx_hal::dma::traits::TargetAddress<PeripheralToMemory>,
{
    transfer: UartRxStreamTransfer<S, U>,
    uart: U,
}

impl<S, U> UartRxDmaTransfer for UartRxDma<S, U>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    U: UartDmaPeripheral,
{
    fn dma_error(&self) -> bool {
        S::error_flags().any()
    }

    fn transfer_complete(&self) -> bool {
        self.transfer.get_transfer_complete_flag()
    }

    fn clear_all_flags(&mut self) {
        self.transfer.clear_interrupts();
    }

    fn is_idle(&self) -> bool {
        self.uart.isr.read().idle().bit_is_set()
    }

    fn clear_idle_interrupt(&mut self) {
        self.uart.icr.write(|w| w.idlecf().set_bit());
    }

    fn number_of_transfers(&self) -> u16 {
        S::get_number_of_transfers()
    }

    fn next_transfer(&mut self, fresh: UartRxBuf) -> Result<UartRxBuf, UartRxRestartError> {
        // Single-buffer mode: the HAL stops the stream, which flushes its
        // FIFO to memory, before handing back the filled buffer.
        self.transfer
            .next_transfer(fresh)
            .map(|(filled, _, _)| filled)
            .map_err(|_| UartRxRestartError)
    }
}

/// The transmit stream and its fixed buffer.
pub struct UartTxDma<S, U>
where
    S: Stream,
    UartTxEndpoint<U>: stm32h7xx_hal::dma::traits::TargetAddress<MemoryToPeripheral>,
{
    transfer: UartTxStreamTransfer<S, U>,
}

impl<S, U> UartTxDmaTransfer for UartTxDma<S, U>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    U: UartDmaPeripheral,
{
    fn start_padded(&mut self, bytes: &[u8]) {
        self.transfer.clear_interrupts();
        // Single-buffer mode: this reloads the length and enables the stream.
        let _ = self.transfer.next_transfer_with(|buffer, _, _| {
            buffer.fill(0);
            buffer[..bytes.len()].copy_from_slice(bytes);
            (buffer, ())
        });
    }

    fn flags(&self) -> UartTxDmaFlags {
        let errors = S::error_flags();
        UartTxDmaFlags {
            transfer_complete: self.transfer.get_transfer_complete_flag(),
            transfer_error: errors.transfer_error,
            direct_mode_error: errors.direct_mode_error,
            fifo_error: errors.fifo_error,
        }
    }

    fn clear_all_flags(&mut self) {
        self.transfer.clear_interrupts();
    }

    /// The HAL clears a stream's flags only together. The transmit stream
    /// does not interrupt on a FIFO error, so a pending one is cleared with
    /// the transfer's terminal flags.
    fn clear_fifo_error(&mut self) {}

    fn pause(&mut self) {
        self.transfer.pause(|_| {});
    }
}

/// The UART settings each protocol needs. SBUS is inverted on the wire; the
/// H7 inverts it in the UART, with no external inverter.
pub fn stm32h7_uart_config(mode: Mode) -> Config {
    match mode {
        Mode::Sbus => Config::new(100_000.Hz())
            .parity_even()
            .stopbits(StopBits::Stop2)
            .invertrx(true),
        Mode::Msp | Mode::EscTelemetry | Mode::Cli => Config::new(115_200.Hz()),
        Mode::Mavlink => Config::new(57_600.Hz()),
        Mode::Crsf => Config::new(420_000.Hz()),
        Mode::Disabled => Config::new(115_200.Hz()),
    }
}

fn rx_dma_config() -> DmaConfig {
    DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .fifo_error_interrupt(true)
        .transfer_error_interrupt(true)
        .transfer_complete_interrupt(true)
}

/// No FIFO-error interrupt: the HAL cannot clear that flag alone, and on a
/// memory-to-peripheral stream it is advisory.
fn tx_dma_config() -> DmaConfig {
    DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .transfer_error_interrupt(true)
        .direct_mode_error_interrupt(true)
        .transfer_complete_interrupt(true)
}

/// Enable DMA requests and the idle interrupt on a UART the HAL configured,
/// and disable overrun detection so a late DMA read cannot stall reception.
fn enable_uart_dma<U: UartDmaPeripheral>(uart: &U, tx: bool) {
    uart.cr1.modify(|_, w| w.ue().clear_bit());
    uart.cr3
        .modify(|_, w| w.ovrdis().set_bit().dmar().set_bit().dmat().bit(tx));
    uart.cr1.modify(|_, w| w.idleie().set_bit().ue().set_bit());
}

fn start_rx<S, U>(
    uart: U,
    stream: S,
    mode: Mode,
    storage: UartRxStorage,
    with_tx: bool,
) -> (UartRxParts<UartRxDma<S, U>>, UartTxEndpoint<U>)
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    U: UartDmaPeripheral,
{
    let (rx_endpoint, tx_endpoint) = uart_endpoints(&uart);
    enable_uart_dma(&uart, with_tx);

    let (first_buffer, free_consumer, filled_producer, parser) = split_uart_rx_storage(storage);
    let mut transfer = Transfer::init(stream, rx_endpoint, first_buffer, None, rx_dma_config());
    transfer.start(|_| {});

    (
        uart_rx_parts(
            mode,
            UartRxDma { transfer, uart },
            free_consumer,
            filled_producer,
            parser,
        ),
        tx_endpoint,
    )
}

/// The receive side of a started port, for a board's shared resources.
pub type UartRxIrq<S, U> = UartRxIrqSide<UartRxDma<S, U>>;
/// The transmit side of a started port.
pub type UartTxSide<S, U> = UartTxDmaSide<UartTxDma<S, U>>;

/// A started port that can transmit: the receive side when it is bound, and
/// the transmit side.
pub type StartedRxTxPort<RxS, TxS, U> = (
    Option<UartRxPort<UartRxIrq<RxS, U>>>,
    UartTxPort<UartTxSide<TxS, U>>,
);

/// Start a port that can transmit, if `bindings` gives it a function: the
/// receive side always, and the transmit side when the bound protocol
/// transmits. `open` configures the board's UART, on its own pins and
/// alternate function, with the line settings it is handed, and releases
/// it; it is called only for a bound port, so an unbound port is never
/// enabled and returns `None` with an unbound transmit side. Streams are the
/// board's choice: any DMA1 or DMA2 stream.
#[allow(clippy::too_many_arguments)]
pub fn start_rx_tx_port<U, RxS, TxS>(
    logical: LogicalSerialPort,
    open: impl FnOnce(Config) -> U,
    rx_dma: RxS,
    tx_dma: TxS,
    storage: UartRxTxPortStorage,
    bindings: &ResolvedBindings,
    functions: &mut SerialFunctionSlots<SerialPortEndpoint>,
) -> StartedRxTxPort<RxS, TxS, U>
where
    U: UartDmaPeripheral,
    RxS: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    TxS: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    let Some(profile) = bindings.profile(logical) else {
        return (None, UartTxPort::unbound());
    };
    let uart = open(stm32h7_uart_config(profile.protocol));
    let with_tx = profile.protocol.needs_tx();
    let (parts, tx_endpoint) = start_rx(
        uart,
        rx_dma,
        profile.protocol,
        storage.rx.into_backend(),
        with_tx,
    );
    let (tx_side, writer) = if with_tx {
        let transfer = Transfer::init(
            tx_dma,
            tx_endpoint,
            storage.tx_buffer,
            None,
            tx_dma_config(),
        );
        let (tx, writer) = tx_port(
            UartTxDmaSide::new(UartTxDma { transfer }),
            storage.tx_stream,
        );
        (tx, Some(writer))
    } else {
        (UartTxPort::unbound(), None)
    };
    let (port, endpoint) = rx_port(logical, parts.irq, parts.parser, storage.stream, writer);
    place_endpoint(functions, bindings, logical, endpoint);
    (Some(port), tx_side)
}

/// Start a receive-only port, if `bindings` gives it a function. `open`
/// configures the board's UART with no transmit pin.
pub fn start_rx_port<U, S>(
    logical: LogicalSerialPort,
    open: impl FnOnce(Config) -> U,
    rx_dma: S,
    storage: UartRxPortStorage,
    bindings: &ResolvedBindings,
    functions: &mut SerialFunctionSlots<SerialPortEndpoint>,
) -> Option<UartRxPort<UartRxIrq<S, U>>>
where
    U: UartDmaPeripheral,
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    let profile = bindings.profile(logical)?;
    let uart = open(stm32h7_uart_config(profile.protocol));
    let (parts, _) = start_rx(
        uart,
        rx_dma,
        profile.protocol,
        storage.rx.into_backend(),
        false,
    );
    let (port, endpoint) = rx_port(logical, parts.irq, parts.parser, storage.stream, None);
    place_endpoint(functions, bindings, logical, endpoint);
    Some(port)
}
