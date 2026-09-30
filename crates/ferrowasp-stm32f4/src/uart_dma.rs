pub use crate::uart_common::*;
use ferrowasp_io_core::serial::SerialProtocol as Mode;
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
    Transfer<Stream4<DMA1>, 4, serial::Tx<UART4>, MemoryToPeripheral, Uart4TxBuf>;
pub type Uart4TxDmaSide = UartTxDmaSide<Uart4TxDma>;

pub struct Usart2SbusResources {
    pub tx_pin: PA2<Input>,
    pub rx_pin: PA3<Input>,
    pub usart: USART2,
    pub rx_dma: Stream5<DMA1>,
}

pub struct Usart1EscTelemetryResources {
    pub rx_pin: PA10<Input>,
    pub usart: USART1,
    pub rx_dma: Stream5<DMA2>,
}

pub struct Uart4MspResources {
    pub tx_pin: PA0<Input>,
    pub rx_pin: PA1<Input>,
    pub uart: UART4,
    pub rx_dma: Stream2<DMA1>,
    pub tx_dma: Stream4<DMA1>,
}

pub struct Uart4MspRxResources {
    pub tx_pin: PA0<Input>,
    pub rx_pin: PA1<Input>,
    pub uart: UART4,
    pub rx_dma: Stream2<DMA1>,
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

pub struct Uart4MspParts {
    pub rx_irq: Uart4RxIrq,
    pub parser: UartRxParserSide,
    pub tx_dma: Uart4TxDmaSide,
}

pub fn init_uart4_tx_dma(
    tx_dma: Stream4<DMA1>,
    tx: serial::Tx<UART4>,
    tx_buffer: Uart4TxBuf,
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

pub fn init_usart2_sbus_rx_dma(
    resources: Usart2SbusResources,
    rcc: &mut Rcc,
    storage: UartRxStorage,
) -> UartRxParts<UartRxTransfer<Stream5<DMA1>, USART2, 4>> {
    init_uart_rx_dma::<_, _, _, _, 4>(
        resources.tx_pin.into_alternate::<7>(),
        resources.rx_pin.into_alternate::<7>(),
        resources.usart,
        resources.rx_dma,
        rcc,
        Mode::Sbus,
        storage,
    )
}

pub fn init_usart1_esc_telemetry_rx_dma(
    resources: Usart1EscTelemetryResources,
    rcc: &mut Rcc,
    storage: UartRxStorage,
) -> UartRxParts<UartRxTransfer<Stream5<DMA2>, USART1, 4>> {
    init_uart_rx_only_dma::<_, _, _, 4>(
        resources.rx_pin.into_alternate::<7>(),
        resources.usart,
        resources.rx_dma,
        rcc,
        Mode::EscTelemetry,
        storage,
    )
}

pub fn init_uart4_msp_rx_dma_with_tx(
    resources: Uart4MspRxResources,
    rcc: &mut Rcc,
    storage: UartRxStorage,
) -> UartRxTxParts<Stream2<DMA1>, UART4, 4> {
    init_uart_rx_dma_with_tx::<_, _, _, _, 4>(
        resources.tx_pin.into_alternate::<8>(),
        resources.rx_pin.into_alternate::<8>(),
        resources.uart,
        resources.rx_dma,
        rcc,
        Mode::Msp,
        storage,
    )
}

pub fn init_usart2_sbus(
    resources: Usart2SbusResources,
    rcc: &mut Rcc,
    storage: crate::app_storage::UartRxStorageResources,
) -> UartRxParts<UartRxTransfer<Stream5<DMA1>, USART2, 4>> {
    init_usart2_sbus_rx_dma(resources, rcc, storage.into_backend())
}

pub fn init_usart1_esc_telemetry(
    resources: Usart1EscTelemetryResources,
    rcc: &mut Rcc,
    storage: crate::app_storage::UartRxStorageResources,
) -> UartRxParts<UartRxTransfer<Stream5<DMA2>, USART1, 4>> {
    init_usart1_esc_telemetry_rx_dma(resources, rcc, storage.into_backend())
}

pub fn init_uart4_msp_osd(
    resources: Uart4MspResources,
    rcc: &mut Rcc,
    rx_storage: crate::app_storage::UartRxStorageResources,
    tx_buffer: Uart4TxBuf,
) -> Uart4MspParts {
    let Uart4MspResources {
        tx_pin,
        rx_pin,
        uart,
        rx_dma,
        tx_dma,
    } = resources;
    let uart4 = init_uart4_msp_rx_dma_with_tx(
        Uart4MspRxResources {
            tx_pin,
            rx_pin,
            uart,
            rx_dma,
        },
        rcc,
        rx_storage.into_backend(),
    );

    Uart4MspParts {
        rx_irq: uart4.irq,
        parser: uart4.parser,
        tx_dma: init_uart4_tx_dma(tx_dma, uart4.tx, tx_buffer),
    }
}
