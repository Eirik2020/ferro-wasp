//! UART receive and transmit DMA on the STM32H7, under the shared UART
//! mechanisms in `ferrowasp_stm32f4::uart_common`.
//!
//! The HAL configures the UART and is then released: the receive side keeps
//! the UART for its idle-line flag, and both sides move bytes through a HAL
//! `Transfer` over a data-register endpoint from `dma_endpoints`.

use crate::dma_endpoints::{
    StreamErrors, UartDmaPeripheral, UartRxEndpoint, UartTxEndpoint, uart_endpoints,
};
use ferrowasp_io_core::serial::SerialProtocol as Mode;
pub use ferrowasp_stm32f4::uart_common::*;
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
    Transfer<S, UartTxEndpoint<U>, MemoryToPeripheral, Uart4TxBuf, DBTransfer>;

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

    fn next_transfer(&mut self, fresh: UartRxBuf) -> Result<UartRxBuf, ()> {
        // Single-buffer mode: the HAL stops the stream, which flushes its
        // FIFO to memory, before handing back the filled buffer.
        self.transfer
            .next_transfer(fresh)
            .map(|(filled, _, _)| filled)
            .map_err(|_| ())
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
        Mode::Msp | Mode::EscTelemetry => Config::new(115_200.Hz()),
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
pub struct Usart6SbusResources<S> {
    pub tx_pin: PC6,
    pub rx_pin: PC7,
    pub usart: USART6,
    pub prec: rec::Usart6,
    pub rx_dma: S,
}

pub struct Usart3MspResources<RxS, TxS> {
    pub tx_pin: PD8,
    pub rx_pin: PD9,
    pub usart: USART3,
    pub prec: rec::Usart3,
    pub rx_dma: RxS,
    pub tx_dma: TxS,
}

pub struct Uart8EscTelemetryResources<S> {
    pub rx_pin: PE0,
    pub uart: UART8,
    pub prec: rec::Uart8,
    pub rx_dma: S,
}

pub struct Usart3MspParts<RxS, TxS>
where
    RxS: Stream,
    TxS: Stream,
{
    pub rx_irq: UartRxIrqSide<UartRxDma<RxS, USART3>>,
    pub parser: UartRxParserSide,
    pub tx_dma: UartTxDmaSide<UartTxDma<TxS, USART3>>,
}

pub fn init_usart6_sbus<S>(
    resources: Usart6SbusResources<S>,
    clocks: &CoreClocks,
    storage: UartRxStorage,
) -> UartRxParts<UartRxDma<S, USART6>>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    let serial = resources
        .usart
        .serial(
            (
                resources.tx_pin.into_alternate::<7>(),
                resources.rx_pin.into_alternate::<7>(),
            ),
            stm32h7_uart_config(Mode::Sbus),
            resources.prec,
            clocks,
        )
        .unwrap();
    start_rx(
        serial.release(),
        resources.rx_dma,
        Mode::Sbus,
        storage,
        false,
    )
    .0
}

pub fn init_uart8_esc_telemetry<S>(
    resources: Uart8EscTelemetryResources<S>,
    clocks: &CoreClocks,
    storage: UartRxStorage,
) -> UartRxParts<UartRxDma<S, UART8>>
where
    S: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    let serial = resources
        .uart
        .serial(
            (NoTx, resources.rx_pin.into_alternate::<8>()),
            stm32h7_uart_config(Mode::EscTelemetry),
            resources.prec,
            clocks,
        )
        .unwrap();
    start_rx(
        serial.release(),
        resources.rx_dma,
        Mode::EscTelemetry,
        storage,
        false,
    )
    .0
}

pub fn init_usart3_msp_osd<RxS, TxS>(
    resources: Usart3MspResources<RxS, TxS>,
    clocks: &CoreClocks,
    rx_storage: UartRxStorage,
    tx_buffer: Uart4TxBuf,
) -> Usart3MspParts<RxS, TxS>
where
    RxS: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
    TxS: DoubleBufferedStream + Stream<Config = DmaConfig> + StreamErrors,
{
    let serial = resources
        .usart
        .serial(
            (
                resources.tx_pin.into_alternate::<7>(),
                resources.rx_pin.into_alternate::<7>(),
            ),
            stm32h7_uart_config(Mode::Msp),
            resources.prec,
            clocks,
        )
        .unwrap();
    let (parts, tx_endpoint) = start_rx(
        serial.release(),
        resources.rx_dma,
        Mode::Msp,
        rx_storage,
        true,
    );
    let tx_transfer = Transfer::init(
        resources.tx_dma,
        tx_endpoint,
        tx_buffer,
        None,
        tx_dma_config(),
    );

    Usart3MspParts {
        rx_irq: parts.irq,
        parser: parts.parser,
        tx_dma: UartTxDmaSide::new(UartTxDma {
            transfer: tx_transfer,
        }),
    }
}
