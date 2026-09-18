//! SPI1, the IMU bus: the DMA owner's request, timeout and completion
//! service, the I/O watchdog that bounds a transaction, and the data-ready
//! interrupt that starts a poll.
//!
//! The owner, timebase and watchdog are bounded rather than named, because
//! boards wire them to different DMA streams and timers.

use crate::prelude::*;
use crate::snapshots::*;

/// Requests from the SPI1 device to its DMA owner. One per firmware: the
/// device posts here and the owner's tasks serve it, so they must name the
/// same static.
pub type Spi1Mailbox = SharedSpiRequestMailbox<SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub type Spi1Executor =
    CriticalSectionSpiExecutor<'static, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub type Spi1Device = AsyncSpiDevice<Spi1Executor, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub static SPI1_MAILBOX: Spi1Mailbox =
    critical_section::Mutex::new(core::cell::RefCell::new(SpiRequestMailbox::new()));

/// Start the next queued SPI1 request, or cancel one the device withdrew.
#[ferroforge::task(
    bounds = [spi1_owner: stm32_spi::SpiDmaService, io_timebase: stm32_timebase::Timebase],
    shared = [spi1_owner, io_timebase],
)]
pub async fn spi1_owner_service(mut cx: spi1_owner_service::Context) {
    let now = cx.shared.io_timebase.lock(|timebase| timebase.now());
    let outcome = cx.shared.spi1_owner.lock(|owner| {
        critical_section::with(|cs| {
            owner.service_request(&mut SPI1_MAILBOX.borrow_ref_mut(cs), now.0)
        })
    });

    match outcome {
        stm32_spi::SpiOwnerServiceOutcome::Idle
        | stm32_spi::SpiOwnerServiceOutcome::Started
        | stm32_spi::SpiOwnerServiceOutcome::Cancelled => {}
        stm32_spi::SpiOwnerServiceOutcome::RecoveryFailed => {
            warn!("SPI1 cancellation recovery failed; owner disabled");
        }
        stm32_spi::SpiOwnerServiceOutcome::StartFailed(_) => {}
    }
}

/// Recover DMA ownership from a transaction the watchdog found overdue.
#[ferroforge::task(
    bounds = [spi1_owner: stm32_spi::SpiDmaService],
    shared = [spi1_owner],
)]
pub async fn spi1_timeout(mut cx: spi1_timeout::Context, observed_at_us: u64) {
    let outcome = cx.shared.spi1_owner.lock(|owner| {
        critical_section::with(|cs| {
            owner.service_timeout(&mut SPI1_MAILBOX.borrow_ref_mut(cs), observed_at_us)
        })
    });

    match outcome {
        stm32_spi::SpiWatchdogOutcome::Idle | stm32_spi::SpiWatchdogOutcome::Active => {}
        stm32_spi::SpiWatchdogOutcome::TimedOut => {
            warn!("SPI1 transaction timed out; DMA ownership recovered");
        }
        stm32_spi::SpiWatchdogOutcome::RecoveryFailed => {
            warn!("SPI1 timeout recovery failed; owner disabled");
        }
    }
}

/// The I/O watchdog tick: if the SPI1 transaction in flight is past its
/// deadline, hand it to `spi1_timeout`.
#[ferroforge::task(
    bounds = [io_watchdog: stm32_watchdog::WatchdogTick, io_timebase: stm32_timebase::Timebase],
    local = [io_watchdog],
    shared = [io_timebase],
    spawn = [spi1_timeout(observed_at_us: u64)],
)]
pub fn io_watchdog(mut cx: io_watchdog::Context) {
    stm32_watchdog::acknowledge_watchdog_tick(cx.local.io_watchdog);

    let now = cx.shared.io_timebase.lock(|timebase| timebase.now());
    let expired = critical_section::with(|cs| {
        SPI1_MAILBOX
            .borrow_ref(cs)
            .lifecycle()
            .deadline_expired(now)
    });

    if expired {
        let _ = cx.spawn.spi1_timeout(now.0);
    }
}

/// SPI1 RX DMA complete: deliver the frame and wake the parser.
#[ferroforge::task(
    bounds = [spi1_owner: stm32_spi::SpiDmaService],
    shared = [spi1_owner],
    spawn = [spi1_parser()],
)]
pub fn spi1_rx_dma(mut cx: spi1_rx_dma::Context) {
    let outcome = cx.shared.spi1_owner.lock(|owner| {
        critical_section::with(|cs| owner.service_dma_irq(&mut SPI1_MAILBOX.borrow_ref_mut(cs)))
    });
    let delivered = match outcome {
        stm32_spi::SpiRxIrqOutcome::Ignored => return,
        stm32_spi::SpiRxIrqOutcome::Delivered => true,
        stm32_spi::SpiRxIrqOutcome::NoChunk => false,
        stm32_spi::SpiRxIrqOutcome::DmaError => {
            warn!("SPI1 RX DMA error");
            return;
        }
        stm32_spi::SpiRxIrqOutcome::DeliveryError(stm32_spi::SpiRxDeliveryError::NoFreshBuffer) => {
            panic!("SPI1 RX free-buffer pool exhausted");
        }
        stm32_spi::SpiRxIrqOutcome::DeliveryError(
            stm32_spi::SpiRxDeliveryError::TransferNotReady,
        ) => {
            info!("SPI1 DMA next_transfer failed");
            return;
        }
        stm32_spi::SpiRxIrqOutcome::DeliveryError(
            stm32_spi::SpiRxDeliveryError::FilledQueueFull,
        ) => {
            panic!("SPI1 filled queue full; RX buffer ownership would be lost");
        }
        stm32_spi::SpiRxIrqOutcome::DeliveryError(
            stm32_spi::SpiRxDeliveryError::PlannerRejected,
        ) => {
            return;
        }
    };

    if delivered {
        let _ = cx.spawn.spi1_parser();
    }
}

/// The IMU's data-ready interrupt: timestamp it and start a poll, counting
/// ones that arrive before the last poll was taken.
#[ferroforge::task(
    bounds = [imu_data_ready: ExtiPin, io_timebase: stm32_timebase::Timebase],
    local = [imu_data_ready],
    shared = [io_timebase],
    spawn = [spi1_poll(observed_at_us: u64)],
)]
pub fn imu_data_ready(mut cx: imu_data_ready::Context) {
    if !cx.local.imu_data_ready.check_interrupt() {
        return;
    }
    cx.local.imu_data_ready.clear_interrupt_pending_bit();

    let observed_at = cx.shared.io_timebase.lock(|timebase| timebase.now());
    IMU_DRDY_IRQ_COUNT.fetch_add(1, Ordering::Relaxed);
    IMU_DRDY_LAST_US.store(observed_at.0 as u32, Ordering::Relaxed);

    if IMU_TRANSPORT_READY.load(Ordering::Relaxed) && cx.spawn.spi1_poll(observed_at.0).is_err() {
        IMU_DRDY_REJECTED_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

/// Read one IMU sample burst over SPI1 DMA, from the register the detected
/// IMU starts its burst at. Which IMUs a board may carry is the board's, so
/// that register comes from configuration: `None` for no IMU detected.
#[ferroforge::task(
    local = [spi1_device: Spi1Device, unavailable_logged: bool = false],
    config = [spi1_imu_burst_register: fn(u8) -> Option<u8>],
)]
pub async fn spi1_poll(cx: spi1_poll::Context, observed_at_us: u64) {
    let Some(request) = CONFIG::SPI1_IMU_BURST_REGISTER(ACTIVE_IMU_KIND.load(Ordering::Relaxed))
    else {
        return;
    };
    cx.local
        .spi1_device
        .executor_mut()
        .set_start(TimestampMicros(observed_at_us));

    let mut read = [0; SPI_BUFFER_SIZE];
    let mut write = [0; SPI_BUFFER_SIZE];
    write[0] = 0x80 | request;
    let mut operations = [Operation::Transfer(&mut read, &write)];
    let result = cx.local.spi1_device.transaction(&mut operations).await;

    match result {
        Ok(()) => {}
        Err(ferrowasp_io_core::spi::SpiDeviceError::Busy) => {
            info!("SPI1 TX DMA busy");
        }
        Err(ferrowasp_io_core::spi::SpiDeviceError::TooManyOperations)
        | Err(ferrowasp_io_core::spi::SpiDeviceError::TxCapacityExceeded)
        | Err(ferrowasp_io_core::spi::SpiDeviceError::RxCapacityExceeded)
        | Err(ferrowasp_io_core::spi::SpiDeviceError::CopybackShapeMismatch) => {
            warn!("SPI1 transaction packing failed");
        }
        Err(ferrowasp_io_core::spi::SpiDeviceError::Unavailable) => {
            if !*cx.local.unavailable_logged {
                warn!("SPI1 owner unavailable after recovery failure");
                *cx.local.unavailable_logged = true;
            }
        }
        Err(ferrowasp_io_core::spi::SpiDeviceError::Timeout) => {}
        Err(ferrowasp_io_core::spi::SpiDeviceError::Cancelled) => {
            info!("SPI1 transaction cancelled");
        }
        Err(ferrowasp_io_core::spi::SpiDeviceError::DmaTransfer) => {
            warn!("SPI1 DMA transaction failed");
        }
        Err(ferrowasp_io_core::spi::SpiDeviceError::InvalidState)
        | Err(ferrowasp_io_core::spi::SpiDeviceError::StaleTransaction)
        | Err(ferrowasp_io_core::spi::SpiDeviceError::Backend) => {
            warn!("SPI1 transaction backend error");
        }
    }
}
