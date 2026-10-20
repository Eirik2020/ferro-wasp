use ferrowasp_serial::{
    LogicalSerialPort, ResolvedBindings, SerialFunctionSlots, SerialProtocol as Mode,
};
pub use ferrowasp_stm32_critical::uart_common::*;
use ferrowasp_stm32_critical::uart_port::{
    SerialPortEndpoint, UartRxPort, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
    place_endpoint, rx_port, tx_port,
};
use stm32f4xx_hal::{
    ClearFlags, ReadFlags,
    dma::{
        ChannelX, MemoryToPeripheral, PeripheralToMemory, Transfer,
        config::DmaConfig,
        traits::{Channel, DMASet, DmaFlagExt, Stream},
    },
    gpio::PushPull,
    prelude::*,
    rcc::Rcc,
    serial::{self, RxISR, RxListen, Serial},
};

pub type UartTxTransfer<StreamT, UsartT, const CHANNEL: u8> =
    Transfer<StreamT, CHANNEL, serial::Tx<UsartT>, MemoryToPeripheral, UartTxBuf>;

/// A UART's transmit DMA stream and its fixed buffer. Each chunk re-creates
/// the HAL transfer around the same buffer.
pub struct UartTxDma<StreamT, UsartT, const CHANNEL: u8>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Tx<UsartT>: DMASet<StreamT, CHANNEL, MemoryToPeripheral>,
{
    transfer: Option<UartTxTransfer<StreamT, UsartT, CHANNEL>>,
    dma_config: DmaConfig,
}

impl<StreamT, UsartT, const CHANNEL: u8> UartTxDmaTransfer for UartTxDma<StreamT, UsartT, CHANNEL>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Tx<UsartT>: DMASet<StreamT, CHANNEL, MemoryToPeripheral>,
{
    fn start_padded(&mut self, bytes: &[u8]) {
        let Some(old_transfer) = self.transfer.take() else {
            return;
        };
        let (stream, tx, buffer, _secondary_buffer) = old_transfer.release();
        buffer.fill(0);
        buffer[..bytes.len()].copy_from_slice(bytes);

        let mut transfer =
            Transfer::init_memory_to_peripheral(stream, tx, buffer, None, self.dma_config);
        transfer.start(|_tx| {});
        self.transfer = Some(transfer);
    }

    fn flags(&self) -> UartTxDmaFlags {
        let Some(transfer) = self.transfer.as_ref() else {
            return UartTxDmaFlags::default();
        };
        let flags = ReadFlags::flags(transfer);
        UartTxDmaFlags {
            transfer_complete: flags.is_transfer_complete(),
            transfer_error: flags.is_transfer_error(),
            direct_mode_error: flags.is_direct_mode_error(),
            fifo_error: flags.is_fifo_error(),
        }
    }

    fn clear_all_flags(&mut self) {
        if let Some(transfer) = self.transfer.as_mut() {
            ClearFlags::clear_all_flags(transfer);
        }
    }

    fn clear_fifo_error(&mut self) {
        if let Some(transfer) = self.transfer.as_mut() {
            transfer.clear_fifo_error();
        }
    }

    fn pause(&mut self) {
        if let Some(transfer) = self.transfer.as_mut() {
            transfer.pause(|_tx| {});
        }
    }
}

/// Start a UART's transmit DMA side on `tx_dma`.
pub fn init_uart_tx_dma<StreamT, UsartT, const CHANNEL: u8>(
    tx_dma: StreamT,
    tx: serial::Tx<UsartT>,
    tx_buffer: UartTxBuf,
) -> UartTxDmaSide<UartTxDma<StreamT, UsartT, CHANNEL>>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Tx<UsartT>: DMASet<StreamT, CHANNEL, MemoryToPeripheral>,
{
    let dma_config = DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .transfer_error_interrupt(true)
        .direct_mode_error_interrupt(true)
        .fifo_error_interrupt(true)
        .transfer_complete_interrupt(true);
    let tx_transfer = Transfer::init_memory_to_peripheral(tx_dma, tx, tx_buffer, None, dma_config);

    UartTxDmaSide::new(UartTxDma {
        transfer: Some(tx_transfer),
        dma_config,
    })
}

/// A UART's receive DMA transfer. A type of this crate's own, so it can
/// carry the chip-neutral `UartRxDmaTransfer`.
pub struct UartRxTransfer<StreamT, UsartT, const CHANNEL: u8>(
    pub Transfer<StreamT, CHANNEL, serial::Rx<UsartT>, PeripheralToMemory, UartRxBuf>,
)
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>;

impl<StreamT, UsartT, const CHANNEL: u8> UartRxDmaTransfer
    for UartRxTransfer<StreamT, UsartT, CHANNEL>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    fn dma_error(&self) -> bool {
        let flags = ReadFlags::flags(&self.0);
        flags.is_transfer_error() || flags.is_direct_mode_error() || flags.is_fifo_error()
    }

    fn transfer_complete(&self) -> bool {
        ReadFlags::flags(&self.0).is_transfer_complete()
    }

    fn clear_all_flags(&mut self) {
        ClearFlags::clear_all_flags(&mut self.0);
    }

    fn is_idle(&self) -> bool {
        RxISR::is_idle(&self.0)
    }

    fn clear_idle_interrupt(&mut self) {
        RxISR::clear_idle_interrupt(&self.0);
    }

    fn number_of_transfers(&self) -> u16 {
        Transfer::number_of_transfers(&self.0)
    }

    fn next_transfer(&mut self, fresh: UartRxBuf) -> Result<UartRxBuf, UartRxRestartError> {
        Transfer::<StreamT, CHANNEL, serial::Rx<UsartT>, PeripheralToMemory, UartRxBuf>::next_transfer(
            &mut self.0, fresh,
        )
        .map(|(raw_buffer, _)| raw_buffer)
            .map_err(|_| UartRxRestartError)
    }
}

pub struct UartRxTxParts<StreamT, UsartT, const CHANNEL: u8>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    pub irq: UartRxIrqSide<UartRxTransfer<StreamT, UsartT, CHANNEL>>,
    pub parser: UartRxParserSide,
    pub tx: serial::Tx<UsartT>,
}

pub fn stm32f4_uart_config(mode: Mode) -> serial::Config {
    match mode {
        Mode::Sbus => serial::Config::default()
            .baudrate(100_000.bps())
            .wordlength_9()
            .parity_even()
            .stopbits(serial::config::StopBits::STOP2)
            .dma(serial::config::DmaConfig::Rx),

        Mode::Msp => serial::Config::default()
            .baudrate(115_200.bps())
            .dma(serial::config::DmaConfig::TxRx),

        Mode::Mavlink => serial::Config::default()
            .baudrate(57_600.bps())
            .dma(serial::config::DmaConfig::Rx),

        // CRSF talks back: telemetry leaves by TX DMA.
        Mode::Crsf => serial::Config::default()
            .baudrate(420_000.bps())
            .dma(serial::config::DmaConfig::TxRx),

        Mode::EscTelemetry => serial::Config::default()
            .baudrate(115_200.bps())
            .dma(serial::config::DmaConfig::Rx),

        // The command line answers every request.
        Mode::Cli => serial::Config::default()
            .baudrate(115_200.bps())
            .dma(serial::config::DmaConfig::TxRx),

        Mode::Disabled => serial::Config::default(),
    }
}

fn rx_dma_config() -> DmaConfig {
    DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .fifo_error_interrupt(true)
        .transfer_complete_interrupt(true)
}

fn start_rx_dma<StreamT, UsartT, const CHANNEL: u8>(
    mut rx: serial::Rx<UsartT, u8>,
    dma_stream: StreamT,
    mode: Mode,
    storage: UartRxStorage,
) -> UartRxParts<UartRxTransfer<StreamT, UsartT, CHANNEL>>
where
    UsartT: serial::Instance,
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    rx.listen_idle();

    let (first_buffer, free_consumer, filled_producer, parser) = split_uart_rx_storage(storage);
    let mut transfer =
        Transfer::init_peripheral_to_memory(dma_stream, rx, first_buffer, None, rx_dma_config());
    transfer.start(|_| {});

    uart_rx_parts(
        mode,
        UartRxTransfer(transfer),
        free_consumer,
        filled_producer,
        parser,
    )
}

pub fn init_uart_rx_only_dma<RxPin, UsartT, StreamT, const CHANNEL: u8>(
    rx_pin: RxPin,
    usart: UsartT,
    dma_stream: StreamT,
    rcc: &mut Rcc,
    mode: Mode,
    storage: UartRxStorage,
) -> UartRxParts<UartRxTransfer<StreamT, UsartT, CHANNEL>>
where
    UsartT: serial::Instance + serial::CommonPins,
    RxPin: Into<<UsartT as serial::CommonPins>::Rx<PushPull>>,
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    let rx: serial::Rx<UsartT, u8> =
        Serial::rx(usart, rx_pin, stm32f4_uart_config(mode), rcc).unwrap();
    start_rx_dma(rx, dma_stream, mode, storage)
}

pub fn init_uart_rx_dma<TxPin, RxPin, UsartT, StreamT, const CHANNEL: u8>(
    tx_pin: TxPin,
    rx_pin: RxPin,
    usart: UsartT,
    dma_stream: StreamT,
    rcc: &mut Rcc,
    mode: Mode,
    storage: UartRxStorage,
) -> UartRxParts<UartRxTransfer<StreamT, UsartT, CHANNEL>>
where
    UsartT: serial::Instance + serial::CommonPins,
    TxPin: Into<<UsartT as serial::CommonPins>::Tx<PushPull>>,
    RxPin: Into<<UsartT as serial::CommonPins>::Rx<PushPull>>,
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    let serial: Serial<UsartT, u8> =
        Serial::new(usart, (tx_pin, rx_pin), stm32f4_uart_config(mode), rcc).unwrap();

    let (_, rx) = serial.split();
    start_rx_dma(rx, dma_stream, mode, storage)
}

pub fn init_uart_rx_dma_with_tx<TxPin, RxPin, UsartT, StreamT, const CHANNEL: u8>(
    tx_pin: TxPin,
    rx_pin: RxPin,
    usart: UsartT,
    dma_stream: StreamT,
    rcc: &mut Rcc,
    mode: Mode,
    storage: UartRxStorage,
) -> UartRxTxParts<StreamT, UsartT, CHANNEL>
where
    UsartT: serial::Instance + serial::CommonPins,
    TxPin: Into<<UsartT as serial::CommonPins>::Tx<PushPull>>,
    RxPin: Into<<UsartT as serial::CommonPins>::Rx<PushPull>>,
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    let serial: Serial<UsartT, u8> =
        Serial::new(usart, (tx_pin, rx_pin), stm32f4_uart_config(mode), rcc).unwrap();

    let (tx, rx) = serial.split();
    let parts = start_rx_dma(rx, dma_stream, mode, storage);

    UartRxTxParts {
        irq: parts.irq,
        parser: parts.parser,
        tx,
    }
}

/// The receive side of a started port, for a board's shared resources.
pub type UartRxIrq<StreamT, UsartT, const CHANNEL: u8> =
    UartRxIrqSide<UartRxTransfer<StreamT, UsartT, CHANNEL>>;
/// The transmit side of a started port.
pub type UartTxSide<StreamT, UsartT, const CHANNEL: u8> =
    UartTxDmaSide<UartTxDma<StreamT, UsartT, CHANNEL>>;

/// A started port that can transmit: the receive side when it is bound, and
/// the transmit side.
pub type StartedRxTxPort<RxStreamT, TxStreamT, UsartT, const RX_CHANNEL: u8, const TX_CHANNEL: u8> = (
    Option<UartRxPort<UartRxIrq<RxStreamT, UsartT, RX_CHANNEL>>>,
    UartTxPort<UartTxSide<TxStreamT, UsartT, TX_CHANNEL>>,
);

/// Start a port that can transmit, if `bindings` gives it a function: the
/// receive side always, and the transmit side when the bound protocol
/// transmits. The pins arrive already in their alternate function, so the
/// alternate-function number and any pull stay board facts. An unbound port
/// is never enabled and returns `None` with an unbound transmit side.
#[allow(clippy::too_many_arguments)]
pub fn start_rx_tx_port<
    TxPin,
    RxPin,
    UsartT,
    RxStreamT,
    TxStreamT,
    const RX_CHANNEL: u8,
    const TX_CHANNEL: u8,
>(
    logical: LogicalSerialPort,
    tx_pin: TxPin,
    rx_pin: RxPin,
    usart: UsartT,
    rx_dma: RxStreamT,
    tx_dma: TxStreamT,
    rcc: &mut Rcc,
    storage: UartRxTxPortStorage,
    bindings: &ResolvedBindings,
    functions: &mut SerialFunctionSlots<SerialPortEndpoint>,
) -> StartedRxTxPort<RxStreamT, TxStreamT, UsartT, RX_CHANNEL, TX_CHANNEL>
where
    UsartT: serial::Instance + serial::CommonPins,
    TxPin: Into<<UsartT as serial::CommonPins>::Tx<PushPull>>,
    RxPin: Into<<UsartT as serial::CommonPins>::Rx<PushPull>>,
    RxStreamT: Stream,
    TxStreamT: Stream,
    ChannelX<RX_CHANNEL>: Channel,
    ChannelX<TX_CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<RxStreamT, RX_CHANNEL, PeripheralToMemory>,
    serial::Tx<UsartT>: DMASet<TxStreamT, TX_CHANNEL, MemoryToPeripheral>,
{
    let Some(profile) = bindings.profile(logical) else {
        return (None, UartTxPort::unbound());
    };
    let (irq, parser, writer, tx_port_side) = if profile.protocol.needs_tx() {
        let parts = init_uart_rx_dma_with_tx::<_, _, _, _, RX_CHANNEL>(
            tx_pin,
            rx_pin,
            usart,
            rx_dma,
            rcc,
            profile.protocol,
            storage.rx.into_backend(),
        );
        let tx_dma = init_uart_tx_dma::<_, _, TX_CHANNEL>(tx_dma, parts.tx, storage.tx_buffer);
        let (tx, writer) = tx_port(tx_dma, storage.tx_stream);
        (parts.irq, parts.parser, Some(writer), tx)
    } else {
        let parts = init_uart_rx_dma::<_, _, _, _, RX_CHANNEL>(
            tx_pin,
            rx_pin,
            usart,
            rx_dma,
            rcc,
            profile.protocol,
            storage.rx.into_backend(),
        );
        (parts.irq, parts.parser, None, UartTxPort::unbound())
    };
    let (port, endpoint) = rx_port(logical, irq, parser, storage.stream, writer);
    place_endpoint(functions, bindings, logical, endpoint);
    (Some(port), tx_port_side)
}

/// Start a receive-only port, if `bindings` gives it a function.
#[allow(clippy::too_many_arguments)]
pub fn start_rx_port<RxPin, UsartT, StreamT, const CHANNEL: u8>(
    logical: LogicalSerialPort,
    rx_pin: RxPin,
    usart: UsartT,
    rx_dma: StreamT,
    rcc: &mut Rcc,
    storage: UartRxPortStorage,
    bindings: &ResolvedBindings,
    functions: &mut SerialFunctionSlots<SerialPortEndpoint>,
) -> Option<UartRxPort<UartRxIrq<StreamT, UsartT, CHANNEL>>>
where
    UsartT: serial::Instance + serial::CommonPins,
    RxPin: Into<<UsartT as serial::CommonPins>::Rx<PushPull>>,
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    let profile = bindings.profile(logical)?;
    let parts = init_uart_rx_only_dma::<_, _, _, CHANNEL>(
        rx_pin,
        usart,
        rx_dma,
        rcc,
        profile.protocol,
        storage.rx.into_backend(),
    );
    let (port, endpoint) = rx_port(logical, parts.irq, parts.parser, storage.stream, None);
    place_endpoint(functions, bindings, logical, endpoint);
    Some(port)
}
