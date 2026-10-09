//! UART receive and transmit DMA on the STM32H7, under the shared UART
//! mechanisms in `ferrowasp_stm32::uart_common`.
//!
//! The HAL configures the UART and is then released: the receive side keeps
//! the UART for its idle-line flag, and both sides move bytes through a HAL
//! `Transfer` over a data-register endpoint from `dma_endpoints`.

use crate::dma_endpoints::{
    StreamErrors, UartDmaPeripheral, UartRxEndpoint, UartTxEndpoint, uart_endpoints,
};
use ferrowasp_io_core::serial::{
    LogicalSerialPort, ResolvedBindings, SerialFunctionSlots, SerialProtocol as Mode,
};
pub use ferrowasp_stm32::uart_common::*;
use ferrowasp_stm32::uart_port::{
    SerialPortEndpoint, UartRxPort, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
    place_endpoint, rx_port, tx_port,
};
use stm32h7xx_hal::{
    dma::{
        DBTransfer, MemoryToPeripheral, PeripheralToMemory, Transfer,
        dma::DmaConfig,
        traits::{DoubleBufferedStream, Stream},
    },
    gpio::{PC6, PC7, PD8, PD9, PE0},
    pac::{UART8, USART3, USART6},
    prelude::*,
    rcc::{CoreClocks, rec},
    serial::{
        NoTx,
        config::{Config, StopBits},
    },
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

/// Stream types are the board's choice; the backend takes any DMA1 or DMA2
/// stream.
///
/// UART6 (the USART6 peripheral) on PC6/PC7, RX and TX by DMA.
pub struct Uart6PortResources<RxS, TxS> {
    pub tx_pin: PC6,
    pub rx_pin: PC7,
    pub uart: USART6,
    pub prec: rec::Usart6,
    pub rx_dma: RxS,
    pub tx_dma: TxS,
}

/// UART3 (the USART3 peripheral) on PD8/PD9, RX and TX by DMA.
pub struct Uart3PortResources<RxS, TxS> {
    pub tx_pin: PD8,
    pub rx_pin: PD9,
    pub uart: USART3,
    pub prec: rec::Usart3,
    pub rx_dma: RxS,
    pub tx_dma: TxS,
}

/// UART8 receive-only on PE0, RX by DMA.
pub struct Uart8PortResources<S> {
    pub rx_pin: PE0,
    pub uart: UART8,
    pub prec: rec::Uart8,
    pub rx_dma: S,
}

/// Every UART the STM32H743 flight boards route, by logical port.
pub struct H743UartPortResources<S3Rx, S3Tx, S6Rx, S6Tx, S8> {
    pub uart3: Uart3PortResources<S3Rx, S3Tx>,
    pub uart6: Uart6PortResources<S6Rx, S6Tx>,
    pub uart8: Uart8PortResources<S8>,
}

/// Static buffers and stream owners for each port.
pub struct H743UartPortStorage {
    pub uart3: UartRxTxPortStorage,
    pub uart6: UartRxTxPortStorage,
    pub uart8: UartRxPortStorage,
}

/// The started ports. A port left unbound stays `None`: its peripheral is
/// never enabled, so its interrupts never fire.
pub struct H743UartPorts<S3Rx, S3Tx, S6Rx, S6Tx, S8>
where
    S3Rx: Stream,
    S3Tx: Stream,
    S6Rx: Stream,
    S6Tx: Stream,
    S8: Stream,
{
    pub uart3_rx: Option<UartRxPort<UartRxIrqSide<UartRxDma<S3Rx, USART3>>>>,
    pub uart3_tx: UartTxPort<UartTxDmaSide<UartTxDma<S3Tx, USART3>>>,
    pub uart6_rx: Option<UartRxPort<UartRxIrqSide<UartRxDma<S6Rx, USART6>>>>,
    pub uart6_tx: UartTxPort<UartTxDmaSide<UartTxDma<S6Tx, USART6>>>,
    pub uart8_rx: Option<UartRxPort<UartRxIrqSide<UartRxDma<S8, UART8>>>>,
    pub functions: SerialFunctionSlots<SerialPortEndpoint>,
}

/// Start every bound port with its function's line settings and give each
/// function its endpoint.
pub fn init_h743_uart_ports<S3Rx, S3Tx, S6Rx, S6Tx, S8>(
    resources: H743UartPortResources<S3Rx, S3Tx, S6Rx, S6Tx, S8>,
    clocks: &CoreClocks,
    storage: H743UartPortStorage,
    bindings: &ResolvedBindings,
) -> H743UartPorts<S3Rx, S3Tx, S6Rx, S6Tx, S8>
where
    S3Rx: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    S3Tx: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    S6Rx: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    S6Tx: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    S8: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    let mut ports = H743UartPorts {
        uart3_rx: None,
        uart3_tx: UartTxPort::unbound(),
        uart6_rx: None,
        uart6_tx: UartTxPort::unbound(),
        uart8_rx: None,
        functions: SerialFunctionSlots::empty(),
    };

    if let Some(profile) = bindings.profile(LogicalSerialPort::Uart3) {
        let uart3 = resources.uart3;
        let serial = uart3
            .uart
            .serial(
                (
                    uart3.tx_pin.into_alternate::<7>(),
                    uart3.rx_pin.into_alternate::<7>(),
                ),
                stm32h7_uart_config(profile.protocol),
                uart3.prec,
                clocks,
            )
            .unwrap();
        let with_tx = profile.protocol.needs_tx();
        let (parts, tx_endpoint) = start_rx(
            serial.release(),
            uart3.rx_dma,
            profile.protocol,
            storage.uart3.rx.into_backend(),
            with_tx,
        );
        let writer = if with_tx {
            let transfer = Transfer::init(
                uart3.tx_dma,
                tx_endpoint,
                storage.uart3.tx_buffer,
                None,
                tx_dma_config(),
            );
            let (tx, writer) = tx_port(
                UartTxDmaSide::new(UartTxDma { transfer }),
                storage.uart3.tx_stream,
            );
            ports.uart3_tx = tx;
            Some(writer)
        } else {
            None
        };
        let (port, endpoint) = rx_port(
            LogicalSerialPort::Uart3,
            parts.irq,
            parts.parser,
            storage.uart3.stream,
            writer,
        );
        ports.uart3_rx = Some(port);
        place_endpoint(
            &mut ports.functions,
            bindings,
            LogicalSerialPort::Uart3,
            endpoint,
        );
    }

    if let Some(profile) = bindings.profile(LogicalSerialPort::Uart6) {
        let uart6 = resources.uart6;
        let serial = uart6
            .uart
            .serial(
                (
                    uart6.tx_pin.into_alternate::<7>(),
                    uart6.rx_pin.into_alternate::<7>(),
                ),
                stm32h7_uart_config(profile.protocol),
                uart6.prec,
                clocks,
            )
            .unwrap();
        let with_tx = profile.protocol.needs_tx();
        let (parts, tx_endpoint) = start_rx(
            serial.release(),
            uart6.rx_dma,
            profile.protocol,
            storage.uart6.rx.into_backend(),
            with_tx,
        );
        let writer = if with_tx {
            let transfer = Transfer::init(
                uart6.tx_dma,
                tx_endpoint,
                storage.uart6.tx_buffer,
                None,
                tx_dma_config(),
            );
            let (tx, writer) = tx_port(
                UartTxDmaSide::new(UartTxDma { transfer }),
                storage.uart6.tx_stream,
            );
            ports.uart6_tx = tx;
            Some(writer)
        } else {
            None
        };
        let (port, endpoint) = rx_port(
            LogicalSerialPort::Uart6,
            parts.irq,
            parts.parser,
            storage.uart6.stream,
            writer,
        );
        ports.uart6_rx = Some(port);
        place_endpoint(
            &mut ports.functions,
            bindings,
            LogicalSerialPort::Uart6,
            endpoint,
        );
    }

    if let Some(profile) = bindings.profile(LogicalSerialPort::Uart8) {
        let uart8 = resources.uart8;
        let serial = uart8
            .uart
            .serial(
                (NoTx, uart8.rx_pin.into_alternate::<8>()),
                stm32h7_uart_config(profile.protocol),
                uart8.prec,
                clocks,
            )
            .unwrap();
        let (parts, _) = start_rx(
            serial.release(),
            uart8.rx_dma,
            profile.protocol,
            storage.uart8.rx.into_backend(),
            false,
        );
        let (port, endpoint) = rx_port(
            LogicalSerialPort::Uart8,
            parts.irq,
            parts.parser,
            storage.uart8.stream,
            None,
        );
        ports.uart8_rx = Some(port);
        place_endpoint(
            &mut ports.functions,
            bindings,
            LogicalSerialPort::Uart8,
            endpoint,
        );
    }

    ports
}
