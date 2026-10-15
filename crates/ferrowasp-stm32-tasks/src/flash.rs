//! The storage owner: the flight log, stored configuration, log sync and
//! the storage commands from USB, the configurator link and MSPv2. One task
//! for every board, over any store with NOR semantics; a board supplies its
//! store and its defaults.

use crate::prelude::*;
use crate::snapshots::*;
use core::fmt::Write as _;
use ferrowasp_drivers::nor_storage::NorStorage;
use ferrowasp_flight::accel_level::{AccelLevelCalibration, CalibrationError};
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_flight::blackbox_storage as blackbox_task;
use ferrowasp_flight::command_link::{CommandLink, CommandSources, ResponseRouter, reboot_refusal};
use ferrowasp_flight::config_save::{ConfigSave, SaveFailure, SaveStep};
use ferrowasp_flight::log_sync;
use ferrowasp_flight::log_writer::{LogWriter, PageWrite};
use ferrowasp_flight::storage_maintenance::{
    Maintenance, MaintenanceDone, MaintenanceFailure, MaintenanceStep,
};
use ferrowasp_io_core::serial::{SerialBindings, SerialRoute};
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2 as mspv2;
use ferrowasp_stm32::system_reset::RebootTarget;
use ferrowasp_stm32::uart_port as stm32_port;

/// How long a reboot waits for the store to finish its last program, in
/// milliseconds. With maintenance refused, that is one page program, a few
/// milliseconds on SPI NOR; a 4 KiB sector erase takes at most 400.
pub const REBOOT_STORAGE_SETTLE_MS: u32 = 500;

/// How long a reboot waits after answering, so the answer leaves over USB
/// before the port goes away.
pub const REBOOT_ANSWER_MS: u64 = 100;

#[derive(Clone, Copy)]
#[cfg_attr(not(feature = "mspv2_configurator"), allow(dead_code))]
pub enum ConfigRpcCompletion {
    Commit(u16),
    Defaults(u16),
}

#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcCommandConsumer = flash_task::RpcCommandConsumer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcCommandConsumer = ();
#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcResponseProducer = flash_task::RpcResponseProducer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcResponseProducer = ();
/// The board's MSPv2 hello: its identity, built by the firmware.
#[cfg(feature = "mspv2_configurator")]
pub type RpcDeviceInfoFn = fn() -> mspv2::rpc::DeviceInfo;
#[cfg(not(feature = "mspv2_configurator"))]
pub type RpcDeviceInfoFn = ();

pub fn scan_flash_log<F: NorStorage>(
    flash: &mut F,
    layout: flash_task::StorageLayout,
) -> Result<(u32, u32, bool), flash_task::StorageReadError<F::Error>> {
    flash_task::scan_log(layout, |address, page| flash.read(address, page))
}

/// The stored configuration, its sequence and slot; `defaults` is the
/// board's configuration for a store that holds none.
pub fn load_flash_config<F: NorStorage>(
    flash: &mut F,
    layout: flash_task::StorageLayout,
    defaults: flash_task::StoredConfig,
) -> Result<(flash_task::StoredConfig, u32, u8), flash_task::StorageReadError<F::Error>> {
    flash_task::load_config(layout, defaults, |address, page| flash.read(address, page))
}

/// Queue one answer line for the link the storage owner is answering.
pub fn queue_storage_response(responses: &mut ResponseRouter, text: &str) -> bool {
    let Some(frame) = flash_task::ResponseFrame::from_text(text) else {
        return false;
    };
    if !responses.send(frame) {
        return false;
    }
    // The configurator link polls its queue; USB is woken.
    if responses.replying_to() == CommandLink::Usb {
        cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
    }
    true
}

#[cfg(feature = "mspv2_configurator")]
pub fn queue_rpc_response(
    producer: &mut flash_task::RpcResponseProducer,
    response: mspv2::rpc::RpcResponse,
) -> bool {
    if producer.enqueue(response).is_err() {
        return false;
    }
    cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
    true
}

#[cfg(feature = "mspv2_configurator")]
pub fn finish_config_rpc_error(
    producer: &mut flash_task::RpcResponseProducer,
    completion: &mut Option<ConfigRpcCompletion>,
    error: mspv2::rpc::DeviceError,
) -> bool {
    let Some(completion) = completion.take() else {
        return false;
    };
    let request_id = match completion {
        ConfigRpcCompletion::Commit(id) | ConfigRpcCompletion::Defaults(id) => id,
    };
    queue_rpc_response(producer, mspv2::rpc::error(request_id, error))
}

#[cfg(feature = "mspv2_configurator")]
pub fn blackbox_info<F: NorStorage>(
    flash: &mut F,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
    active_id: Option<u32>,
) -> Result<Option<mspv2::rpc::BlackboxInfo>, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::blackbox_info(&mut read, layout, used_pages, id, active_id)
}

#[cfg(feature = "mspv2_configurator")]
pub fn list_blackboxes<F: NorStorage>(
    flash: &mut F,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    active_id: Option<u32>,
) -> Result<mspv2::rpc::BlackboxList, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::list_blackboxes(&mut read, layout, used_pages, active_id)
}

#[cfg(feature = "mspv2_configurator")]
pub fn read_blackbox_chunk<F: NorStorage>(
    flash: &mut F,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
    offset: u32,
    requested_length: u16,
) -> Result<mspv2::rpc::BlackboxChunk, mspv2::rpc::DeviceError> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::read_blackbox_chunk(&mut read, layout, used_pages, id, offset, requested_length)
}

pub fn queue_page_hex(
    producer: &mut ResponseRouter,
    page_index: u32,
    page: &[u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN],
) -> bool {
    flash_task::emit_page_hex_lines(page_index, page, |line| {
        queue_storage_response(producer, line)
    })
}

/// Starts saving `config` (`ConfigSave`), answering the link that asked
/// when it completes. `Err` is the answer line when it cannot start.
fn begin_config_save(
    config_save: &mut ConfigSave,
    config: flash_task::StoredConfig,
    config_sequence: u32,
    config_active_slot: u8,
    flash_responses: &mut ResponseRouter,
) -> Result<(), &'static str> {
    config_save
        .begin(config, config_sequence, config_active_slot)
        .map_err(|_| "ERR config encoding failed\r\n")?;
    flash_responses.start_maintenance();
    Ok(())
}

/// The answer line for a calibration that produced nothing.
fn calibration_failure(error: CalibrationError) -> &'static str {
    match error {
        CalibrationError::Moving => "ERR imu calibration failed; the craft moved\r\n",
        CalibrationError::NotLevel => {
            "ERR imu calibration failed; not level, more than 10 degrees\r\n"
        }
        CalibrationError::BadGravity => {
            "ERR imu calibration failed; the accelerometer does not read 1 g\r\n"
        }
    }
}

/// How long a calibration may wait for its IMU samples, in milliseconds.
pub const IMU_CALIBRATION_DEADLINE_MS: u64 = 3_000;

/// Own the store: recover the log and configuration, write the flight log,
/// and answer every storage command, one bounded pass a millisecond. Lowest
/// priority: it never preempts flight work.
#[ferroforge::task(
    bounds = [flash_device: NorStorage],
    local = [
        flash_device,
        flash_record_consumer: flash_task::RecordConsumer,
        flash_commands: CommandSources,
        flash_responses: ResponseRouter,
        flash_rpc_command_consumer: FlashRpcCommandConsumer,
        flash_rpc_response_producer: FlashRpcResponseProducer,
        flash_rpc_device_info: RpcDeviceInfoFn,
        log: LogWriter = LogWriter::new(),
        initialized: bool = false,
        stored_config: flash_task::StoredConfig,
        config_sequence: u32 = 0,
        config_active_slot: u8 = 1,
        maintenance: Maintenance = Maintenance::Idle,
        config_save: ConfigSave = ConfigSave::idle(),
        staged_rpc_config: Option<flash_task::StoredConfig> = None,
        config_rpc_completion: Option<ConfigRpcCompletion> = None,
        accel_calibration: Option<AccelLevelCalibration> = None,
        accel_calibration_seq: u32 = 0,
        accel_calibration_deadline_ms: u64 = 0,
    ],
    shared = [
        tuning_profile: dt::TuningProfile,
        tuning_request_seq: u32,
        imu_data: imu::ImuData,
    ],
    config = [
        default_stored_config: flash_task::StoredConfig,
        default_serial_bindings: SerialBindings,
        serial_routes: &'static [SerialRoute],
        imu_control_axis_profile: ferrowasp_core::frames::ImuControlAxisProfile,
    ],
    monotonic = Mono,
)]
pub async fn flash_manager_task(mut cx: flash_manager_task::Context) {
    let flash_device = cx.local.flash_device;
    let flash_record_consumer = cx.local.flash_record_consumer;
    let flash_commands = cx.local.flash_commands;
    let flash_responses = cx.local.flash_responses;
    let flash_rpc_command_consumer = cx.local.flash_rpc_command_consumer;
    let flash_rpc_response_producer = cx.local.flash_rpc_response_producer;
    let flash_rpc_device_info = cx.local.flash_rpc_device_info;
    let log = cx.local.log;
    let initialized = cx.local.initialized;
    let stored_config = cx.local.stored_config;
    let config_sequence = cx.local.config_sequence;
    let config_active_slot = cx.local.config_active_slot;
    let maintenance = cx.local.maintenance;
    let config_save = cx.local.config_save;
    let staged_rpc_config = cx.local.staged_rpc_config;
    let config_rpc_completion = cx.local.config_rpc_completion;
    let accel_calibration = cx.local.accel_calibration;
    let accel_calibration_seq = cx.local.accel_calibration_seq;
    let accel_calibration_deadline_ms = cx.local.accel_calibration_deadline_ms;
    #[cfg(not(feature = "mspv2_configurator"))]
    let _ = (
        flash_rpc_command_consumer,
        flash_rpc_response_producer,
        flash_rpc_device_info,
        staged_rpc_config,
        config_rpc_completion,
    );
    loop {
        // Log pages a store holds back in RAM (an SD card's write-back
        // window) are written out at the top of every pass once they are old
        // enough, or when no flight is recording (`LogWriter`).
        let now_ms = Mono::now().duration_since_epoch().to_millis();
        if log.flush_write_back(flash_device, now_ms).is_err() {
            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
            FLASH_READY.store(false, Ordering::Release);
            warn!("Storage flush failed; recording stopped");
        }
        {
            if !FLASH_READY.load(Ordering::Acquire) {
                #[cfg(feature = "mspv2_configurator")]
                if let Some(request) = flash_rpc_command_consumer.dequeue() {
                    let _ = queue_rpc_response(
                        flash_rpc_response_producer,
                        mspv2::rpc::error(
                            request.request_id,
                            mspv2::rpc::DeviceError::StorageUnavailable,
                        ),
                    );
                }
                if let Some((command, link)) = flash_commands.take_next() {
                    flash_responses.answer(link);
                    let response = flash_task::storage_unavailable_response(
                        &command,
                        [
                            FLASH_JEDEC_MANUFACTURER.load(Ordering::Relaxed),
                            FLASH_JEDEC_MEMORY_TYPE.load(Ordering::Relaxed),
                            FLASH_JEDEC_CAPACITY_CODE.load(Ordering::Relaxed),
                        ],
                    );
                    queue_storage_response(flash_responses, response.as_str());
                }
                Mono::delay(100u64.millis()).await;
                continue;
            }
            let Some(layout) =
                flash_task::StorageLayout::new(FLASH_CAPACITY_BYTES.load(Ordering::Relaxed))
            else {
                FLASH_READY.store(false, Ordering::Release);
                #[cfg(feature = "mspv2_configurator")]
                if let Some(request) = flash_rpc_command_consumer.dequeue() {
                    let _ = queue_rpc_response(
                        flash_rpc_response_producer,
                        mspv2::rpc::error(
                            request.request_id,
                            mspv2::rpc::DeviceError::StorageUnavailable,
                        ),
                    );
                }
                continue;
            };

            if !*initialized {
                let Ok((page, flight, writable)) = scan_flash_log(flash_device, layout) else {
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage log recovery failed; storage disabled");
                    continue;
                };
                let Ok((config, sequence, slot)) =
                    load_flash_config(flash_device, layout, CONFIG::DEFAULT_STORED_CONFIG)
                else {
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage configuration read failed; storage disabled");
                    continue;
                };
                log.recover(page, flight, writable);
                *stored_config = config;
                *config_sequence = sequence;
                *config_active_slot = slot;
                publish_accel_level(config.accel_level);
                FLASH_LOG_RATE_DIVISOR.store(u32::from(config.log_rate_divisor), Ordering::Relaxed);
                cx.shared
                    .tuning_profile
                    .lock(|profile| *profile = config.tuning);
                cx.shared
                    .tuning_request_seq
                    .lock(|request_sequence| *request_sequence = request_sequence.wrapping_add(1));
                *initialized = true;
                info!(
                    "Storage recovered at log page {}, next flight {}, writable {}, config seq {}",
                    page, flight, writable, sequence
                );
            }

            let busy = match flash_device.busy() {
                Ok(busy) => busy,
                Err(_) => {
                    FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage status read failed; storage disabled");
                    #[cfg(feature = "mspv2_configurator")]
                    let _ = finish_config_rpc_error(
                        flash_rpc_response_producer,
                        config_rpc_completion,
                        mspv2::rpc::DeviceError::StorageUnavailable,
                    );
                    continue;
                }
            };
            if busy {
                Mono::delay(1u64.millis()).await;
                continue;
            }

            // Whatever flash operation is running answers the link that
            // started it; a new command below switches to its own link.
            flash_responses.answer_maintenance();

            if SAFETY_ARMED.load(Ordering::Acquire)
                && (maintenance.is_active() || config_save.is_active())
            {
                // An erase stopped part-way has cleared the top of the log
                // and maybe some of its newest pages. Find the append
                // point again, or new pages would land above a blank gap
                // that hides them from every later scan.
                if maintenance.is_erasing_log() {
                    match scan_flash_log(flash_device, layout) {
                        Ok((page, flight, writable)) => {
                            log.recover(page, flight, writable);
                        }
                        Err(_) => {
                            FLASH_READY.store(false, Ordering::Release);
                            warn!(
                                "Storage log rescan after aborted erase failed; storage disabled"
                            );
                        }
                    }
                }
                maintenance.cancel();
                config_save.cancel();
                #[cfg(feature = "mspv2_configurator")]
                let rpc_notified = finish_config_rpc_error(
                    flash_rpc_response_producer,
                    config_rpc_completion,
                    mspv2::rpc::DeviceError::Armed,
                );
                #[cfg(not(feature = "mspv2_configurator"))]
                let rpc_notified = false;
                if !rpc_notified {
                    queue_storage_response(
                        flash_responses,
                        "ERR maintenance aborted because system armed\r\n",
                    );
                }
            }

            // A log erase, a sync-ledger acknowledgement or the scratch test:
            // one flash operation a pass, each awaited by the busy check above.
            let erasing_log = maintenance.is_erasing_log();
            match maintenance.step(flash_device, layout) {
                MaintenanceStep::Idle => {}
                step => {
                    match step {
                        MaintenanceStep::Done(MaintenanceDone::LogErased) => {
                            log.reset_after_erase();
                            queue_storage_response(flash_responses, "OK logs erased\r\n");
                        }
                        MaintenanceStep::Done(MaintenanceDone::Synced { flight }) => {
                            let mut line =
                                heapless::String::<{ flash_task::USB_RESPONSE_CAPACITY }>::new();
                            let _ = write!(line, "OK synced flight={flight}\r\n");
                            queue_storage_response(flash_responses, line.as_str());
                        }
                        MaintenanceStep::Done(MaintenanceDone::ScratchVerified) => {
                            queue_storage_response(
                                flash_responses,
                                "OK flash scratch erase/program/read verified\r\n",
                            );
                        }
                        MaintenanceStep::Failed(failure) => {
                            if failure.is_write_fault() {
                                FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            }
                            queue_storage_response(
                                flash_responses,
                                match failure {
                                    MaintenanceFailure::LogErase => {
                                        "ERR log sector erase failed\r\n"
                                    }
                                    MaintenanceFailure::SyncLedger { .. } => {
                                        "ERR sync ledger write failed\r\n"
                                    }
                                    MaintenanceFailure::ScratchErase => {
                                        "ERR flash test scratch erase failed\r\n"
                                    }
                                    MaintenanceFailure::ScratchProgram => {
                                        "ERR flash test scratch program failed\r\n"
                                    }
                                    MaintenanceFailure::ScratchReadback => {
                                        "ERR flash test readback mismatch\r\n"
                                    }
                                },
                            );
                        }
                        MaintenanceStep::Running | MaintenanceStep::Idle => {}
                    }
                    // A log erase yields a millisecond after every sector.
                    if erasing_log {
                        Mono::delay(1u64.millis()).await;
                    }
                    continue;
                }
            }

            match config_save.step(flash_device, layout) {
                SaveStep::Idle => {}
                SaveStep::Running => continue,
                SaveStep::Failed(failure) => {
                    FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                    #[cfg(feature = "mspv2_configurator")]
                    let rpc_notified = finish_config_rpc_error(
                        flash_rpc_response_producer,
                        config_rpc_completion,
                        match failure {
                            SaveFailure::Verify => mspv2::rpc::DeviceError::ChecksumMismatch,
                            _ => mspv2::rpc::DeviceError::WriteFailure,
                        },
                    );
                    #[cfg(not(feature = "mspv2_configurator"))]
                    let rpc_notified = false;
                    if !rpc_notified {
                        queue_storage_response(
                            flash_responses,
                            match failure {
                                SaveFailure::Erase => "ERR config slot erase failed\r\n",
                                SaveFailure::Program => "ERR config page program failed\r\n",
                                SaveFailure::Verify | SaveFailure::Encoding => {
                                    "ERR config persistence verification failed\r\n"
                                }
                            },
                        );
                    }
                    continue;
                }
                SaveStep::Saved {
                    config,
                    slot,
                    sequence,
                } => {
                    *stored_config = config;
                    *config_active_slot = slot;
                    *config_sequence = sequence;
                    publish_accel_level(stored_config.accel_level);
                    FLASH_LOG_RATE_DIVISOR
                        .store(u32::from(stored_config.log_rate_divisor), Ordering::Relaxed);
                    cx.shared
                        .tuning_profile
                        .lock(|profile| *profile = stored_config.tuning);
                    cx.shared
                        .tuning_request_seq
                        .lock(|sequence| *sequence = sequence.wrapping_add(1));
                    #[cfg(feature = "mspv2_configurator")]
                    let rpc_notified = if let Some(completion) = config_rpc_completion.take() {
                        let crc = stored_config.crc32();
                        let (request_id, response) = match completion {
                            ConfigRpcCompletion::Commit(id) => (
                                id,
                                mspv2::rpc::Response::ConfigCommitted {
                                    persisted_crc32: crc,
                                    reboot_required: false,
                                },
                            ),
                            ConfigRpcCompletion::Defaults(id) => (
                                id,
                                mspv2::rpc::Response::DefaultsRestored {
                                    persisted_crc32: crc,
                                    reboot_required: false,
                                },
                            ),
                        };
                        queue_rpc_response(
                            flash_rpc_response_producer,
                            mspv2::rpc::ok(request_id, response),
                        )
                    } else {
                        false
                    };
                    #[cfg(not(feature = "mspv2_configurator"))]
                    let rpc_notified = false;
                    if !rpc_notified {
                        queue_storage_response(flash_responses, "OK config saved\r\n");
                    }
                }
            }

            if let Some((command, link)) = flash_commands.take_next() {
                flash_responses.answer(link);
                let mut response = heapless::String::<{ flash_task::USB_RESPONSE_CAPACITY }>::new();
                let maintenance_busy = maintenance.is_active()
                    || config_save.is_active()
                    || accel_calibration.is_some();
                let unfinished_flight = log.unfinished_flight();
                match command {
                    // The configurator link refuses these before queueing
                    // them; this is the second check.
                    command if link == CommandLink::Serial && command.usb_only() => {
                        queue_storage_response(
                            flash_responses,
                            "ERR USB only for this command\r\n",
                        );
                    }
                    flash_task::StorageCommand::Help => {
                        queue_storage_response(
                            flash_responses,
                            "OK flash info | flash test CONFIRM | logs list\r\n",
                        );
                        queue_storage_response(
                            flash_responses,
                            "OK logs erase CONFIRM | config get/set KEY | config save\r\n",
                        );
                        queue_storage_response(
                            flash_responses,
                            "OK logs unsynced | logs ack FLIGHT PAGES\r\n",
                        );
                        queue_storage_response(flash_responses, "OK logs erase-synced CONFIRM\r\n");
                        queue_storage_response(
                            flash_responses,
                            "OK serial | serial PORT FUNCTION\r\n",
                        );
                        queue_storage_response(
                            flash_responses,
                            "OK FUNCTION none/rc/osd/esc_telemetry/configurator\r\n",
                        );
                        queue_storage_response(
                            flash_responses,
                            "OK live | rc | motor stop | motor spin N CONFIRM\r\n",
                        );
                        queue_storage_response(
                            flash_responses,
                            "OK motor dir N normal|reversed CONFIRM\r\n",
                        );
                        queue_storage_response(flash_responses, "OK reboot | reboot dfu\r\n");
                        queue_storage_response(flash_responses, "OK imu calibrate | imu level\r\n");
                    }
                    // Answered by the USB task; never queued here.
                    flash_task::StorageCommand::Live
                    | flash_task::StorageCommand::Rc
                    | flash_task::StorageCommand::Motor(_) => {}
                    flash_task::StorageCommand::SerialShow => {
                        stm32_port::write_serial_bindings(
                            stored_config.serial_bindings,
                            CONFIG::DEFAULT_SERIAL_BINDINGS,
                            CONFIG::SERIAL_ROUTES,
                            |line| {
                                queue_storage_response(flash_responses, line);
                            },
                        );
                    }
                    flash_task::StorageCommand::SerialSet(port, function) => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR config changes disabled while armed\r\n",
                            );
                        } else {
                            let mut bindings = stored_config
                                .serial_bindings
                                .unwrap_or(CONFIG::DEFAULT_SERIAL_BINDINGS);
                            bindings.set(port, function);
                            stored_config.serial_bindings = Some(bindings);
                            queue_storage_response(
                                flash_responses,
                                "OK staged; use config save, then reboot\r\n",
                            );
                        }
                    }
                    flash_task::StorageCommand::FlashInfo => {
                        let _ = write!(
                            response,
                            "OK jedec={:02x}:{:02x}:{:02x} bytes={} ready=1\r\n",
                            FLASH_JEDEC_MANUFACTURER.load(Ordering::Relaxed),
                            FLASH_JEDEC_MEMORY_TYPE.load(Ordering::Relaxed),
                            FLASH_JEDEC_CAPACITY_CODE.load(Ordering::Relaxed),
                            layout.capacity_bytes
                        );
                        queue_storage_response(flash_responses, response.as_str());
                    }
                    flash_task::StorageCommand::FlashTestConfirmed => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash test disabled while armed\r\n",
                            );
                        } else if maintenance.is_active()
                            || config_save.is_active()
                            || accel_calibration.is_some()
                        {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            *maintenance = Maintenance::scratch_test();
                            flash_responses.start_maintenance();
                            queue_storage_response(
                                flash_responses,
                                "OK flash scratch test started\r\n",
                            );
                        }
                    }
                    flash_task::StorageCommand::LogsList => {
                        if let Some(response) = flash_task::format_log_info_response(
                            log.next_page(),
                            log.next_flight(),
                            layout.log_page_count,
                            log.writable(),
                        ) {
                            queue_storage_response(flash_responses, response.as_str());
                        } else {
                            queue_storage_response(
                                flash_responses,
                                "ERR log summary formatting failed\r\n",
                            );
                        }
                    }
                    flash_task::StorageCommand::LogsReadPage(page_index) => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR log reads disabled while armed\r\n",
                            );
                        } else if page_index >= log.next_page() {
                            queue_storage_response(
                                flash_responses,
                                "ERR log page is not present\r\n",
                            );
                        } else if let Some(address) = layout.log_page_address(page_index) {
                            let mut page = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                            if flash_device.read(address, &mut page).is_err()
                                || !queue_page_hex(flash_responses, page_index, &page)
                            {
                                queue_storage_response(
                                    flash_responses,
                                    "ERR log page read/response failed\r\n",
                                );
                            }
                        }
                    }
                    flash_task::StorageCommand::LogsEraseConfirmed => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR log erase disabled while armed\r\n",
                            );
                        } else if maintenance.is_active()
                            || config_save.is_active()
                            || accel_calibration.is_some()
                        {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            *maintenance = Maintenance::erase_log();
                            flash_responses.start_maintenance();
                            queue_storage_response(flash_responses, "OK log erase started\r\n");
                        }
                    }
                    flash_task::StorageCommand::LogsUnsynced => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR log reads disabled while armed\r\n",
                            );
                        } else {
                            let mut read =
                                |address, output: &mut [u8]| flash_device.read(address, output);
                            match log_sync::unsynced_flights(
                                layout,
                                &mut read,
                                log.next_page(),
                                unfinished_flight,
                            ) {
                                Ok((flights, more)) => {
                                    log_sync::emit_unsynced_lines(&flights, more, |line| {
                                        queue_storage_response(flash_responses, line)
                                    });
                                }
                                Err(_) => {
                                    queue_storage_response(
                                        flash_responses,
                                        "ERR log scan failed\r\n",
                                    );
                                }
                            }
                        }
                    }
                    flash_task::StorageCommand::LogsAck { flight, pages } => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR log sync disabled while armed\r\n",
                            );
                        } else if maintenance_busy {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            let mut read =
                                |address, output: &mut [u8]| flash_device.read(address, output);
                            match log_sync::check_ack(
                                layout,
                                &mut read,
                                log.next_page(),
                                unfinished_flight,
                                flight,
                                pages,
                            ) {
                                Ok(Ok(())) => {
                                    *maintenance = Maintenance::sync_mark(flight);
                                    flash_responses.start_maintenance();
                                }
                                Ok(Err(refusal)) => {
                                    queue_storage_response(flash_responses, refusal.response());
                                }
                                Err(_) => {
                                    queue_storage_response(
                                        flash_responses,
                                        "ERR log scan failed\r\n",
                                    );
                                }
                            }
                        }
                    }
                    flash_task::StorageCommand::LogsEraseSyncedConfirmed => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR log erase disabled while armed\r\n",
                            );
                        } else if !log.writable() && log.next_page() < layout.log_page_count {
                            // Something other than flights follows the
                            // last one, so nothing says it was stored.
                            queue_storage_response(
                                flash_responses,
                                "ERR log holds data that is not a flight; erase over USB\r\n",
                            );
                        } else if maintenance_busy {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            let mut read =
                                |address, output: &mut [u8]| flash_device.read(address, output);
                            match log_sync::all_synced(
                                layout,
                                &mut read,
                                log.next_page(),
                                unfinished_flight,
                            ) {
                                Ok(true) => {
                                    *maintenance = Maintenance::erase_log();
                                    flash_responses.start_maintenance();
                                    queue_storage_response(
                                        flash_responses,
                                        "OK log erase started\r\n",
                                    );
                                }
                                Ok(false) => {
                                    queue_storage_response(
                                        flash_responses,
                                        "ERR unsynced flights remain\r\n",
                                    );
                                }
                                Err(_) => {
                                    queue_storage_response(
                                        flash_responses,
                                        "ERR log scan failed\r\n",
                                    );
                                }
                            }
                        }
                    }
                    flash_task::StorageCommand::ConfigGet(key) => {
                        let _ = write!(
                            response,
                            "OK {}={:.4}\r\n",
                            key.name(),
                            stored_config.get(key)
                        );
                        queue_storage_response(flash_responses, response.as_str());
                    }
                    flash_task::StorageCommand::ConfigSet(key, value) => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR config changes disabled while armed\r\n",
                            );
                        } else if stored_config.set(key, value) {
                            queue_storage_response(
                                flash_responses,
                                "OK staged; use config save\r\n",
                            );
                        } else {
                            queue_storage_response(
                                flash_responses,
                                "ERR value outside allowed range\r\n",
                            );
                        }
                    }
                    flash_task::StorageCommand::ConfigSave => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR config save disabled while armed\r\n",
                            );
                        } else if maintenance.is_active()
                            || config_save.is_active()
                            || accel_calibration.is_some()
                        {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            let answer = match begin_config_save(
                                config_save,
                                *stored_config,
                                *config_sequence,
                                *config_active_slot,
                                flash_responses,
                            ) {
                                Ok(()) => "OK config save started\r\n",
                                Err(answer) => answer,
                            };
                            queue_storage_response(flash_responses, answer);
                        }
                    }
                    flash_task::StorageCommand::ImuLevel => {
                        let [x, y, z] = stored_config.accel_level.offset_mg;
                        let _ = write!(response, "OK imu level x={x} y={y} z={z} mg\r\n");
                        queue_storage_response(flash_responses, &response);
                    }
                    flash_task::StorageCommand::ImuCalibrate => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            queue_storage_response(
                                flash_responses,
                                "ERR imu calibration disabled while armed\r\n",
                            );
                        } else if maintenance_busy {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            *accel_calibration = Some(AccelLevelCalibration::new());
                            flash_responses.start_maintenance();
                            *accel_calibration_seq = IMU_LATEST_SEQ.load(Ordering::Acquire);
                            *accel_calibration_deadline_ms =
                                Mono::now().duration_since_epoch().to_millis()
                                    + IMU_CALIBRATION_DEADLINE_MS;
                            queue_storage_response(
                                flash_responses,
                                "OK imu calibration started; keep the craft still\r\n",
                            );
                        }
                    }
                    // Restarting cuts the store off: only once disarmed, with no
                    // maintenance or flight log open, written-back pages
                    // flushed, and the last program or erase finished.
                    flash_task::StorageCommand::Reboot { rom_bootloader } => {
                        if let Some(refusal) = reboot_refusal(
                            SAFETY_ARMED.load(Ordering::Acquire),
                            maintenance_busy,
                            unfinished_flight.is_some(),
                        ) {
                            queue_storage_response(flash_responses, refusal);
                        } else {
                            let flushed =
                                !flash_device.write_back_pending() || flash_device.flush().is_ok();
                            let mut idle = false;
                            for _ in 0..REBOOT_STORAGE_SETTLE_MS {
                                if matches!(flash_device.busy(), Ok(false)) {
                                    idle = true;
                                    break;
                                }
                                Mono::delay(1u64.millis()).await;
                            }
                            if !flushed || !idle {
                                queue_storage_response(
                                    flash_responses,
                                    "ERR storage did not settle; not rebooting\r\n",
                                );
                            } else {
                                queue_storage_response(
                                    flash_responses,
                                    if rom_bootloader {
                                        "OK rebooting to DFU\r\n"
                                    } else {
                                        "OK rebooting\r\n"
                                    },
                                );
                                // Let the USB task send the answer before the
                                // port goes away.
                                Mono::delay(REBOOT_ANSWER_MS.millis()).await;
                                if SAFETY_ARMED.load(Ordering::Acquire) {
                                    queue_storage_response(
                                        flash_responses,
                                        "ERR reboot cancelled; system armed\r\n",
                                    );
                                } else {
                                    warn!(
                                        "Rebooting on request, ROM bootloader: {}",
                                        rom_bootloader
                                    );
                                    backend::system_bootloader::reboot(if rom_bootloader {
                                        RebootTarget::RomBootloader
                                    } else {
                                        RebootTarget::Firmware
                                    });
                                }
                            }
                        }
                    }
                }
            }

            #[cfg(feature = "mspv2_configurator")]
            if let Some(request) = flash_rpc_command_consumer.dequeue() {
                let request_id = request.request_id;
                let maintenance_busy = maintenance.is_active()
                    || config_save.is_active()
                    || accel_calibration.is_some();
                let active_blackbox_id = log
                    .recording()
                    .then(|| log.next_flight().wrapping_sub(1).max(1));
                let response = match request.operation {
                    mspv2::rpc::Request::Hello => Some(mspv2::rpc::ok(
                        request_id,
                        mspv2::rpc::Response::Hello((*flash_rpc_device_info)()),
                    )),
                    mspv2::rpc::Request::GetConfig => {
                        let crc = stored_config.crc32();
                        Some(mspv2::rpc::ok(
                            request_id,
                            mspv2::rpc::Response::Config(mspv2::rpc::ConfigSnapshot {
                                schema_version: mspv2::rpc::CONFIG_SCHEMA_VERSION,
                                active_crc32: crc,
                                persisted_crc32: crc,
                                config: stored_config.to_rpc(),
                            }),
                        ))
                    }
                    mspv2::rpc::Request::StageConfig(config) => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            Some(mspv2::rpc::error(
                                request_id,
                                mspv2::rpc::DeviceError::Armed,
                            ))
                        } else if maintenance_busy {
                            Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                        } else {
                            match stored_config.apply_rpc(config) {
                                Ok(candidate) => {
                                    let crc = candidate.crc32();
                                    *staged_rpc_config = Some(candidate);
                                    Some(mspv2::rpc::ok(
                                        request_id,
                                        mspv2::rpc::Response::ConfigStaged {
                                            staged_crc32: crc,
                                            reboot_required: false,
                                        },
                                    ))
                                }
                                Err(field) => Some(mspv2::rpc::RpcResponse {
                                    protocol_version: mspv2::rpc::FWSP_RPC_VERSION,
                                    request_id,
                                    result: mspv2::rpc::RpcResult::Err(mspv2::rpc::ErrorDetail {
                                        code: mspv2::rpc::DeviceError::InvalidConfiguration,
                                        field: Some(field),
                                        argument: None,
                                    }),
                                }),
                            }
                        }
                    }
                    mspv2::rpc::Request::CommitConfig {
                        expected_staged_crc32,
                    } => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            Some(mspv2::rpc::error(
                                request_id,
                                mspv2::rpc::DeviceError::Armed,
                            ))
                        } else if maintenance_busy {
                            Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                        } else if let Some(candidate) = *staged_rpc_config {
                            if candidate.crc32() != expected_staged_crc32 {
                                Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::ConfigurationConflict,
                                ))
                            } else {
                                match config_save.begin(
                                    candidate,
                                    *config_sequence,
                                    *config_active_slot,
                                ) {
                                    Ok(()) => {
                                        *config_rpc_completion =
                                            Some(ConfigRpcCompletion::Commit(request_id));
                                        *staged_rpc_config = None;
                                        None
                                    }
                                    Err(_) => Some(mspv2::rpc::error(
                                        request_id,
                                        mspv2::rpc::DeviceError::Internal,
                                    )),
                                }
                            }
                        } else {
                            Some(mspv2::rpc::error(
                                request_id,
                                mspv2::rpc::DeviceError::ConfigurationConflict,
                            ))
                        }
                    }
                    mspv2::rpc::Request::ResetConfigToDefaults => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            Some(mspv2::rpc::error(
                                request_id,
                                mspv2::rpc::DeviceError::Armed,
                            ))
                        } else if maintenance_busy {
                            Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                        } else {
                            match config_save.begin(
                                CONFIG::DEFAULT_STORED_CONFIG,
                                *config_sequence,
                                *config_active_slot,
                            ) {
                                Ok(()) => {
                                    *config_rpc_completion =
                                        Some(ConfigRpcCompletion::Defaults(request_id));
                                    *staged_rpc_config = None;
                                    None
                                }
                                Err(_) => Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::Internal,
                                )),
                            }
                        }
                    }
                    mspv2::rpc::Request::ListBlackboxes => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            Some(mspv2::rpc::error(
                                request_id,
                                mspv2::rpc::DeviceError::Armed,
                            ))
                        } else {
                            match list_blackboxes(
                                flash_device,
                                layout,
                                log.next_page(),
                                active_blackbox_id,
                            ) {
                                Ok(list) => Some(mspv2::rpc::ok(
                                    request_id,
                                    mspv2::rpc::Response::BlackboxList(list),
                                )),
                                Err(_) => Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::ReadFailure,
                                )),
                            }
                        }
                    }
                    mspv2::rpc::Request::GetBlackboxInfo { id } => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            Some(mspv2::rpc::error(
                                request_id,
                                mspv2::rpc::DeviceError::Armed,
                            ))
                        } else {
                            match blackbox_info(
                                flash_device,
                                layout,
                                log.next_page(),
                                id,
                                active_blackbox_id,
                            ) {
                                Ok(Some(info)) => Some(mspv2::rpc::ok(
                                    request_id,
                                    mspv2::rpc::Response::BlackboxInfo(info),
                                )),
                                Ok(None) => Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::BlackboxNotFound,
                                )),
                                Err(_) => Some(mspv2::rpc::error(
                                    request_id,
                                    mspv2::rpc::DeviceError::ReadFailure,
                                )),
                            }
                        }
                    }
                    mspv2::rpc::Request::ReadBlackboxChunk {
                        id,
                        offset,
                        requested_length,
                    } => {
                        if SAFETY_ARMED.load(Ordering::Acquire) {
                            Some(mspv2::rpc::error(
                                request_id,
                                mspv2::rpc::DeviceError::Armed,
                            ))
                        } else if active_blackbox_id == Some(id.0) {
                            Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
                        } else {
                            match read_blackbox_chunk(
                                flash_device,
                                layout,
                                log.next_page(),
                                id,
                                offset,
                                requested_length,
                            ) {
                                Ok(chunk) => Some(mspv2::rpc::ok(
                                    request_id,
                                    mspv2::rpc::Response::BlackboxChunk(chunk),
                                )),
                                Err(error) => Some(mspv2::rpc::error(request_id, error)),
                            }
                        }
                    }
                    mspv2::rpc::Request::EraseBlackbox { .. }
                    | mspv2::rpc::Request::Reboot { .. } => Some(mspv2::rpc::error(
                        request_id,
                        mspv2::rpc::DeviceError::UnsupportedOperation,
                    )),
                };
                if let Some(response) = response {
                    let _ = queue_rpc_response(flash_rpc_response_producer, response);
                }
            }
            // One IMU sample a pass into a level calibration, answered on the
            // link that asked; a success is saved like `config save`.
            if let Some(calibration) = accel_calibration.as_mut() {
                let sequence = IMU_LATEST_SEQ.load(Ordering::Acquire);
                let outcome = if SAFETY_ARMED.load(Ordering::Acquire) {
                    Some(Err("ERR imu calibration aborted; the system armed\r\n"))
                } else if Mono::now().duration_since_epoch().to_millis()
                    > *accel_calibration_deadline_ms
                {
                    Some(Err("ERR imu calibration timed out; no IMU samples\r\n"))
                } else if sequence != *accel_calibration_seq {
                    *accel_calibration_seq = sequence;
                    // One sample, both sensors: the parser stores them in g and
                    // degrees per second.
                    let (sensor_accel, gyro_dps) =
                        cx.shared.imu_data.lock(|imu| (imu.acc, imu.gyro));
                    let gravity = CONFIG::IMU_CONTROL_AXIS_PROFILE
                        .sensor_accel_to_drone_gravity(sensor_accel);
                    calibration
                        .push(gravity, gyro_dps)
                        .map(|result| result.map_err(calibration_failure))
                } else {
                    None
                };
                if let Some(outcome) = outcome {
                    *accel_calibration = None;
                    flash_responses.answer_maintenance();
                    let saved = outcome.and_then(|level| {
                        let mut config = *stored_config;
                        config.accel_level = level;
                        if config_save.is_active() || maintenance.is_active() {
                            return Err("ERR flash maintenance already active\r\n");
                        }
                        begin_config_save(
                            config_save,
                            config,
                            *config_sequence,
                            *config_active_slot,
                            flash_responses,
                        )
                        .map(|()| level)
                    });
                    match saved {
                        Ok(level) => {
                            let [x, y, z] = level.offset_mg;
                            let mut line =
                                heapless::String::<{ flash_task::USB_RESPONSE_CAPACITY }>::new();
                            let _ = write!(line, "OK imu calibrated x={x} y={y} z={z} mg\r\n");
                            queue_storage_response(flash_responses, &line);
                        }
                        Err(answer) => {
                            queue_storage_response(flash_responses, answer);
                        }
                    }
                }
            }

            match log.write_pending(flash_device, layout) {
                PageWrite::Nothing => {}
                PageWrite::Written => {
                    FLASH_PAGES_WRITTEN.fetch_add(1, Ordering::Relaxed);
                    // Fall through to the record drain rather than yielding
                    // here: a pass that either wrote a page or consumed
                    // records, never both, halved the store's throughput.
                }
                PageWrite::RegionFull => {
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage blackbox region full; recording stopped");
                    continue;
                }
                PageWrite::ProgramFailed => {
                    FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage page program failed; recording stopped");
                    continue;
                }
            }
            let drained = log.drain(
                flash_device,
                layout,
                flash_record_consumer,
                SAFETY_ARMED.load(Ordering::Acquire),
                // Bench-only: record while disarmed, so the flash write path
                // can be exercised without flying.
                cfg!(feature = "bench_blackbox"),
            );
            FLASH_PAGES_WRITTEN.fetch_add(drained.pages_written, Ordering::Relaxed);
            FLASH_RECORDS_DROPPED.fetch_add(drained.records_dropped, Ordering::Relaxed);
            FLASH_WRITE_FAULTS.fetch_add(drained.write_faults, Ordering::Relaxed);
        }

        Mono::delay(1u64.millis()).await;
    }
}
