//! SPI1, the IMU bus: the DMA owner's request, timeout and completion
//! service, the I/O watchdog that bounds a transaction, and the data-ready
//! interrupt that starts a poll.
//!
//! The owner, timebase and watchdog are bounded rather than named, because
//! boards wire them to different DMA streams and timers.
//!
//! Requirements: HW-SPI-001
//! Safety-critical.

use super::prelude::*;

/// Requests from the SPI1 device to its DMA owner. One per firmware: the
/// device posts here and the owner's tasks serve it, so they must name the
/// same static.
pub type Spi1Mailbox = SharedSpiRequestMailbox<SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub type Spi1Executor =
    CriticalSectionSpiExecutor<'static, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub type Spi1Device = AsyncSpiDevice<Spi1Executor, SPI1_JOB_MAX_OPERATIONS, SPI1_JOB_MAX_BYTES>;
pub use crate::spi1_imu::Spi1ImuKind;

/// One decoded IMU sample, in g, degrees per second and degrees Celsius.
pub struct ParsedImuSample {
    pub acc: [f32; 3],
    pub gyro: [f32; 3],
    pub gyro_raw: [i16; 3],
    pub temp: f32,
}

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
    bounds = [io_watchdog: stm32_watchdog::TimerTick, io_timebase: stm32_timebase::Timebase],
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
    bounds = [
        imu_data_ready: crate::exti::InterruptInput,
        io_timebase: stm32_timebase::Timebase,
    ],
    local = [
        imu_data_ready,
        // Whether the IMU configured at boot: a fact of init.
        imu_transport_ready: bool,
        drdy_status_link: LinkProducer<'static, observed::DrdyStatus>,
        drdy_status: observed::DrdyStatus = observed::DrdyStatus {
            interrupts: 0,
            rejected: 0,
            last_us: 0,
        },
    ],
    shared = [io_timebase],
    spawn = [spi1_poll(observed_at_us: u64)],
    monotonic = Mono,
)]
pub fn imu_data_ready(mut cx: imu_data_ready::Context) {
    if !cx.local.imu_data_ready.check_interrupt() {
        return;
    }
    cx.local.imu_data_ready.clear_interrupt_pending_bit();

    let observed_at = cx.shared.io_timebase.lock(|timebase| timebase.now());
    let status = cx.local.drdy_status;
    status.interrupts = status.interrupts.wrapping_add(1);
    status.last_us = observed_at.0 as u32;

    if *cx.local.imu_transport_ready && cx.spawn.spi1_poll(observed_at.0).is_err() {
        status.rejected = status.rejected.wrapping_add(1);
    }
    cx.local.drdy_status_link.push(
        TimestampMicros(Mono::now().duration_since_epoch().to_micros()),
        *status,
    );
}

/// Read one IMU sample burst over SPI1 DMA, from the register the detected
/// IMU starts its burst at. Which IMUs a board may carry is the board's, so
/// that register comes from configuration: `None` for no IMU detected.
#[ferroforge::task(
    local = [
        spi1_device: Spi1Device,
        unavailable_logged: bool = false,
        // The IMU detected at boot, as the board numbers its kinds.
        poll_imu_kind: u8,
    ],
    config = [spi1_imu_burst_register: fn(u8) -> Option<u8>],
)]
pub async fn spi1_poll(cx: spi1_poll::Context, observed_at_us: u64) {
    let Some(request) = CONFIG::SPI1_IMU_BURST_REGISTER(*cx.local.poll_imu_kind) else {
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
        Err(ferrowasp_imu::spi::SpiDeviceError::Busy) => {
            info!("SPI1 TX DMA busy");
        }
        Err(ferrowasp_imu::spi::SpiDeviceError::TooManyOperations)
        | Err(ferrowasp_imu::spi::SpiDeviceError::TxCapacityExceeded)
        | Err(ferrowasp_imu::spi::SpiDeviceError::RxCapacityExceeded)
        | Err(ferrowasp_imu::spi::SpiDeviceError::CopybackShapeMismatch) => {
            warn!("SPI1 transaction packing failed");
        }
        Err(ferrowasp_imu::spi::SpiDeviceError::Unavailable) => {
            if !*cx.local.unavailable_logged {
                warn!("SPI1 owner unavailable after recovery failure");
                *cx.local.unavailable_logged = true;
            }
        }
        Err(ferrowasp_imu::spi::SpiDeviceError::Timeout) => {}
        Err(ferrowasp_imu::spi::SpiDeviceError::Cancelled) => {
            info!("SPI1 transaction cancelled");
        }
        Err(ferrowasp_imu::spi::SpiDeviceError::DmaTransfer) => {
            warn!("SPI1 DMA transaction failed");
        }
        Err(ferrowasp_imu::spi::SpiDeviceError::InvalidState)
        | Err(ferrowasp_imu::spi::SpiDeviceError::StaleTransaction)
        | Err(ferrowasp_imu::spi::SpiDeviceError::Backend) => {
            warn!("SPI1 transaction backend error");
        }
    }
}

/// Decode each SPI1 frame the DMA delivered as a sample from the detected
/// IMU, and publish it. A frame that is not the detected IMU's burst is
/// dropped with a warning.
#[ferroforge::task(
    local = [
        spi1_parser: stm32_spi::SpiRxParserSide,
        parser_imu_kind: u8,
        imu_sample_link: LinkProducer<'static, ferrowasp_io_contracts::imu::ImuSample>,
        imu_status_link: LinkProducer<'static, observed::ImuStatus>,
        imu_sample_seq: u32 = 0,
    ],
    monotonic = Mono,
)]
pub async fn spi1_parser(cx: spi1_parser::Context) {
    let active_kind = Spi1ImuKind::from_discriminant(*cx.local.parser_imu_kind);

    while let Some(filled) = cx.local.spi1_parser.filled_consumer.dequeue() {
        let len = filled.len.min(filled.buf.len());
        let frame = &filled.buf[..len];

        let parsed = active_kind.and_then(|kind| {
            if filled.request != kind.dma_burst_register() {
                return None;
            }

            match kind {
                Spi1ImuKind::Mpu6500 => {
                    imu::decode_accel_temp_gyro_burst(frame).ok().map(|sample| {
                        let acc_scale = 4_096.0;
                        let gyro_scale = 16.4;
                        ParsedImuSample {
                            acc: [
                                sample.acc_raw[0] as f32 / acc_scale,
                                sample.acc_raw[1] as f32 / acc_scale,
                                sample.acc_raw[2] as f32 / acc_scale,
                            ],
                            gyro: [
                                sample.gyro_raw[0] as f32 / gyro_scale,
                                sample.gyro_raw[1] as f32 / gyro_scale,
                                sample.gyro_raw[2] as f32 / gyro_scale,
                            ],
                            gyro_raw: sample.gyro_raw,
                            temp: sample.temp_raw as f32 / 333.87 + 21.0,
                        }
                    })
                }
                Spi1ImuKind::Icm42688P => {
                    icm::decode_temp_accel_gyro_burst(frame)
                        .ok()
                        .map(|sample| ParsedImuSample {
                            acc: sample.accel_g(icm::AccelFullScale::G16),
                            gyro: sample.gyro_dps(icm::GyroFullScale::Dps2000),
                            gyro_raw: sample.gyro_raw,
                            temp: sample.temperature_c(),
                        })
                }
                Spi1ImuKind::Mpu6000 => {
                    mpu6000::decode_accel_temp_gyro_burst(frame)
                        .ok()
                        .map(|sample| {
                            let config = mpu6000::Config::default();
                            ParsedImuSample {
                                acc: sample.accel_g(config.accel_full_scale),
                                gyro: sample.gyro_dps(config.gyro_full_scale),
                                gyro_raw: sample.gyro_raw,
                                temp: sample.temperature_c(),
                            }
                        })
                }
            }
        });

        match parsed {
            Some(sample) => {
                let now = TimestampMicros(Mono::now().duration_since_epoch().to_micros());
                // Gyro and accelerometer from one burst, in one message.
                cx.local.imu_sample_link.push(
                    now,
                    ferrowasp_io_contracts::imu::ImuSample {
                        gyro_raw: sample.gyro_raw,
                        accel_g: sample.acc,
                        gyro_dps: sample.gyro,
                        temperature_c: sample.temp,
                    },
                );
                *cx.local.imu_sample_seq = cx.local.imu_sample_seq.wrapping_add(1);
                cx.local.imu_status_link.push(
                    now,
                    observed::ImuStatus {
                        sample_seq: *cx.local.imu_sample_seq,
                        gyro_raw: sample.gyro_raw,
                        accel_g: sample.acc,
                        gyro_dps: sample.gyro,
                        temperature_c: sample.temp,
                    },
                );
            }
            None => match active_kind {
                Some(Spi1ImuKind::Mpu6500) => {
                    warn!("Invalid MPU6500 accel/temp/gyro frame");
                }
                Some(Spi1ImuKind::Icm42688P) => {
                    warn!("Invalid ICM42688-P temp/accel/gyro frame");
                }
                Some(Spi1ImuKind::Mpu6000) => {
                    warn!("Invalid MPU-6000 accel/temp/gyro frame");
                }
                None => warn!("IMU frame received without an active sensor"),
            },
        }

        cx.local.spi1_parser.free_producer.enqueue(filled.buf).ok();
    }
}
