//! Task-facing adapters for the STM32F405 SPI1 IMU DMA endpoint.

/// Bytes in one complete IMU DMA frame.
pub const SPI_IMU_FRAME_SIZE: usize = 15;

/// Discriminant used when no supported IMU completed initialization.
pub const IMU_KIND_NONE: u8 = 0;
/// Discriminant used for an MPU6500.
pub const IMU_KIND_MPU6500: u8 = 1;
/// Discriminant used for an ICM42688-P.
pub const IMU_KIND_ICM42688P: u8 = 2;

/// Result of servicing an asynchronous mailbox request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiImuOwnerOutcome {
    /// No request was pending.
    Idle,
    /// A DMA transaction started.
    Started,
    /// A cancellation completed.
    Cancelled,
    /// Starting the DMA transaction failed.
    StartFailed,
    /// Cancellation recovery failed and disabled the owner.
    RecoveryFailed,
}

/// Result of servicing the receive DMA interrupt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiImuRxOutcome {
    /// The interrupt did not belong to this transfer.
    Ignored,
    /// One completed frame was delivered to the parser.
    Delivered,
    /// The completed transfer contained no frame.
    NoChunk,
    /// The DMA peripheral reported an error.
    DmaError,
    /// No fresh receive buffer was available.
    NoFreshBuffer,
    /// Restarting the receive transfer failed.
    TransferNotReady,
    /// The bounded filled-frame queue was full.
    FilledQueueFull,
    /// The completion planner rejected the transfer.
    PlannerRejected,
}

/// Result of evaluating one transaction deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiImuTimeoutOutcome {
    /// No transaction is active.
    Idle,
    /// The active transaction remains within its deadline.
    Active,
    /// The transaction timed out and ownership was recovered.
    TimedOut,
    /// Timeout recovery failed and disabled the owner.
    RecoveryFailed,
}

/// Completed DMA frame temporarily owned by the parser task.
pub struct SpiImuFrame {
    /// Static DMA buffer.
    pub buffer: &'static mut [u8; SPI_IMU_FRAME_SIZE],
    /// Number of valid bytes.
    pub len: usize,
    /// Register address used to request this frame.
    pub request: u8,
}

/// Returned when the bounded free-buffer queue cannot accept a buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiImuBufferError;

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
mod stm32f405 {
    use embedded_hal_async::spi::SpiDevice as _;
    use ferrowasp_io_core::{
        spi::{
            AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiDeadlineUs,
            SpiRequestMailbox,
        },
        time::TimestampMicros,
    };
    use stm32f4xx_hal::{
        dma::{Stream0, Stream3},
        pac::{DMA2, SPI1, TIM2},
    };

    use super::{
        SPI_IMU_FRAME_SIZE, SpiImuBufferError, SpiImuFrame, SpiImuOwnerOutcome, SpiImuRxOutcome,
        SpiImuTimeoutOutcome,
    };
    use crate::{memory, spi_dma, timebase::MicrosecondTimebase};

    /// Concrete SPI1 receive-DMA transfer used by the Foxeer IMU route.
    pub type Spi1ImuRxTransfer = spi_dma::SpiRxTransfer<Stream0<DMA2>, SPI1, 3>;
    /// Concrete SPI1 transmit-DMA transfer used by the Foxeer IMU route.
    pub type Spi1ImuTxTransfer = spi_dma::SpiTxTransfer<Stream3<DMA2>, SPI1, 3>;
    /// Concrete SPI1 DMA owner.
    pub type Spi1ImuOwner =
        spi_dma::SpiDmaOwner<Spi1ImuRxTransfer, Spi1ImuTxTransfer, spi_dma::Spi1ImuCs>;
    /// Bounded request mailbox shared by the async device and hardware owner.
    pub type Spi1ImuMailbox = SharedSpiRequestMailbox<
        { memory::SPI1_JOB_MAX_OPERATIONS },
        { memory::SPI1_JOB_MAX_BYTES },
    >;
    type Spi1ImuExecutor = CriticalSectionSpiExecutor<
        'static,
        { memory::SPI1_JOB_MAX_OPERATIONS },
        { memory::SPI1_JOB_MAX_BYTES },
    >;
    type Spi1ImuAsyncDevice = AsyncSpiDevice<
        Spi1ImuExecutor,
        { memory::SPI1_JOB_MAX_OPERATIONS },
        { memory::SPI1_JOB_MAX_BYTES },
    >;

    /// TIM2-backed microsecond clock used by the SPI endpoint tasks.
    pub type Spi1ImuTimebase = MicrosecondTimebase<TIM2>;

    /// Creates unclaimed static storage for one SPI1 IMU request mailbox.
    pub const fn new_spi1_imu_mailbox() -> Spi1ImuMailbox {
        critical_section::Mutex::new(core::cell::RefCell::new(SpiRequestMailbox::new()))
    }

    /// Exclusive DMA owner and its request mailbox.
    pub struct Spi1ImuEndpointOwner {
        owner: Spi1ImuOwner,
        mailbox: &'static Spi1ImuMailbox,
    }

    impl Spi1ImuEndpointOwner {
        /// Creates an endpoint owner from initialized DMA resources.
        pub const fn new(owner: Spi1ImuOwner, mailbox: &'static Spi1ImuMailbox) -> Self {
            Self { owner, mailbox }
        }

        /// Checks and services one pending mailbox request.
        pub fn service_request(&mut self, now_us: u64) -> SpiImuOwnerOutcome {
            critical_section::with(|cs| {
                match self
                    .owner
                    .service_request(&mut self.mailbox.borrow_ref_mut(cs), now_us)
                {
                    spi_dma::SpiOwnerServiceOutcome::Idle => SpiImuOwnerOutcome::Idle,
                    spi_dma::SpiOwnerServiceOutcome::Started => SpiImuOwnerOutcome::Started,
                    spi_dma::SpiOwnerServiceOutcome::Cancelled => SpiImuOwnerOutcome::Cancelled,
                    spi_dma::SpiOwnerServiceOutcome::StartFailed(_) => {
                        SpiImuOwnerOutcome::StartFailed
                    }
                    spi_dma::SpiOwnerServiceOutcome::RecoveryFailed => {
                        SpiImuOwnerOutcome::RecoveryFailed
                    }
                }
            })
        }

        /// Services one SPI1 receive-DMA interrupt.
        pub fn service_dma_irq(&mut self) -> SpiImuRxOutcome {
            critical_section::with(|cs| {
                match self
                    .owner
                    .service_dma_irq(&mut self.mailbox.borrow_ref_mut(cs))
                {
                    spi_dma::SpiRxIrqOutcome::Ignored => SpiImuRxOutcome::Ignored,
                    spi_dma::SpiRxIrqOutcome::Delivered => SpiImuRxOutcome::Delivered,
                    spi_dma::SpiRxIrqOutcome::NoChunk => SpiImuRxOutcome::NoChunk,
                    spi_dma::SpiRxIrqOutcome::DmaError => SpiImuRxOutcome::DmaError,
                    spi_dma::SpiRxIrqOutcome::DeliveryError(
                        spi_dma::SpiRxDeliveryError::NoFreshBuffer,
                    ) => SpiImuRxOutcome::NoFreshBuffer,
                    spi_dma::SpiRxIrqOutcome::DeliveryError(
                        spi_dma::SpiRxDeliveryError::TransferNotReady,
                    ) => SpiImuRxOutcome::TransferNotReady,
                    spi_dma::SpiRxIrqOutcome::DeliveryError(
                        spi_dma::SpiRxDeliveryError::FilledQueueFull,
                    ) => SpiImuRxOutcome::FilledQueueFull,
                    spi_dma::SpiRxIrqOutcome::DeliveryError(
                        spi_dma::SpiRxDeliveryError::PlannerRejected,
                    ) => SpiImuRxOutcome::PlannerRejected,
                }
            })
        }

        /// Applies bounded timeout recovery to the active request.
        pub fn service_timeout(&mut self, now_us: u64) -> SpiImuTimeoutOutcome {
            critical_section::with(|cs| {
                match self
                    .owner
                    .service_timeout(&mut self.mailbox.borrow_ref_mut(cs), now_us)
                {
                    spi_dma::SpiWatchdogOutcome::Idle => SpiImuTimeoutOutcome::Idle,
                    spi_dma::SpiWatchdogOutcome::Active => SpiImuTimeoutOutcome::Active,
                    spi_dma::SpiWatchdogOutcome::TimedOut => SpiImuTimeoutOutcome::TimedOut,
                    spi_dma::SpiWatchdogOutcome::RecoveryFailed => {
                        SpiImuTimeoutOutcome::RecoveryFailed
                    }
                }
            })
        }
    }

    /// Async SPI device owned by the IMU polling task.
    pub struct Spi1ImuDevice {
        inner: Spi1ImuAsyncDevice,
    }

    impl Spi1ImuDevice {
        /// Creates an async endpoint using the shared mailbox and owner pender.
        pub const fn new(mailbox: &'static Spi1ImuMailbox, pend_owner: fn()) -> Self {
            Self {
                inner: AsyncSpiDevice::new(CriticalSectionSpiExecutor::new(
                    mailbox,
                    SpiDeadlineUs(spi_dma::SPI1_IMU_DEADLINE_US),
                    pend_owner,
                )),
            }
        }

        /// Submits one fixed-size register burst through the endpoint mailbox.
        pub async fn read_burst(
            &mut self,
            request: u8,
            observed_at_us: u64,
        ) -> Result<(), ferrowasp_io_core::spi::SpiDeviceError> {
            self.inner
                .executor_mut()
                .set_start(TimestampMicros(observed_at_us));
            let mut read = [0; SPI_IMU_FRAME_SIZE];
            let mut write = [0; SPI_IMU_FRAME_SIZE];
            write[0] = 0x80 | request;
            let mut operations = [embedded_hal::spi::Operation::Transfer(&mut read, &write)];
            self.inner.transaction(&mut operations).await
        }
    }

    /// Completed-frame queue and free-buffer producer owned by the parser task.
    pub struct Spi1ImuParser {
        inner: spi_dma::SpiRxParserSide,
    }

    impl Spi1ImuParser {
        /// Wraps the initialized parser side.
        pub const fn new(inner: spi_dma::SpiRxParserSide) -> Self {
            Self { inner }
        }

        /// Takes the next completed DMA frame, if any.
        pub fn next_frame(&mut self) -> Option<SpiImuFrame> {
            self.inner
                .filled_consumer
                .dequeue()
                .map(|filled| SpiImuFrame {
                    buffer: filled.buf,
                    len: filled.len,
                    request: filled.request,
                })
        }

        /// Returns one processed frame buffer to the bounded free pool.
        pub fn return_buffer(
            &mut self,
            buffer: &'static mut [u8; SPI_IMU_FRAME_SIZE],
        ) -> Result<(), SpiImuBufferError> {
            self.inner
                .free_producer
                .enqueue(buffer)
                .map_err(|_| SpiImuBufferError)
        }
    }
}

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use stm32f405::{
    Spi1ImuDevice, Spi1ImuEndpointOwner, Spi1ImuMailbox, Spi1ImuOwner, Spi1ImuParser,
    Spi1ImuRxTransfer, Spi1ImuTimebase, Spi1ImuTxTransfer, new_spi1_imu_mailbox,
};
