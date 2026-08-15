//! Task-facing UART interrupt and DMA service contracts.
//!
//! These traits keep reusable RTIC task bodies independent of the concrete
//! STM32F4 HAL transfer types while preserving the endpoint's real ownership
//! and failure semantics.

use ferrowasp_io_core::serial::TxChunk;

/// Failure encountered while rotating or publishing a received DMA buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartRxDeliveryError {
    /// No replacement buffer was available for the DMA transfer.
    NoFreshBuffer,

    /// The HAL rejected installation of the replacement DMA buffer.
    TransferNotReady,

    /// The completed-buffer queue had no remaining capacity.
    FilledQueueFull,

    /// The RX state planner rejected the completed buffer.
    PlannerRejected,
}

/// Result of servicing one UART receive interrupt source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartRxIrqOutcome {
    /// The interrupt source had no actionable flag.
    Ignored,

    /// A completed receive chunk was delivered to the endpoint.
    Delivered,

    /// The interrupt was acknowledged without producing a chunk.
    NoChunk,

    /// DMA reported a transfer, direct-mode, or FIFO error.
    DmaError,

    /// A completed buffer could not be delivered safely.
    DeliveryError(UartRxDeliveryError),
}

/// Receive-side operations required by STM32F4 UART interrupt tasks.
pub trait UartRxIrqService {
    /// Acknowledges and services the UART's RX DMA interrupt source.
    fn service_dma_irq(&mut self) -> UartRxIrqOutcome;

    /// Acknowledges and services the UART peripheral's IDLE interrupt source.
    fn service_idle_irq(&mut self) -> UartRxIrqOutcome;
}

/// Result of moving one completed DMA buffer into the owned receive channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartOwnedRxBridgeOutcome {
    /// No completed DMA buffer was waiting.
    NoChunk,
    /// One owned receive chunk was published.
    Published,
    /// The completed buffer did not describe a valid chunk.
    InvalidChunk,
    /// The bounded owned receive queue was full.
    QueueOverflow,
    /// The owned receive channel was disabled.
    Disabled,
    /// The DMA buffer could not be returned to the free pool.
    RecycleFailed,
}

/// Task-facing operation for draining a UART DMA parser into an owned channel.
pub trait UartOwnedRxBridgeService {
    /// Publishes one waiting chunk using an endpoint-local diagnostic timestamp.
    fn publish_next_untimed(&mut self) -> UartOwnedRxBridgeOutcome;
}

/// Failure encountered while starting one UART TX DMA chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartTxStartError {
    /// A previous DMA transfer is still active.
    Busy,

    /// The supplied owned chunk is malformed.
    InvalidChunk,

    /// The endpoint no longer owns a usable DMA transfer object.
    TransferMissing,
}

/// Terminal DMA fault reported by the UART transmit stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartTxDmaError {
    /// DMA reported a transfer error.
    Transfer,

    /// DMA reported a direct-mode error.
    DirectMode,
}

/// Result of servicing one UART transmit DMA interrupt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartTxIrqOutcome {
    /// The interrupt source had no terminal flag for an active transfer.
    Ignored,

    /// The active transfer completed successfully.
    Completed,

    /// The active transfer terminated with a DMA fault.
    DmaError(UartTxDmaError),
}

/// Transmit-side operations required by STM32F4 UART endpoint tasks.
pub trait UartTxDmaService<const N: usize> {
    /// Starts one DMA transfer from an endpoint-owned chunk.
    fn start_chunk(&mut self, chunk: &TxChunk<N>) -> Result<(), UartTxStartError>;

    /// Acknowledges and services the UART's TX DMA interrupt source.
    fn service_irq(&mut self) -> UartTxIrqOutcome;
}
