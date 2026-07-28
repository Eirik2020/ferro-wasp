use crate::serial::irq_plan::{RxIrqAction, RxIrqFlags, RxIrqPlanner};
use crate::serial::rx_state::{DetachedRxBuffer, RxDetachCause};
use crate::serial::tx_state::{
    TxDmaFinish, TxDmaIrqFlags, TxDmaIrqTerminal, TxDmaLifecycle, plan_tx_dma_irq,
};
use ferrowasp_io_core::serial::{
    Discontinuity, MSP_V1_MAX_FRAME_LEN, RxChunk, RxCompletion, SerialFault,
    SerialProtocol as Mode, SerialRxProducer, StreamGeneration, TxChunk,
};
use ferrowasp_io_core::time::TimestampMicros;
use heapless::spsc::{Consumer, Producer, Queue};
use stm32f4xx_hal::{
    dma::{
        ChannelX, MemoryToPeripheral, PeripheralToMemory, Stream2, Stream4, Stream5, Transfer,
        config::DmaConfig,
        traits::{Channel, DMASet, Stream},
    },
    gpio::{Input, PA0, PA1, PA2, PA3, PA10, PushPull},
    pac::{DMA1, DMA2, UART4, USART1, USART2},
    prelude::*,
    rcc::Rcc,
    serial::{self, RxListen, Serial},
};

pub const UART_RX_BUFFER_SIZE: usize = Mode::max_frame_size();
pub const UART_RX_QUEUE_CAPACITY: usize = 4;
pub const UART4_TX_BUFFER_SIZE: usize = MSP_V1_MAX_FRAME_LEN;

pub type UartRxBuf = &'static mut [u8; UART_RX_BUFFER_SIZE];
pub type Uart1RxIrq = UartRxIrqSide<Stream5<DMA2>, USART1, 4>;
pub type Uart2RxIrq = UartRxIrqSide<Stream5<DMA1>, USART2, 4>;
pub type Uart4RxIrq = UartRxIrqSide<Stream2<DMA1>, UART4, 4>;
pub type Uart4TxBuf = &'static mut [u8; UART4_TX_BUFFER_SIZE];
pub type Uart4TxTransfer =
    Transfer<Stream4<DMA1>, 4, serial::Tx<UART4>, MemoryToPeripheral, Uart4TxBuf>;

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

pub struct Uart4TxDmaSide {
    tx_transfer: Option<Uart4TxTransfer>,
    dma_config: DmaConfig,
    lifecycle: TxDmaLifecycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartTxStartError {
    Busy,
    InvalidChunk,
    TransferMissing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartTxIrqOutcome {
    Ignored,
    Completed,
    DmaError(UartTxDmaError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartTxDmaError {
    Transfer,
    DirectMode,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UartTxDmaStats {
    pub completed_chunks: u32,
    pub dma_errors: u32,
}

impl Uart4TxDmaSide {
    /// Starts one fixed-size UART4 DMA transfer.
    ///
    /// The current target keeps its validated 70-byte wire transfer and pads
    /// the owned chunk with zeros. A variable-length DMA buffer is a separate
    /// backend checkpoint.
    pub fn start_chunk(
        &mut self,
        chunk: &TxChunk<MSP_V1_MAX_FRAME_LEN>,
    ) -> Result<(), UartTxStartError> {
        if self.lifecycle.is_in_flight() {
            return Err(UartTxStartError::Busy);
        }
        let bytes = chunk
            .as_slice()
            .map_err(|_| UartTxStartError::InvalidChunk)?;
        let Some(old_transfer) = self.tx_transfer.take() else {
            return Err(UartTxStartError::TransferMissing);
        };

        let (stream, tx, buffer, _secondary_buffer) = old_transfer.release();
        buffer.fill(0);
        buffer[..bytes.len()].copy_from_slice(bytes);

        let mut transfer =
            Transfer::init_memory_to_peripheral(stream, tx, buffer, None, self.dma_config);
        transfer.start(|_tx| {});
        self.tx_transfer = Some(transfer);
        self.lifecycle.begin().map_err(|_| UartTxStartError::Busy)?;
        Ok(())
    }

    pub fn service_irq(&mut self) -> UartTxIrqOutcome {
        let Some(transfer) = self.tx_transfer.as_mut() else {
            return UartTxIrqOutcome::Ignored;
        };
        let flags = transfer.flags();
        if !self.lifecycle.is_in_flight() {
            if flags.is_transfer_error()
                || flags.is_direct_mode_error()
                || flags.is_fifo_error()
                || flags.is_transfer_complete()
            {
                transfer.clear_all_flags();
            }
            return UartTxIrqOutcome::Ignored;
        }

        let plan = plan_tx_dma_irq(TxDmaIrqFlags {
            transfer_complete: flags.is_transfer_complete(),
            transfer_error: flags.is_transfer_error(),
            direct_mode_error: flags.is_direct_mode_error(),
            fifo_error: flags.is_fifo_error(),
        });

        // Match the HAL UART DMA policy: FIFO error is advisory and does not
        // terminate the active transfer.
        if plan.clear_fifo_error {
            transfer.clear_fifo_error();
        }

        match plan.terminal {
            TxDmaIrqTerminal::None => UartTxIrqOutcome::Ignored,
            TxDmaIrqTerminal::Completed => {
                transfer.clear_all_flags();
                match self.lifecycle.complete() {
                    TxDmaFinish::Completed => UartTxIrqOutcome::Completed,
                    TxDmaFinish::Ignored | TxDmaFinish::DmaError => UartTxIrqOutcome::Ignored,
                }
            }
            TxDmaIrqTerminal::TransferError | TxDmaIrqTerminal::DirectModeError => {
                transfer.pause(|_tx| {});
                transfer.clear_all_flags();
                let error = match plan.terminal {
                    TxDmaIrqTerminal::TransferError => UartTxDmaError::Transfer,
                    TxDmaIrqTerminal::DirectModeError => UartTxDmaError::DirectMode,
                    TxDmaIrqTerminal::None | TxDmaIrqTerminal::Completed => {
                        return UartTxIrqOutcome::Ignored;
                    }
                };
                match self.lifecycle.dma_error() {
                    TxDmaFinish::DmaError => UartTxIrqOutcome::DmaError(error),
                    TxDmaFinish::Ignored | TxDmaFinish::Completed => UartTxIrqOutcome::Ignored,
                }
            }
        }
    }

    pub const fn stats(&self) -> UartTxDmaStats {
        let stats = self.lifecycle.stats();
        UartTxDmaStats {
            completed_chunks: stats.completed_chunks,
            dma_errors: stats.dma_errors,
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

    Uart4TxDmaSide {
        tx_transfer: Some(tx_transfer),
        dma_config,
        lifecycle: TxDmaLifecycle::new(),
    }
}

pub struct FilledUartRxBuf {
    pub buf: UartRxBuf,
    pub len: usize,
    pub completion: RxCompletion,
    pub generation: StreamGeneration,
    pub uart_error_seen: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartRxDeliveryError {
    NoFreshBuffer,
    TransferNotReady,
    FilledQueueFull,
    PlannerRejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartRxIrqOutcome {
    Ignored,
    Delivered,
    NoChunk,
    DmaError,
    DeliveryError(UartRxDeliveryError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UartRxIrqStats {
    pub dma_full_chunks: u32,
    pub idle_chunks: u32,
    pub dma_errors: u32,
    pub stale_events: u32,
    pub ignored_events: u32,
}

pub type FreeProducer = Producer<'static, UartRxBuf>;
pub type FreeConsumer = Consumer<'static, UartRxBuf>;

pub type FilledProducer = Producer<'static, FilledUartRxBuf>;
pub type FilledConsumer = Consumer<'static, FilledUartRxBuf>;

pub type FreeQueue = Queue<UartRxBuf, UART_RX_QUEUE_CAPACITY>;
pub type FilledQueue = Queue<FilledUartRxBuf, UART_RX_QUEUE_CAPACITY>;

pub type UartRxTransfer<StreamT, UsartT, const CHANNEL: u8> =
    Transfer<StreamT, CHANNEL, serial::Rx<UsartT>, PeripheralToMemory, UartRxBuf>;

pub struct UartRxStorage {
    pub buffer1: UartRxBuf,
    pub buffer2: UartRxBuf,
    pub buffer3: UartRxBuf,
    pub buffer4: UartRxBuf,

    pub free_queue: &'static mut FreeQueue,
    pub filled_queue: &'static mut FilledQueue,
}

pub struct UartRxIrqSide<StreamT, UsartT, const CHANNEL: u8>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    pub mode: Mode,
    pub rx_planner: RxIrqPlanner,
    pub rx_stats: UartRxIrqStats,
    pub transfer: UartRxTransfer<StreamT, UsartT, CHANNEL>,
    pub free_consumer: FreeConsumer,
    pub filled_producer: FilledProducer,
}

impl<StreamT, UsartT, const CHANNEL: u8> UartRxIrqSide<StreamT, UsartT, CHANNEL>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    pub fn service_dma_irq(&mut self) -> UartRxIrqOutcome {
        let flags = self.transfer.flags();

        if flags.is_transfer_error() || flags.is_direct_mode_error() || flags.is_fifo_error() {
            let _ = self.plan_dma_error();
            self.transfer.clear_all_flags();
            return UartRxIrqOutcome::DmaError;
        }

        if !flags.is_transfer_complete() {
            return UartRxIrqOutcome::Ignored;
        }

        let outcome = match self.deliver_dma_full_chunk() {
            Ok(true) => UartRxIrqOutcome::Delivered,
            Ok(false) => UartRxIrqOutcome::NoChunk,
            Err(error) => UartRxIrqOutcome::DeliveryError(error),
        };
        self.transfer.clear_all_flags();
        outcome
    }

    pub fn service_idle_irq(&mut self) -> UartRxIrqOutcome {
        if !self.transfer.is_idle() {
            return UartRxIrqOutcome::Ignored;
        }

        let received_len =
            UART_RX_BUFFER_SIZE.saturating_sub(self.transfer.number_of_transfers() as usize);
        let outcome = match self.deliver_idle_chunk(received_len) {
            Ok(true) => UartRxIrqOutcome::Delivered,
            Ok(false) => UartRxIrqOutcome::NoChunk,
            Err(error) => UartRxIrqOutcome::DeliveryError(error),
        };
        self.transfer.clear_idle_interrupt();
        outcome
    }

    pub fn plan_dma_full(&mut self) -> RxIrqAction {
        let observed_generation = self.rx_planner.state().generation();

        let action = self.rx_planner.handle(RxIrqFlags {
            observed_generation,
            dma_full: true,
            dma_error: false,
            idle: false,
            received_len: UART_RX_BUFFER_SIZE,
        });
        self.record_rx_action(action);
        action
    }

    pub fn plan_idle(&mut self, received_len: usize) -> RxIrqAction {
        let observed_generation = self.rx_planner.state().generation();

        let action = self.rx_planner.handle(RxIrqFlags {
            observed_generation,
            dma_full: false,
            dma_error: false,
            idle: true,
            received_len,
        });
        self.record_rx_action(action);
        action
    }

    pub fn plan_dma_error(&mut self) -> RxIrqAction {
        let observed_generation = self.rx_planner.state().generation();

        let action = self.rx_planner.handle(RxIrqFlags {
            observed_generation,
            dma_full: false,
            dma_error: true,
            idle: false,
            received_len: 0,
        });
        self.record_rx_action(action);
        action
    }

    pub fn rx_generation(&self) -> StreamGeneration {
        self.rx_planner.state().generation()
    }

    pub fn deliver_dma_full_chunk(&mut self) -> Result<bool, UartRxDeliveryError> {
        match self.plan_dma_full() {
            RxIrqAction::Deliver(detached) => self.rotate_rx_buffer(detached).map(|()| true),
            RxIrqAction::None | RxIrqAction::ClearIdle | RxIrqAction::Stale => Ok(false),
            RxIrqAction::Fault(_) => Err(UartRxDeliveryError::PlannerRejected),
        }
    }

    pub fn deliver_idle_chunk(&mut self, received_len: usize) -> Result<bool, UartRxDeliveryError> {
        if received_len == 0 {
            let _ = self.plan_idle(received_len);
            return Ok(false);
        }

        match self.plan_idle(received_len) {
            RxIrqAction::Deliver(detached) => self.rotate_rx_buffer(detached).map(|()| true),
            RxIrqAction::None | RxIrqAction::ClearIdle | RxIrqAction::Stale => {
                Err(UartRxDeliveryError::PlannerRejected)
            }
            RxIrqAction::Fault(_) => Err(UartRxDeliveryError::PlannerRejected),
        }
    }

    fn rotate_rx_buffer(&mut self, detached: DetachedRxBuffer) -> Result<(), UartRxDeliveryError> {
        let Some(fresh_buffer) = self.free_consumer.dequeue() else {
            return Err(UartRxDeliveryError::NoFreshBuffer);
        };

        let (raw_buffer, _) = self
            .transfer
            .next_transfer(fresh_buffer)
            .map_err(|_| UartRxDeliveryError::TransferNotReady)?;

        let filled_buffer = FilledUartRxBuf {
            buf: raw_buffer,
            len: detached.len,
            completion: match detached.cause {
                RxDetachCause::Idle => RxCompletion::Idle,
                RxDetachCause::Full => RxCompletion::DmaFull,
            },
            generation: detached.generation,
            uart_error_seen: false,
        };

        self.filled_producer
            .enqueue(filled_buffer)
            .map_err(|_| UartRxDeliveryError::FilledQueueFull)
    }

    fn record_rx_action(&mut self, action: RxIrqAction) {
        match action {
            RxIrqAction::Deliver(detached) => match detached.cause {
                RxDetachCause::Full => {
                    self.rx_stats.dma_full_chunks = self.rx_stats.dma_full_chunks.saturating_add(1);
                }
                RxDetachCause::Idle => {
                    self.rx_stats.idle_chunks = self.rx_stats.idle_chunks.saturating_add(1);
                }
            },
            RxIrqAction::Fault(_) => {
                self.rx_stats.dma_errors = self.rx_stats.dma_errors.saturating_add(1);
            }
            RxIrqAction::Stale => {
                self.rx_stats.stale_events = self.rx_stats.stale_events.saturating_add(1);
            }
            RxIrqAction::None | RxIrqAction::ClearIdle => {
                self.rx_stats.ignored_events = self.rx_stats.ignored_events.saturating_add(1);
            }
        }
    }
}

pub struct UartRxParserSide {
    pub free_producer: FreeProducer,
    pub filled_consumer: FilledConsumer,
}

pub struct UartOwnedRxBridge<'a, const N: usize, const DEPTH: usize> {
    parser: UartRxParserSide,
    producer: SerialRxProducer<'a, N, DEPTH>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartOwnedRxBridgeOutcome {
    NoChunk,
    Published,
    InvalidChunk,
    QueueOverflow,
    Disabled,
    RecycleFailed,
}

impl<'a, const N: usize, const DEPTH: usize> UartOwnedRxBridge<'a, N, DEPTH> {
    pub const fn new(parser: UartRxParserSide, producer: SerialRxProducer<'a, N, DEPTH>) -> Self {
        Self { parser, producer }
    }

    pub fn publish_next(&mut self, observed_at: TimestampMicros) -> UartOwnedRxBridgeOutcome {
        let Some(filled) = self.parser.filled_consumer.dequeue() else {
            return UartOwnedRxBridgeOutcome::NoChunk;
        };
        let len = filled.len.min(filled.buf.len());
        let chunk = RxChunk::from_slice(
            &filled.buf[..len],
            observed_at,
            filled.completion,
            filled.generation,
            filled.uart_error_seen,
        );

        if self.parser.free_producer.enqueue(filled.buf).is_err() {
            return UartOwnedRxBridgeOutcome::RecycleFailed;
        }

        let Ok(chunk) = chunk else {
            return UartOwnedRxBridgeOutcome::InvalidChunk;
        };
        match self.producer.try_send(chunk) {
            Ok(()) => UartOwnedRxBridgeOutcome::Published,
            Err(SerialFault::QueueOverflow) => UartOwnedRxBridgeOutcome::QueueOverflow,
            Err(SerialFault::Disabled) => UartOwnedRxBridgeOutcome::Disabled,
            Err(
                SerialFault::UnsupportedProtocol
                | SerialFault::DmaTransfer
                | SerialFault::Timeout
                | SerialFault::InvalidChunk
                | SerialFault::InvalidState,
            ) => UartOwnedRxBridgeOutcome::InvalidChunk,
        }
    }

    pub fn record_discontinuity(
        &mut self,
        cause: Discontinuity,
        generation: StreamGeneration,
        observed_at: TimestampMicros,
    ) {
        self.producer
            .record_discontinuity(cause, generation, observed_at);
    }
}

pub struct UartRxParts<StreamT, UsartT, const CHANNEL: u8>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    pub irq: UartRxIrqSide<StreamT, UsartT, CHANNEL>,
    pub parser: UartRxParserSide,
}

pub struct UartRxTxParts<StreamT, UsartT, const CHANNEL: u8>
where
    StreamT: Stream,
    UsartT: serial::Instance,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    pub irq: UartRxIrqSide<StreamT, UsartT, CHANNEL>,
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

pub fn init_uart_rx_only_dma<RxPin, UsartT, StreamT, const CHANNEL: u8>(
    rx_pin: RxPin,
    usart: UsartT,
    dma_stream: StreamT,
    rcc: &mut Rcc,
    mode: Mode,
    storage: UartRxStorage,
) -> UartRxParts<StreamT, UsartT, CHANNEL>
where
    UsartT: serial::Instance + serial::CommonPins,
    RxPin: Into<<UsartT as serial::CommonPins>::Rx<PushPull>>,
    StreamT: Stream,
    ChannelX<CHANNEL>: Channel,
    serial::Rx<UsartT>: DMASet<StreamT, CHANNEL, PeripheralToMemory>,
{
    let mut rx: serial::Rx<UsartT, u8> =
        Serial::rx(usart, rx_pin, stm32f4_uart_config(mode), rcc).unwrap();
    rx.listen_idle();

    storage.free_queue.enqueue(storage.buffer1).ok();
    storage.free_queue.enqueue(storage.buffer2).ok();
    storage.free_queue.enqueue(storage.buffer4).ok();

    let (free_producer, free_consumer) = storage.free_queue.split();
    let (filled_producer, filled_consumer) = storage.filled_queue.split();

    let mut transfer = Transfer::init_peripheral_to_memory(
        dma_stream,
        rx,
        storage.buffer3,
        None,
        DmaConfig::default()
            .memory_increment(true)
            .fifo_enable(true)
            .fifo_error_interrupt(true)
            .transfer_complete_interrupt(true),
    );
    transfer.start(|_| {});

    UartRxParts {
        irq: UartRxIrqSide {
            mode,
            rx_planner: RxIrqPlanner::new(UART_RX_BUFFER_SIZE),
            rx_stats: UartRxIrqStats::default(),
            transfer,
            free_consumer,
            filled_producer,
        },
        parser: UartRxParserSide {
            free_producer,
            filled_consumer,
        },
    }
}

pub fn init_uart_rx_dma<TxPin, RxPin, UsartT, StreamT, const CHANNEL: u8>(
    tx_pin: TxPin,
    rx_pin: RxPin,
    usart: UsartT,
    dma_stream: StreamT,
    rcc: &mut Rcc,
    mode: Mode,
    storage: UartRxStorage,
) -> UartRxParts<StreamT, UsartT, CHANNEL>
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

    let (_, mut rx) = serial.split();
    rx.listen_idle();

    storage.free_queue.enqueue(storage.buffer1).ok();
    storage.free_queue.enqueue(storage.buffer2).ok();
    storage.free_queue.enqueue(storage.buffer4).ok();

    let (free_producer, free_consumer) = storage.free_queue.split();
    let (filled_producer, filled_consumer) = storage.filled_queue.split();

    let mut transfer = Transfer::init_peripheral_to_memory(
        dma_stream,
        rx,
        storage.buffer3,
        None,
        DmaConfig::default()
            .memory_increment(true)
            .fifo_enable(true)
            .fifo_error_interrupt(true)
            .transfer_complete_interrupt(true),
    );

    transfer.start(|_| {});

    UartRxParts {
        irq: UartRxIrqSide {
            mode,
            rx_planner: RxIrqPlanner::new(UART_RX_BUFFER_SIZE),
            rx_stats: UartRxIrqStats::default(),
            transfer,
            free_consumer,
            filled_producer,
        },
        parser: UartRxParserSide {
            free_producer,
            filled_consumer,
        },
    }
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

    let (tx, mut rx) = serial.split();
    rx.listen_idle();

    storage.free_queue.enqueue(storage.buffer1).ok();
    storage.free_queue.enqueue(storage.buffer2).ok();
    storage.free_queue.enqueue(storage.buffer4).ok();

    let (free_producer, free_consumer) = storage.free_queue.split();
    let (filled_producer, filled_consumer) = storage.filled_queue.split();

    let mut transfer = Transfer::init_peripheral_to_memory(
        dma_stream,
        rx,
        storage.buffer3,
        None,
        DmaConfig::default()
            .memory_increment(true)
            .fifo_enable(true)
            .fifo_error_interrupt(true)
            .transfer_complete_interrupt(true),
    );

    transfer.start(|_| {});

    UartRxTxParts {
        irq: UartRxIrqSide {
            mode,
            rx_planner: RxIrqPlanner::new(UART_RX_BUFFER_SIZE),
            rx_stats: UartRxIrqStats::default(),
            transfer,
            free_consumer,
            filled_producer,
        },
        parser: UartRxParserSide {
            free_producer,
            filled_consumer,
        },
        tx,
    }
}

pub fn init_usart2_sbus_rx_dma(
    resources: Usart2SbusResources,
    rcc: &mut Rcc,
    storage: UartRxStorage,
) -> UartRxParts<Stream5<DMA1>, USART2, 4> {
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
) -> UartRxParts<Stream5<DMA2>, USART1, 4> {
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
) -> UartRxParts<Stream5<DMA1>, USART2, 4> {
    init_usart2_sbus_rx_dma(resources, rcc, storage.into_backend())
}

pub fn init_usart1_esc_telemetry(
    resources: Usart1EscTelemetryResources,
    rcc: &mut Rcc,
    storage: crate::app_storage::UartRxStorageResources,
) -> UartRxParts<Stream5<DMA2>, USART1, 4> {
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
