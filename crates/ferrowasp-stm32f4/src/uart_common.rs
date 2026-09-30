//! UART DMA receive and transmit mechanisms that do not depend on an STM32
//! family's HAL: buffer rotation, IRQ planning, the owned-stream bridge, and
//! the transmit lifecycle. Each family's backend supplies the DMA transfer
//! behind the two small traits here, so a board on either family runs the
//! same receive and transmit policy.

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

pub const UART_RX_BUFFER_SIZE: usize = Mode::max_frame_size();
pub const UART_RX_QUEUE_CAPACITY: usize = 4;
pub const UART4_TX_BUFFER_SIZE: usize = MSP_V1_MAX_FRAME_LEN;

pub type UartRxBuf = &'static mut [u8; UART_RX_BUFFER_SIZE];
pub type Uart4TxBuf = &'static mut [u8; UART4_TX_BUFFER_SIZE];

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

/// The status a memory-to-UART DMA stream reports in its interrupt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UartTxDmaFlags {
    pub transfer_complete: bool,
    pub transfer_error: bool,
    pub direct_mode_error: bool,
    pub fifo_error: bool,
}

impl UartTxDmaFlags {
    const fn any(self) -> bool {
        self.transfer_complete || self.transfer_error || self.direct_mode_error || self.fifo_error
    }
}

/// One memory-to-UART DMA stream with its fixed transmit buffer. A family
/// backend implements it over its own DMA driver.
pub trait UartTxDmaTransfer {
    /// Copy `bytes` into the transmit buffer, zero-padding the rest, and
    /// start the stream. `bytes` is never longer than the buffer.
    fn start_padded(&mut self, bytes: &[u8]);
    fn flags(&self) -> UartTxDmaFlags;
    fn clear_all_flags(&mut self);
    fn clear_fifo_error(&mut self);
    fn pause(&mut self);
}

/// A UART transmit DMA side: one chunk in flight at most, finished by the
/// stream's interrupt.
pub struct UartTxDmaSide<T> {
    transfer: Option<T>,
    lifecycle: TxDmaLifecycle,
}

impl<T> UartTxDmaSide<T>
where
    T: UartTxDmaTransfer,
{
    pub const fn new(transfer: T) -> Self {
        Self {
            transfer: Some(transfer),
            lifecycle: TxDmaLifecycle::new(),
        }
    }

    /// Starts one fixed-size UART DMA transfer.
    ///
    /// The current targets keep the validated 70-byte wire transfer and pad
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
        let Some(transfer) = self.transfer.as_mut() else {
            return Err(UartTxStartError::TransferMissing);
        };

        transfer.start_padded(bytes);
        self.lifecycle.begin().map_err(|_| UartTxStartError::Busy)?;
        Ok(())
    }

    pub fn service_irq(&mut self) -> UartTxIrqOutcome {
        let Some(transfer) = self.transfer.as_mut() else {
            return UartTxIrqOutcome::Ignored;
        };
        let flags = transfer.flags();
        if !self.lifecycle.is_in_flight() {
            if flags.any() {
                transfer.clear_all_flags();
            }
            return UartTxIrqOutcome::Ignored;
        }

        let plan = plan_tx_dma_irq(TxDmaIrqFlags {
            transfer_complete: flags.transfer_complete,
            transfer_error: flags.transfer_error,
            direct_mode_error: flags.direct_mode_error,
            fifo_error: flags.fifo_error,
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
                transfer.pause();
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

/// What a shared transmit task needs of a UART TX DMA side, whichever UART
/// and DMA stream a board wires it to.
pub trait UartTxDmaService {
    fn start_chunk(
        &mut self,
        chunk: &TxChunk<MSP_V1_MAX_FRAME_LEN>,
    ) -> Result<(), UartTxStartError>;

    fn service_irq(&mut self) -> UartTxIrqOutcome;
}

impl<T> UartTxDmaService for UartTxDmaSide<T>
where
    T: UartTxDmaTransfer,
{
    fn start_chunk(
        &mut self,
        chunk: &TxChunk<MSP_V1_MAX_FRAME_LEN>,
    ) -> Result<(), UartTxStartError> {
        UartTxDmaSide::start_chunk(self, chunk)
    }

    fn service_irq(&mut self) -> UartTxIrqOutcome {
        UartTxDmaSide::service_irq(self)
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

pub struct UartRxStorage {
    pub buffer1: UartRxBuf,
    pub buffer2: UartRxBuf,
    pub buffer3: UartRxBuf,
    pub buffer4: UartRxBuf,

    pub free_queue: &'static mut FreeQueue,
    pub filled_queue: &'static mut FilledQueue,
}

/// One UART-to-memory DMA stream and the UART's idle-line detector. A family
/// backend implements it over its own DMA driver and UART.
pub trait UartRxDmaTransfer {
    /// A transfer, direct-mode or FIFO error is pending.
    fn dma_error(&self) -> bool;
    fn transfer_complete(&self) -> bool;
    fn clear_all_flags(&mut self);
    /// The UART saw its line go idle.
    fn is_idle(&self) -> bool;
    fn clear_idle_interrupt(&mut self);
    /// Transfers the stream still has to make into the current buffer.
    fn number_of_transfers(&self) -> u16;
    /// Restart the stream into `fresh` and hand back the buffer it filled.
    /// On failure the stream keeps no buffer the caller can recover.
    fn next_transfer(&mut self, fresh: UartRxBuf) -> Result<UartRxBuf, ()>;
}

pub struct UartRxIrqSide<T> {
    pub mode: Mode,
    pub rx_planner: RxIrqPlanner,
    pub rx_stats: UartRxIrqStats,
    pub transfer: T,
    pub free_consumer: FreeConsumer,
    pub filled_producer: FilledProducer,
}

impl<T> UartRxIrqSide<T>
where
    T: UartRxDmaTransfer,
{
    pub fn service_dma_irq(&mut self) -> UartRxIrqOutcome {
        if self.transfer.dma_error() {
            let _ = self.plan_dma_error();
            self.transfer.clear_all_flags();
            return UartRxIrqOutcome::DmaError;
        }

        if !self.transfer.transfer_complete() {
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

        let raw_buffer = self
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

/// What a shared receive task needs of a UART RX IRQ side, whichever UART
/// and DMA stream a board wires it to. Each method forwards to the side's
/// own.
pub trait UartRxIrqService {
    fn service_dma_irq(&mut self) -> UartRxIrqOutcome;
    fn service_idle_irq(&mut self) -> UartRxIrqOutcome;
    fn rx_generation(&self) -> StreamGeneration;
}

impl<T> UartRxIrqService for UartRxIrqSide<T>
where
    T: UartRxDmaTransfer,
{
    fn service_dma_irq(&mut self) -> UartRxIrqOutcome {
        UartRxIrqSide::service_dma_irq(self)
    }

    fn service_idle_irq(&mut self) -> UartRxIrqOutcome {
        UartRxIrqSide::service_idle_irq(self)
    }

    fn rx_generation(&self) -> StreamGeneration {
        UartRxIrqSide::rx_generation(self)
    }
}

/// Prime the free queue with the spare buffers and split both queues. Returns
/// the buffer the receive stream starts into, and the two ends each side
/// keeps. Buffer 3 is the first DMA target on every backend.
pub fn split_uart_rx_storage(
    storage: UartRxStorage,
) -> (UartRxBuf, FreeConsumer, FilledProducer, UartRxParserSide) {
    storage.free_queue.enqueue(storage.buffer1).ok();
    storage.free_queue.enqueue(storage.buffer2).ok();
    storage.free_queue.enqueue(storage.buffer4).ok();

    let (free_producer, free_consumer) = storage.free_queue.split();
    let (filled_producer, filled_consumer) = storage.filled_queue.split();
    (
        storage.buffer3,
        free_consumer,
        filled_producer,
        UartRxParserSide {
            free_producer,
            filled_consumer,
        },
    )
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

/// A receive side and the parser end of its queues, as one UART initializer
/// returns them.
pub struct UartRxParts<T> {
    pub irq: UartRxIrqSide<T>,
    pub parser: UartRxParserSide,
}

/// Build the receive side for a stream already started into the first
/// buffer `split_uart_rx_storage` returned.
pub fn uart_rx_parts<T>(
    mode: Mode,
    transfer: T,
    free_consumer: FreeConsumer,
    filled_producer: FilledProducer,
    parser: UartRxParserSide,
) -> UartRxParts<T> {
    UartRxParts {
        irq: UartRxIrqSide {
            mode,
            rx_planner: RxIrqPlanner::new(UART_RX_BUFFER_SIZE),
            rx_stats: UartRxIrqStats::default(),
            transfer,
            free_consumer,
            filled_producer,
        },
        parser,
    }
}
