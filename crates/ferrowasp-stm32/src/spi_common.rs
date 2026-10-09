//! SPI DMA ownership that does not depend on an STM32 family's HAL: the
//! request mailbox service, timeout recovery, and RX buffer rotation. Each
//! family's backend supplies the DMA streams behind `SpiRxTransferExt` and
//! `SpiTxFrameDma`, so a board on either family runs the same SPI policy.

pub use crate::memory::{SPI1_JOB_MAX_BYTES, SPI1_JOB_MAX_OPERATIONS};
use crate::spi::{SpiRxIrqAction, SpiRxIrqFlags, SpiRxIrqPlanner};
use embedded_hal::digital::OutputPin;
use ferrowasp_io_core::{
    spi::{OwnedSpiOperation, SpiFault, SpiJobToken, SpiMailboxOwnerAction, SpiRequestMailbox},
    time::TimestampMicros,
};
use heapless::spsc::{Consumer, Producer, Queue};

pub const SPI_ARRAY_SIZE: usize = 15;
pub const SPI_BUFFER_SIZE: usize = SPI_ARRAY_SIZE;

pub const SPI_RX_QUEUE_CAPACITY: usize = 4;
pub const SPI1_IMU_DEADLINE_US: u32 = 250;

pub type SpiRxBuf = &'static mut [u8; SPI_BUFFER_SIZE];
pub type SpiTxBuf = &'static mut [u8; SPI_BUFFER_SIZE];

pub struct FilledSpiRxBuf {
    pub buf: SpiRxBuf,
    pub len: usize,
    pub request: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiRxDeliveryError {
    NoFreshBuffer,
    TransferNotReady,
    FilledQueueFull,
    PlannerRejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiRxIrqOutcome {
    Ignored,
    Delivered,
    NoChunk,
    DmaError,
    DeliveryError(SpiRxDeliveryError),
}

pub struct SpiRxRestartFailure {
    pub buffer: SpiRxBuf,
}

// Keep the old names so existing app code does not break.
pub type FreeProducer = Producer<'static, SpiRxBuf>;
pub type FreeConsumer = Consumer<'static, SpiRxBuf>;

pub type FilledProducer = Producer<'static, FilledSpiRxBuf>;
pub type FilledConsumer = Consumer<'static, FilledSpiRxBuf>;

pub type FreeQueue = Queue<SpiRxBuf, SPI_RX_QUEUE_CAPACITY>;
pub type FilledQueue = Queue<FilledSpiRxBuf, SPI_RX_QUEUE_CAPACITY>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiTxStartError {
    Busy,
    FrameTooLong,
}

/// The transmit half of an SPI DMA owner: one frame at a time from a fixed
/// buffer. A family backend implements it over its own DMA driver.
pub trait SpiTxFrameDma {
    /// A frame can be started: the transmit stream is back in the poller's
    /// hands.
    fn is_ready(&self) -> bool;

    /// Copy `frame` into the transmit buffer, run `before_start` (the chip
    /// select), and start the stream.
    fn start_frame<F>(&mut self, frame: &[u8], before_start: F) -> Result<(), SpiTxStartError>
    where
        F: FnOnce();

    fn pause_frame(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiOwnerStartError {
    Busy,
    Unavailable,
    UnsupportedJob,
    Tx(SpiTxStartError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiOwnerServiceOutcome {
    Idle,
    Started,
    Cancelled,
    RecoveryFailed,
    StartFailed(SpiOwnerStartError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiWatchdogOutcome {
    Idle,
    Active,
    TimedOut,
    RecoveryFailed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpiOwnerStats {
    pub timeouts: u32,
    pub recovery_failures: u32,
}

pub struct SpiDmaOwner<RxTransferT, PollerT, CsT> {
    irq: SpiRxIrqSide<RxTransferT>,
    poller: PollerT,
    cs: CsT,
    active_request: u8,
    active_token: Option<SpiJobToken>,
    recovery_rx_buffer: Option<SpiRxBuf>,
    available: bool,
    stats: SpiOwnerStats,
    inject_timeout_once: bool,
}

/// What a task needs of an SPI DMA owner, whichever DMA streams and chip
/// select a board wires it to. What a shared task definition bounds on; each
/// method forwards to the owner's own.
pub trait SpiDmaService {
    fn service_request<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        now_us: u64,
    ) -> SpiOwnerServiceOutcome;

    fn service_timeout<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        now_us: u64,
    ) -> SpiWatchdogOutcome;

    fn service_dma_irq<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
    ) -> SpiRxIrqOutcome;
}

impl<RxTransferT, PollerT, CsT> SpiDmaService for SpiDmaOwner<RxTransferT, PollerT, CsT>
where
    PollerT: SpiTxFrameDma,
    RxTransferT: SpiRxTransferExt,
    CsT: OutputPin,
{
    fn service_request<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        now_us: u64,
    ) -> SpiOwnerServiceOutcome {
        SpiDmaOwner::service_request(self, mailbox, now_us)
    }

    fn service_timeout<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        now_us: u64,
    ) -> SpiWatchdogOutcome {
        SpiDmaOwner::service_timeout(self, mailbox, now_us)
    }

    fn service_dma_irq<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
    ) -> SpiRxIrqOutcome {
        SpiDmaOwner::service_dma_irq(self, mailbox)
    }
}

impl<RxTransferT, PollerT, CsT> SpiDmaOwner<RxTransferT, PollerT, CsT>
where
    RxTransferT: SpiRxTransferExt,
    CsT: OutputPin,
{
    pub fn new(
        irq: SpiRxIrqSide<RxTransferT>,
        poller: PollerT,
        recovery_rx_buffer: SpiRxBuf,
        cs: CsT,
    ) -> Self {
        Self {
            irq,
            poller,
            cs,
            active_request: 0,
            active_token: None,
            recovery_rx_buffer: Some(recovery_rx_buffer),
            available: true,
            stats: SpiOwnerStats::default(),
            inject_timeout_once: cfg!(feature = "bench_spi_timeout_recovery"),
        }
    }

    pub fn service_dma_irq<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
    ) -> SpiRxIrqOutcome {
        let Some(token) = self.active_token else {
            self.irq.discard_unowned_dma_irq();
            let _ = self.cs.set_high();
            return SpiRxIrqOutcome::Ignored;
        };

        let mut copied_to_job = false;
        let outcome = self
            .irq
            .service_dma_irq_with_frame(self.active_request, |frame| {
                if let Ok(job) = mailbox.job_mut(token) {
                    let rx = job.rx_bytes_mut();
                    if rx.len() == frame.len() {
                        rx.copy_from_slice(frame);
                        copied_to_job = true;
                    }
                }
            });

        if matches!(outcome, SpiRxIrqOutcome::Ignored) {
            return outcome;
        }

        let _ = self.cs.set_high();

        match outcome {
            SpiRxIrqOutcome::Delivered if copied_to_job => {
                let _ = mailbox.complete(token);
            }
            SpiRxIrqOutcome::DmaError | SpiRxIrqOutcome::DeliveryError(_) => {
                let _ = mailbox.dma_fault(token);
            }
            SpiRxIrqOutcome::Delivered | SpiRxIrqOutcome::NoChunk => {
                let _ = mailbox.owner_fault(token, SpiFault::Backend);
            }
            SpiRxIrqOutcome::Ignored => {}
        }

        if matches!(
            outcome,
            SpiRxIrqOutcome::DeliveryError(SpiRxDeliveryError::TransferNotReady)
        ) {
            self.available = false;
            self.stats.recovery_failures = self.stats.recovery_failures.saturating_add(1);
        }
        self.active_token = None;

        outcome
    }

    pub const fn stats(&self) -> SpiOwnerStats {
        self.stats
    }
}

impl<RxTransferT, PollerT, CsT> SpiDmaOwner<RxTransferT, PollerT, CsT>
where
    PollerT: SpiTxFrameDma,
    RxTransferT: SpiRxTransferExt,
    CsT: OutputPin,
{
    pub fn service_request<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        now_us: u64,
    ) -> SpiOwnerServiceOutcome {
        match mailbox.owner_action() {
            SpiMailboxOwnerAction::Idle => SpiOwnerServiceOutcome::Idle,
            SpiMailboxOwnerAction::Start(token) => {
                match self.start_request(mailbox, token, now_us) {
                    Ok(()) => SpiOwnerServiceOutcome::Started,
                    Err(error) => SpiOwnerServiceOutcome::StartFailed(error),
                }
            }
            SpiMailboxOwnerAction::Abort(token) => {
                if self.active_token == Some(token) {
                    if self.recover_active_transfer() {
                        let _ = mailbox.owner_abort(token);
                        self.active_token = None;
                        SpiOwnerServiceOutcome::Cancelled
                    } else {
                        let _ = mailbox.owner_abort(token);
                        self.active_token = None;
                        self.mark_recovery_failed();
                        SpiOwnerServiceOutcome::RecoveryFailed
                    }
                } else {
                    let _ = mailbox.owner_abort(token);
                    SpiOwnerServiceOutcome::Cancelled
                }
            }
        }
    }

    fn start_request<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        token: SpiJobToken,
        now_us: u64,
    ) -> Result<(), SpiOwnerStartError> {
        if !self.available {
            let _ = mailbox.owner_fault(token, SpiFault::Unavailable);
            return Err(SpiOwnerStartError::Unavailable);
        }

        if !self.poller.is_ready() || self.active_token.is_some() {
            let _ = mailbox.owner_fault(token, SpiFault::Backend);
            return Err(SpiOwnerStartError::Busy);
        }

        let supported = mailbox.job(token).is_ok_and(|job| {
            matches!(
                job.operations(),
                [OwnedSpiOperation::Transfer {
                    tx_len,
                    rx_len,
                    wire_len,
                    ..
                }] if *tx_len == SPI_BUFFER_SIZE
                    && *rx_len == SPI_BUFFER_SIZE
                    && *wire_len == SPI_BUFFER_SIZE
            ) && job.tx_bytes().len() == SPI_BUFFER_SIZE
                && job.rx_bytes().len() == SPI_BUFFER_SIZE
        });
        if !supported {
            let _ = mailbox.owner_fault(token, SpiFault::Backend);
            return Err(SpiOwnerStartError::UnsupportedJob);
        }

        if mailbox.activate_at(token, TimestampMicros(now_us)).is_err() {
            let _ = mailbox.owner_fault(token, SpiFault::Backend);
            return Err(SpiOwnerStartError::UnsupportedJob);
        }

        self.active_request = mailbox
            .job(token)
            .ok()
            .and_then(|job| job.tx_bytes().first().copied())
            .map(|request| request & 0x7f)
            .unwrap_or(0);
        self.active_token = Some(token);

        if self.inject_timeout_once {
            self.inject_timeout_once = false;
            let _ = self.cs.set_low();
            return Ok(());
        }

        let cs = &mut self.cs;
        let tx_result = mailbox
            .job(token)
            .map_or(Err(SpiTxStartError::Busy), |job| {
                self.poller.start_frame(job.tx_bytes(), || {
                    let _ = cs.set_low();
                })
            });

        if let Err(error) = tx_result {
            let _ = self.cs.set_high();
            self.active_token = None;
            let fault = match error {
                SpiTxStartError::FrameTooLong => SpiFault::Oversize,
                SpiTxStartError::Busy => SpiFault::Backend,
            };
            let _ = mailbox.owner_fault(token, fault);
            return Err(SpiOwnerStartError::Tx(error));
        }

        Ok(())
    }

    pub fn service_timeout<const MAX_OPS: usize, const MAX_BYTES: usize>(
        &mut self,
        mailbox: &mut SpiRequestMailbox<MAX_OPS, MAX_BYTES>,
        now_us: u64,
    ) -> SpiWatchdogOutcome {
        if !mailbox.check_timeout(TimestampMicros(now_us)) {
            return if self.active_token.is_some() {
                SpiWatchdogOutcome::Active
            } else {
                SpiWatchdogOutcome::Idle
            };
        }

        self.stats.timeouts = self.stats.timeouts.saturating_add(1);
        let Some(token) = self.active_token else {
            return SpiWatchdogOutcome::Idle;
        };

        let recovered = self.recover_active_transfer();
        let _ = mailbox.owner_abort(token);
        self.active_token = None;

        if recovered {
            SpiWatchdogOutcome::TimedOut
        } else {
            self.mark_recovery_failed();
            SpiWatchdogOutcome::RecoveryFailed
        }
    }

    fn recover_active_transfer(&mut self) -> bool {
        self.poller.pause_frame();
        self.irq.rx_transfer.spi_rx_pause();
        let _ = self.cs.set_high();

        self.recovery_rx_buffer.take().is_some_and(|buffer| {
            match self.irq.rx_transfer.next_spi_rx_transfer(buffer) {
                Ok(partial_buffer) => {
                    self.recovery_rx_buffer = Some(partial_buffer);
                    true
                }
                Err(failure) => {
                    self.recovery_rx_buffer = Some(failure.buffer);
                    false
                }
            }
        })
    }

    fn mark_recovery_failed(&mut self) {
        self.available = false;
        self.stats.recovery_failures = self.stats.recovery_failures.saturating_add(1);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpiRxIrqStats {
    pub completed_transfers: u32,
    pub dma_errors: u32,
    pub ignored_events: u32,
}

pub struct SpiRxIrqSide<RxTransferT> {
    pub rx_planner: SpiRxIrqPlanner,
    pub rx_stats: SpiRxIrqStats,
    pub rx_transfer: RxTransferT,
    pub free_consumer: FreeConsumer,
    pub filled_producer: FilledProducer,
    restart_failure_buffer: Option<SpiRxBuf>,
}

impl<RxTransferT> SpiRxIrqSide<RxTransferT> {
    pub fn new(
        rx_transfer: RxTransferT,
        free_consumer: FreeConsumer,
        filled_producer: FilledProducer,
    ) -> Self {
        Self {
            rx_planner: SpiRxIrqPlanner::new(),
            rx_stats: SpiRxIrqStats::default(),
            rx_transfer,
            free_consumer,
            filled_producer,
            restart_failure_buffer: None,
        }
    }

    fn discard_unowned_dma_irq(&mut self)
    where
        RxTransferT: SpiRxTransferExt,
    {
        self.rx_transfer.clear_spi_rx_flags();
        self.rx_stats.ignored_events = self.rx_stats.ignored_events.saturating_add(1);
    }

    pub fn service_dma_irq(&mut self, request: u8) -> SpiRxIrqOutcome
    where
        RxTransferT: SpiRxTransferExt,
    {
        self.service_dma_irq_with_frame(request, |_| {})
    }

    pub fn service_dma_irq_with_frame<F>(&mut self, request: u8, on_frame: F) -> SpiRxIrqOutcome
    where
        RxTransferT: SpiRxTransferExt,
        F: FnOnce(&[u8]),
    {
        if self.rx_transfer.spi_rx_has_dma_error() {
            let _ = self.plan_dma_error();
            self.rx_transfer.clear_spi_rx_flags();
            return SpiRxIrqOutcome::DmaError;
        }

        if !self.rx_transfer.spi_rx_is_complete() {
            return SpiRxIrqOutcome::Ignored;
        }

        let outcome = match self.deliver_completed_transfer_with(request, on_frame) {
            Ok(true) => SpiRxIrqOutcome::Delivered,
            Ok(false) => SpiRxIrqOutcome::NoChunk,
            Err(error) => SpiRxIrqOutcome::DeliveryError(error),
        };
        self.rx_transfer.clear_spi_rx_flags();
        outcome
    }

    pub fn plan_dma_complete(&mut self) -> SpiRxIrqAction {
        let action = self.rx_planner.handle(SpiRxIrqFlags {
            transfer_complete: true,
            dma_error: false,
        });
        self.record_rx_action(action);
        action
    }

    pub fn plan_dma_error(&mut self) -> SpiRxIrqAction {
        let action = self.rx_planner.handle(SpiRxIrqFlags {
            transfer_complete: false,
            dma_error: true,
        });
        self.record_rx_action(action);
        action
    }

    pub fn deliver_completed_transfer(&mut self, request: u8) -> Result<bool, SpiRxDeliveryError>
    where
        RxTransferT: SpiRxTransferExt,
    {
        self.deliver_completed_transfer_with(request, |_| {})
    }

    fn deliver_completed_transfer_with<F>(
        &mut self,
        request: u8,
        on_frame: F,
    ) -> Result<bool, SpiRxDeliveryError>
    where
        RxTransferT: SpiRxTransferExt,
        F: FnOnce(&[u8]),
    {
        match self.plan_dma_complete() {
            SpiRxIrqAction::Deliver { .. } => {
                self.rotate_rx_buffer(request, on_frame).map(|()| true)
            }
            SpiRxIrqAction::None => Ok(false),
            SpiRxIrqAction::Fault(_) => Err(SpiRxDeliveryError::PlannerRejected),
        }
    }

    fn rotate_rx_buffer<F>(&mut self, request: u8, on_frame: F) -> Result<(), SpiRxDeliveryError>
    where
        RxTransferT: SpiRxTransferExt,
        F: FnOnce(&[u8]),
    {
        let Some(fresh_buffer) = self.free_consumer.dequeue() else {
            return Err(SpiRxDeliveryError::NoFreshBuffer);
        };

        let raw_buffer = match self.rx_transfer.next_spi_rx_transfer(fresh_buffer) {
            Ok(raw_buffer) => raw_buffer,
            Err(failure) => {
                self.restart_failure_buffer = Some(failure.buffer);
                return Err(SpiRxDeliveryError::TransferNotReady);
            }
        };

        on_frame(&raw_buffer[..SPI_ARRAY_SIZE]);
        let filled_buffer = FilledSpiRxBuf {
            buf: raw_buffer,
            len: SPI_ARRAY_SIZE,
            request,
        };
        self.filled_producer
            .enqueue(filled_buffer)
            .map_err(|_| SpiRxDeliveryError::FilledQueueFull)
    }

    fn record_rx_action(&mut self, action: SpiRxIrqAction) {
        match action {
            SpiRxIrqAction::Deliver { .. } => {
                self.rx_stats.completed_transfers =
                    self.rx_stats.completed_transfers.saturating_add(1);
            }
            SpiRxIrqAction::Fault(_) => {
                self.rx_stats.dma_errors = self.rx_stats.dma_errors.saturating_add(1);
            }
            SpiRxIrqAction::None => {
                self.rx_stats.ignored_events = self.rx_stats.ignored_events.saturating_add(1);
            }
        }
    }
}

pub trait SpiRxTransferExt {
    fn spi_rx_has_dma_error(&self) -> bool;

    fn spi_rx_is_complete(&self) -> bool;

    fn clear_spi_rx_flags(&mut self);

    fn spi_rx_pause(&mut self);

    fn next_spi_rx_transfer(
        &mut self,
        fresh_buffer: SpiRxBuf,
    ) -> Result<SpiRxBuf, SpiRxRestartFailure>;
}

pub struct SpiRxParserSide {
    pub free_producer: FreeProducer,
    pub filled_consumer: FilledConsumer,
}

pub struct SpiDmaStorage {
    pub rx_buffer1: SpiRxBuf,
    pub rx_buffer2: SpiRxBuf,
    pub rx_buffer3: SpiRxBuf,
    pub _rx_buffer4: SpiRxBuf,
    pub recovery_rx_buffer: SpiRxBuf,

    pub tx_buffer: SpiTxBuf,

    pub free_queue: &'static mut FreeQueue,
    pub filled_queue: &'static mut FilledQueue,
}

/// The buffers an SPI DMA backend starts from, once the free queue holds its
/// spares.
pub struct SpiDmaStorageParts {
    /// The buffer the receive stream starts into.
    pub first_rx_buffer: SpiRxBuf,
    pub recovery_rx_buffer: SpiRxBuf,
    pub tx_buffer: SpiTxBuf,
    pub free_consumer: FreeConsumer,
    pub filled_producer: FilledProducer,
    pub parser: SpiRxParserSide,
}

impl SpiDmaStorage {
    /// Prime the free queue and split both queues, as every backend does
    /// before starting its receive stream.
    pub fn split(self) -> SpiDmaStorageParts {
        let SpiDmaStorage {
            rx_buffer1,
            rx_buffer2,
            rx_buffer3,
            _rx_buffer4: rx_buffer4,
            recovery_rx_buffer,
            tx_buffer,
            free_queue,
            filled_queue,
        } = self;

        free_queue.enqueue(rx_buffer1).ok();
        free_queue.enqueue(rx_buffer2).ok();
        free_queue.enqueue(rx_buffer4).ok();

        let (free_producer, free_consumer) = free_queue.split();
        let (filled_producer, filled_consumer) = filled_queue.split();

        SpiDmaStorageParts {
            first_rx_buffer: rx_buffer3,
            recovery_rx_buffer,
            tx_buffer,
            free_consumer,
            filled_producer,
            parser: SpiRxParserSide {
                free_producer,
                filled_consumer,
            },
        }
    }
}
