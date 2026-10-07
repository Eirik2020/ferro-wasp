pub use crate::uart_common::*;
use crate::uart_port::{
    SerialPortEndpoint, UartRxPort, UartRxPortStorage, UartRxTxPortStorage, UartTxPort,
    place_endpoint, rx_port, tx_port,
};
use ferrowasp_io_core::serial::{
    LogicalSerialPort, ResolvedBindings, SerialFunctionSlots, SerialProtocol as Mode,
};
use stm32f4xx_hal::{
    ClearFlags, ReadFlags,
    dma::{
        ChannelX, MemoryToPeripheral, PeripheralToMemory, Stream2, Stream4, Stream5, Transfer,
        config::DmaConfig,
        traits::{Channel, DMASet, DmaFlagExt, Stream},
    },
    gpio::{Input, PA0, PA1, PA2, PA3, PA10, PushPull},
    pac::{DMA1, DMA2, UART4, USART1, USART2},
    prelude::*,
    rcc::Rcc,
    serial::{self, RxISR, RxListen, Serial},
};

pub type Uart1RxIrq = UartRxIrqSide<UartRxTransfer<Stream5<DMA2>, USART1, 4>>;
pub type Uart2RxIrq = UartRxIrqSide<UartRxTransfer<Stream5<DMA1>, USART2, 4>>;
pub type Uart4RxIrq = UartRxIrqSide<UartRxTransfer<Stream2<DMA1>, UART4, 4>>;
pub type Uart4TxTransfer =
    Transfer<Stream4<DMA1>, 4, serial::Tx<UART4>, MemoryToPeripheral, UartTxBuf>;
pub type Uart4TxDmaSide = UartTxDmaSide<Uart4TxDma>;

/// UART2 (the USART2 peripheral) on PA2/PA3, RX on DMA1 Stream 5.
pub struct Uart2PortResources {
    pub tx_pin: PA2<Input>,
    pub rx_pin: PA3<Input>,
    pub uart: USART2,
    pub rx_dma: Stream5<DMA1>,
}

/// UART1 (the USART1 peripheral) receive-only on PA10, RX on DMA2 Stream 5.
pub struct Uart1PortResources {
    pub rx_pin: PA10<Input>,
    pub uart: USART1,
    pub rx_dma: Stream5<DMA2>,
}

/// UART4 on PA0/PA1, RX on DMA1 Stream 2 and TX on DMA1 Stream 4.
pub struct Uart4PortResources {
    pub tx_pin: PA0<Input>,
    pub rx_pin: PA1<Input>,
    pub uart: UART4,
    pub rx_dma: Stream2<DMA1>,
    pub tx_dma: Stream4<DMA1>,
}

/// UART4's transmit DMA stream and its fixed buffer. Each chunk re-creates
/// the HAL transfer around the same buffer.
pub struct Uart4TxDma {
    transfer: Option<Uart4TxTransfer>,
    dma_config: DmaConfig,
}

impl UartTxDmaTransfer for Uart4TxDma {
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

pub fn init_uart4_tx_dma(
    tx_dma: Stream4<DMA1>,
    tx: serial::Tx<UART4>,
    tx_buffer: UartTxBuf,
) -> Uart4TxDmaSide {
    let dma_config = DmaConfig::default()
        .memory_increment(true)
        .fifo_enable(true)
        .transfer_error_interrupt(true)
        .direct_mode_error_interrupt(true)
        .fifo_error_interrupt(true)
        .transfer_complete_interrupt(true);
    let tx_transfer = Transfer::init_memory_to_peripheral(tx_dma, tx, tx_buffer, None, dma_config);

    UartTxDmaSide::new(Uart4TxDma {
        transfer: Some(tx_transfer),
        dma_config,
    })
}

pub type UartRxTransfer<StreamT, UsartT, const CHANNEL: u8> =
    Transfer<StreamT, CHANNEL, serial::Rx<UsartT>, PeripheralToMemory, UartRxBuf>;

impl<StreamT, UsartT, const CHANNEL: u8> UartRxDmaTransfer
    for UartRxTransfer<StreamT, UsartT, CHANNEL>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    fn dma_error(&self) -> bool {
        let flags = ReadFlags::flags(self);
        flags.is_transfer_error() || flags.is_direct_mode_error() || flags.is_fifo_error()
    }

    fn transfer_complete(&self) -> bool {
        ReadFlags::flags(self).is_transfer_complete()
    }

    fn clear_all_flags(&mut self) {
        ClearFlags::clear_all_flags(self);
    }

    fn is_idle(&self) -> bool {
        RxISR::is_idle(self)
    }

    fn clear_idle_interrupt(&mut self) {
        RxISR::clear_idle_interrupt(self);
    }

    fn number_of_transfers(&self) -> u16 {
        Transfer::number_of_transfers(self)
    }

    fn next_transfer(&mut self, fresh: UartRxBuf) -> Result<UartRxBuf, UartRxRestartError> {
        Transfer::<StreamT, CHANNEL, serial::Rx<UsartT>, PeripheralToMemory, UartRxBuf>::next_transfer(
            self, fresh,
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

        Mode::Crsf => serial::Config::default()
            .baudrate(420_000.bps())
            .dma(serial::config::DmaConfig::Rx),

        Mode::EscTelemetry => serial::Config::default()
            .baudrate(115_200.bps())
            .dma(serial::config::DmaConfig::Rx),

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

    uart_rx_parts(mode, transfer, free_consumer, filled_producer, parser)
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

/// Every UART the STM32F405 flight boards route, by logical port.
pub struct F405UartPortResources {
    pub uart1: Uart1PortResources,
    pub uart2: Uart2PortResources,
    pub uart4: Uart4PortResources,
}

/// Static buffers and stream owners for each port.
pub struct F405UartPortStorage {
    pub uart1: UartRxPortStorage,
    pub uart2: UartRxPortStorage,
    pub uart4: UartRxTxPortStorage,
}

/// The started ports. A port left unbound stays `None`: its peripheral is
/// never enabled, so its interrupts never fire.
pub struct F405UartPorts {
    pub uart1_rx: Option<UartRxPort<Uart1RxIrq>>,
    pub uart2_rx: Option<UartRxPort<Uart2RxIrq>>,
    pub uart4_rx: Option<UartRxPort<Uart4RxIrq>>,
    pub uart4_tx: UartTxPort<Uart4TxDmaSide>,
    pub functions: SerialFunctionSlots<SerialPortEndpoint>,
}

/// Start every bound port with its function's line settings and give each
/// function its endpoint.
pub fn init_f405_uart_ports(
    resources: F405UartPortResources,
    rcc: &mut Rcc,
    storage: F405UartPortStorage,
    bindings: &ResolvedBindings,
) -> F405UartPorts {
    let mut ports = F405UartPorts {
        uart1_rx: None,
        uart2_rx: None,
        uart4_rx: None,
        uart4_tx: UartTxPort::unbound(),
        functions: SerialFunctionSlots::empty(),
    };

    if let Some(profile) = bindings.profile(LogicalSerialPort::Uart1) {
        let parts = init_uart_rx_only_dma::<_, _, _, 4>(
            resources.uart1.rx_pin.into_alternate::<7>(),
            resources.uart1.uart,
            resources.uart1.rx_dma,
            rcc,
            profile.protocol,
            storage.uart1.rx.into_backend(),
        );
        let (port, endpoint) = rx_port(
            LogicalSerialPort::Uart1,
            parts.irq,
            parts.parser,
            storage.uart1.stream,
            None,
        );
        ports.uart1_rx = Some(port);
        place_endpoint(
            &mut ports.functions,
            bindings,
            LogicalSerialPort::Uart1,
            endpoint,
        );
    }

    if let Some(profile) = bindings.profile(LogicalSerialPort::Uart2) {
        let parts = init_uart_rx_dma::<_, _, _, _, 4>(
            resources.uart2.tx_pin.into_alternate::<7>(),
            resources.uart2.rx_pin.into_alternate::<7>(),
            resources.uart2.uart,
            resources.uart2.rx_dma,
            rcc,
            profile.protocol,
            storage.uart2.rx.into_backend(),
        );
        let (port, endpoint) = rx_port(
            LogicalSerialPort::Uart2,
            parts.irq,
            parts.parser,
            storage.uart2.stream,
            None,
        );
        ports.uart2_rx = Some(port);
        place_endpoint(
            &mut ports.functions,
            bindings,
            LogicalSerialPort::Uart2,
            endpoint,
        );
    }

    if let Some(profile) = bindings.profile(LogicalSerialPort::Uart4) {
        let tx_pin = resources.uart4.tx_pin.into_alternate::<8>();
        let rx_pin = resources.uart4.rx_pin.into_alternate::<8>();
        let (irq, parser, writer) = if profile.protocol.needs_tx() {
            let parts = init_uart_rx_dma_with_tx::<_, _, _, _, 4>(
                tx_pin,
                rx_pin,
                resources.uart4.uart,
                resources.uart4.rx_dma,
                rcc,
                profile.protocol,
                storage.uart4.rx.into_backend(),
            );
            let tx_dma =
                init_uart4_tx_dma(resources.uart4.tx_dma, parts.tx, storage.uart4.tx_buffer);
            let (tx, writer) = tx_port(tx_dma, storage.uart4.tx_stream);
            ports.uart4_tx = tx;
            (parts.irq, parts.parser, Some(writer))
        } else {
            let parts = init_uart_rx_dma::<_, _, _, _, 4>(
                tx_pin,
                rx_pin,
                resources.uart4.uart,
                resources.uart4.rx_dma,
                rcc,
                profile.protocol,
                storage.uart4.rx.into_backend(),
            );
            (parts.irq, parts.parser, None)
        };
        let (port, endpoint) = rx_port(
            LogicalSerialPort::Uart4,
            irq,
            parser,
            storage.uart4.stream,
            writer,
        );
        ports.uart4_rx = Some(port);
        place_endpoint(
            &mut ports.functions,
            bindings,
            LogicalSerialPort::Uart4,
            endpoint,
        );
    }

    ports
}
