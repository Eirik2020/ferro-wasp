//! The storage owner: the flight log, stored configuration, log sync and
//! the storage commands from USB, the configurator link and MSPv2. One task
//! for every board, over any store with NOR semantics; a board supplies its
//! store and its defaults.

use crate::prelude::*;
use crate::snapshots::*;
use core::fmt::Write as _;
use ferrowasp_drivers::nor_storage::NorStorage;
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_flight::blackbox_storage as blackbox_task;
use ferrowasp_flight::command_link::{CommandLink, CommandSources, ResponseRouter, reboot_refusal};
use ferrowasp_flight::{log_sync, sync_ledger};
use ferrowasp_io_core::serial::{SerialBindings, SerialRoute};
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2 as mspv2;
use ferrowasp_stm32::system_reset::RebootTarget;
use ferrowasp_stm32::uart_port as stm32_port;

/// How many blackbox records the flash task consumes per pass.
///
/// Its loop yields for a millisecond, so one record per pass capped the store
/// near a thousand records a second regardless of the flash. Bounded so a full
/// queue cannot starve the rest of the system.
pub const FLASH_RECORD_DRAIN_PER_PASS: u32 = 16;

/// How long log pages may wait in a store's write-back buffer before the
/// task writes them out; at most this much log is lost to a power cut.
pub const WRITE_BACK_MAX_AGE_MS: u64 = 100;

/// Erase, header and bit are the most one acknowledgement can take.
pub const MAX_SYNC_MARK_STEPS: u8 = 3;

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

/// The next ledger operation toward acknowledging `flight_id`.
pub fn sync_mark_step<F: NorStorage>(
    flash: &mut F,
    layout: flash_task::StorageLayout,
    flight_id: u32,
) -> Result<sync_ledger::MarkStep, sync_ledger::LedgerError<F::Error>> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output);
    sync_ledger::next_mark_step(layout, &mut read, flight_id)
}

/// Starts one ledger operation; the storage owner waits for it to finish.
pub fn start_sync_mark_step<F: NorStorage>(
    flash: &mut F,
    step: sync_ledger::MarkStep,
) -> Result<(), F::Error> {
    match step {
        sync_ledger::MarkStep::EraseLedger { address } => flash.erase_sector_4k(address),
        sync_ledger::MarkStep::ProgramHeader { address } => {
            flash.page_program(address, &sync_ledger::SYNC_LEDGER_MAGIC)
        }
        sync_ledger::MarkStep::ProgramBit { address, value } => {
            flash.page_program(address, &[value])
        }
        sync_ledger::MarkStep::Done => Ok(()),
    }
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

pub fn flash_scratch_test_page() -> [u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN] {
    flash_task::scratch_test_page()
}

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
        assembler: flash_task::PageAssembler = flash_task::PageAssembler::new(),
        boot_session_start_pending: bool = true,
        pending_page: Option<[u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN]> = None,
        initialized: bool = false,
        next_page_index: u32 = 0,
        next_flight_id: u32 = 1,
        log_region_writable: bool = false,
        stored_config: flash_task::StoredConfig,
        config_sequence: u32 = 0,
        config_active_slot: u8 = 1,
        erase_sector_index: Option<u32> = None,
        flash_test_phase: u8 = 0,
        config_save_phase: u8 = 0,
        sync_mark: Option<u32> = None,
        sync_mark_steps: u8 = 0,
        config_save_slot: u8 = 0,
        config_save_candidate: flash_task::StoredConfig,
        config_save_page: [u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN] =
            [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN],
        staged_rpc_config: Option<flash_task::StoredConfig> = None,
        config_rpc_completion: Option<ConfigRpcCompletion> = None,
        write_back_since_ms: Option<u64> = None,
    ],
    shared = [tuning_profile: dt::TuningProfile, tuning_request_seq: u32],
    config = [
        default_stored_config: flash_task::StoredConfig,
        default_serial_bindings: SerialBindings,
        serial_routes: &'static [SerialRoute],
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
    let assembler = cx.local.assembler;
    let boot_session_start_pending = cx.local.boot_session_start_pending;
    let pending_page = cx.local.pending_page;
    let initialized = cx.local.initialized;
    let next_page_index = cx.local.next_page_index;
    let next_flight_id = cx.local.next_flight_id;
    let log_region_writable = cx.local.log_region_writable;
    let stored_config = cx.local.stored_config;
    let config_sequence = cx.local.config_sequence;
    let config_active_slot = cx.local.config_active_slot;
    let erase_sector_index = cx.local.erase_sector_index;
    let flash_test_phase = cx.local.flash_test_phase;
    let config_save_phase = cx.local.config_save_phase;
    let sync_mark = cx.local.sync_mark;
    let sync_mark_steps = cx.local.sync_mark_steps;
    let config_save_slot = cx.local.config_save_slot;
    let config_save_candidate = cx.local.config_save_candidate;
    let config_save_page = cx.local.config_save_page;
    let staged_rpc_config = cx.local.staged_rpc_config;
    let config_rpc_completion = cx.local.config_rpc_completion;
    let write_back_since_ms = cx.local.write_back_since_ms;
    #[cfg(not(feature = "mspv2_configurator"))]
    let _ = (
        flash_rpc_command_consumer,
        flash_rpc_response_producer,
        flash_rpc_device_info,
        staged_rpc_config,
        config_rpc_completion,
    );
    loop {
        // Log pages a store holds back in RAM (an SD card's write-back window)
        // wait there until it fills. Write
        // them out once they have waited WRITE_BACK_MAX_AGE_MS, or as
        // soon as no flight is recording, so a power cut loses at most
        // that much log. At the top of the pass, which every pass reaches.
        if flash_device.write_back_pending() {
            let now_ms = Mono::now().duration_since_epoch().to_millis();
            let since_ms = *write_back_since_ms.get_or_insert(now_ms);
            if !assembler.recording() || now_ms.saturating_sub(since_ms) >= WRITE_BACK_MAX_AGE_MS {
                *write_back_since_ms = None;
                if flash_device.flush().is_err() {
                    FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage flush failed; recording stopped");
                }
            }
        } else {
            *write_back_since_ms = None;
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
                *next_page_index = page;
                *next_flight_id = flight;
                *log_region_writable = writable;
                *stored_config = config;
                *config_save_candidate = config;
                *config_sequence = sequence;
                *config_active_slot = slot;
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
                && (erase_sector_index.is_some()
                    || *flash_test_phase != 0
                    || *config_save_phase != 0
                    || sync_mark.is_some())
            {
                // An erase stopped part-way has cleared the top of the log
                // and maybe some of its newest pages. Find the append
                // point again, or new pages would land above a blank gap
                // that hides them from every later scan.
                if erase_sector_index.is_some() {
                    match scan_flash_log(flash_device, layout) {
                        Ok((page, flight, writable)) => {
                            *next_page_index = page;
                            *next_flight_id = flight;
                            *log_region_writable = writable;
                        }
                        Err(_) => {
                            FLASH_READY.store(false, Ordering::Release);
                            warn!(
                                "Storage log rescan after aborted erase failed; storage disabled"
                            );
                        }
                    }
                }
                *erase_sector_index = None;
                *flash_test_phase = 0;
                *config_save_phase = 0;
                *sync_mark = None;
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

            // Top down, sync ledger first: see `log_erase_sector_address`.
            if let Some(step) = *erase_sector_index {
                if let Some(address) = layout.log_erase_sector_address(step) {
                    if flash_device.erase_sector_4k(address).is_err() {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        *erase_sector_index = None;
                        queue_storage_response(flash_responses, "ERR log sector erase failed\r\n");
                    } else {
                        *erase_sector_index = Some(step + 1);
                    }
                } else {
                    *erase_sector_index = None;
                    *next_page_index = 0;
                    *next_flight_id = 1;
                    *log_region_writable = true;
                    queue_storage_response(flash_responses, "OK logs erased\r\n");
                }
                Mono::delay(1u64.millis()).await;
                continue;
            }

            // A host's acknowledgement, written to the sync ledger one
            // operation per pass, each awaited by the busy check above.
            if let Some(flight_id) = *sync_mark {
                let failed = match sync_mark_step(flash_device, layout, flight_id) {
                    Ok(sync_ledger::MarkStep::Done) => Some(false),
                    Ok(_) if *sync_mark_steps >= MAX_SYNC_MARK_STEPS => Some(true),
                    Ok(step) => {
                        *sync_mark_steps += 1;
                        if start_sync_mark_step(flash_device, step).is_err() {
                            FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                            Some(true)
                        } else {
                            None
                        }
                    }
                    Err(_) => Some(true),
                };
                if let Some(failed) = failed {
                    *sync_mark = None;
                    if failed {
                        queue_storage_response(flash_responses, "ERR sync ledger write failed\r\n");
                    } else {
                        let mut line =
                            heapless::String::<{ flash_task::USB_RESPONSE_CAPACITY }>::new();
                        let _ = write!(line, "OK synced flight={flight_id}\r\n");
                        queue_storage_response(flash_responses, line.as_str());
                    }
                }
                continue;
            }

            match *flash_test_phase {
                1 => {
                    if flash_device
                        .erase_sector_4k(flash_task::SCRATCH_SECTOR_ADDRESS)
                        .is_err()
                    {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        *flash_test_phase = 0;
                        queue_storage_response(
                            flash_responses,
                            "ERR flash test scratch erase failed\r\n",
                        );
                    } else {
                        *flash_test_phase = 2;
                    }
                    continue;
                }
                2 => {
                    let page = flash_scratch_test_page();
                    if flash_device
                        .page_program(flash_task::SCRATCH_SECTOR_ADDRESS, &page)
                        .is_err()
                    {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        *flash_test_phase = 0;
                        queue_storage_response(
                            flash_responses,
                            "ERR flash test scratch program failed\r\n",
                        );
                    } else {
                        *flash_test_phase = 3;
                    }
                    continue;
                }
                3 => {
                    let expected = flash_scratch_test_page();
                    let mut actual = [0u8; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                    *flash_test_phase = 0;
                    if flash_device
                        .read(flash_task::SCRATCH_SECTOR_ADDRESS, &mut actual)
                        .is_ok()
                        && actual == expected
                    {
                        queue_storage_response(
                            flash_responses,
                            "OK flash scratch erase/program/read verified\r\n",
                        );
                    } else {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        queue_storage_response(
                            flash_responses,
                            "ERR flash test readback mismatch\r\n",
                        );
                    }
                    continue;
                }
                _ => {}
            }

            match *config_save_phase {
                1 => {
                    let address = layout.config_slot_addresses[*config_save_slot as usize];
                    if flash_device.erase_sector_4k(address).is_err() {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        *config_save_phase = 0;
                        #[cfg(feature = "mspv2_configurator")]
                        let rpc_notified = finish_config_rpc_error(
                            flash_rpc_response_producer,
                            config_rpc_completion,
                            mspv2::rpc::DeviceError::WriteFailure,
                        );
                        #[cfg(not(feature = "mspv2_configurator"))]
                        let rpc_notified = false;
                        if !rpc_notified {
                            queue_storage_response(
                                flash_responses,
                                "ERR config slot erase failed\r\n",
                            );
                        }
                    } else {
                        *config_save_phase = 2;
                    }
                    continue;
                }
                2 => {
                    let address = layout.config_slot_addresses[*config_save_slot as usize];
                    if flash_device
                        .page_program(address, config_save_page)
                        .is_err()
                    {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        *config_save_phase = 0;
                        #[cfg(feature = "mspv2_configurator")]
                        let rpc_notified = finish_config_rpc_error(
                            flash_rpc_response_producer,
                            config_rpc_completion,
                            mspv2::rpc::DeviceError::WriteFailure,
                        );
                        #[cfg(not(feature = "mspv2_configurator"))]
                        let rpc_notified = false;
                        if !rpc_notified {
                            queue_storage_response(
                                flash_responses,
                                "ERR config page program failed\r\n",
                            );
                        }
                    } else {
                        *config_save_phase = 3;
                    }
                    continue;
                }
                3 => {
                    let address = layout.config_slot_addresses[*config_save_slot as usize];
                    let mut persisted = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                    let expected_sequence = config_sequence.wrapping_add(1);
                    let expected_payload = config_save_candidate.encode();
                    let verified = flash_device.read(address, &mut persisted).is_ok()
                        && matches!(
                            ferrowasp_core::blackbox::decode_config_page(&persisted),
                            Ok((sequence, payload))
                                if sequence == expected_sequence
                                    && payload == expected_payload.as_slice()
                        );
                    if !verified {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                        *config_save_phase = 0;
                        #[cfg(feature = "mspv2_configurator")]
                        let rpc_notified = finish_config_rpc_error(
                            flash_rpc_response_producer,
                            config_rpc_completion,
                            mspv2::rpc::DeviceError::ChecksumMismatch,
                        );
                        #[cfg(not(feature = "mspv2_configurator"))]
                        let rpc_notified = false;
                        if !rpc_notified {
                            queue_storage_response(
                                flash_responses,
                                "ERR config persistence verification failed\r\n",
                            );
                        }
                        continue;
                    }
                    *stored_config = *config_save_candidate;
                    *config_active_slot = *config_save_slot;
                    *config_sequence = config_sequence.wrapping_add(1);
                    *config_save_phase = 0;
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
                _ => {}
            }

            if let Some((command, link)) = flash_commands.take_next() {
                flash_responses.answer(link);
                let mut response = heapless::String::<{ flash_task::USB_RESPONSE_CAPACITY }>::new();
                let maintenance_busy = erase_sector_index.is_some()
                    || *flash_test_phase != 0
                    || *config_save_phase != 0
                    || sync_mark.is_some();
                let unfinished_flight = assembler.unfinished_flight(pending_page.is_some());
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
                        } else if erase_sector_index.is_some()
                            || *flash_test_phase != 0
                            || *config_save_phase != 0
                            || sync_mark.is_some()
                        {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            *flash_test_phase = 1;
                            flash_responses.start_maintenance();
                            queue_storage_response(
                                flash_responses,
                                "OK flash scratch test started\r\n",
                            );
                        }
                    }
                    flash_task::StorageCommand::LogsList => {
                        if let Some(response) = flash_task::format_log_info_response(
                            *next_page_index,
                            *next_flight_id,
                            layout.log_page_count,
                            *log_region_writable,
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
                        } else if page_index >= *next_page_index {
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
                        } else if erase_sector_index.is_some()
                            || *flash_test_phase != 0
                            || *config_save_phase != 0
                            || sync_mark.is_some()
                        {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            *erase_sector_index = Some(0);
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
                                *next_page_index,
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
                                *next_page_index,
                                unfinished_flight,
                                flight,
                                pages,
                            ) {
                                Ok(Ok(())) => {
                                    *sync_mark = Some(flight);
                                    *sync_mark_steps = 0;
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
                        } else if !*log_region_writable && *next_page_index < layout.log_page_count
                        {
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
                                *next_page_index,
                                unfinished_flight,
                            ) {
                                Ok(true) => {
                                    *erase_sector_index = Some(0);
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
                        } else if erase_sector_index.is_some()
                            || *flash_test_phase != 0
                            || *config_save_phase != 0
                            || sync_mark.is_some()
                        {
                            queue_storage_response(
                                flash_responses,
                                "ERR flash maintenance already active\r\n",
                            );
                        } else {
                            let next_sequence = config_sequence.wrapping_add(1);
                            match ferrowasp_core::blackbox::encode_config_page(
                                next_sequence,
                                &stored_config.encode(),
                            ) {
                                Ok(page) => {
                                    *config_save_candidate = *stored_config;
                                    *config_save_page = page;
                                    *config_save_slot = 1 - *config_active_slot;
                                    *config_save_phase = 1;
                                    flash_responses.start_maintenance();
                                    queue_storage_response(
                                        flash_responses,
                                        "OK config save started\r\n",
                                    );
                                }
                                Err(_) => {
                                    queue_storage_response(
                                        flash_responses,
                                        "ERR config encoding failed\r\n",
                                    );
                                }
                            }
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
                let maintenance_busy = erase_sector_index.is_some()
                    || *flash_test_phase != 0
                    || *config_save_phase != 0
                    || sync_mark.is_some();
                let active_blackbox_id = assembler
                    .recording()
                    .then(|| next_flight_id.wrapping_sub(1).max(1));
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
                                let next_sequence = config_sequence.wrapping_add(1);
                                match ferrowasp_core::blackbox::encode_config_page(
                                    next_sequence,
                                    &candidate.encode(),
                                ) {
                                    Ok(page) => {
                                        *config_save_candidate = candidate;
                                        *config_save_page = page;
                                        *config_save_slot = 1 - *config_active_slot;
                                        *config_save_phase = 1;
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
                            let candidate = CONFIG::DEFAULT_STORED_CONFIG;
                            let next_sequence = config_sequence.wrapping_add(1);
                            match ferrowasp_core::blackbox::encode_config_page(
                                next_sequence,
                                &candidate.encode(),
                            ) {
                                Ok(page) => {
                                    *config_save_candidate = candidate;
                                    *config_save_page = page;
                                    *config_save_slot = 1 - *config_active_slot;
                                    *config_save_phase = 1;
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
                                *next_page_index,
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
                                *next_page_index,
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
                                *next_page_index,
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
            if *log_region_writable && pending_page.is_none() {
                *pending_page = assembler.take_ready_page();
            }
            if let Some(page) = pending_page.as_ref() {
                let Some(address) = layout.log_page_address(*next_page_index) else {
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage blackbox region full; recording stopped");
                    continue;
                };
                if flash_device.page_program(address, page).is_err() {
                    FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                    FLASH_READY.store(false, Ordering::Release);
                    warn!("Storage page program failed; recording stopped");
                    continue;
                }
                *pending_page = None;
                *next_page_index = next_page_index.saturating_add(1);
                FLASH_PAGES_WRITTEN.fetch_add(1, Ordering::Relaxed);
                // Fall through to the record drain rather than yielding
                // here. Yielding meant a pass either wrote a page or
                // consumed records, never both, so the store alternated
                // between the two and halved its own throughput. The loop
                // still yields once at the end of every pass.
            }
            if !*log_region_writable {
                if flash_record_consumer.dequeue().is_some() {
                    FLASH_RECORDS_DROPPED.fetch_add(1, Ordering::Relaxed);
                }
            } else if flash_record_consumer.peek().is_some() {
                // Drain a bounded batch rather than one record per pass.
                //
                // This loop ends in a one millisecond yield, so consuming a
                // single record per pass capped the store near one record
                // per millisecond however fast the flash is - measured at
                // 84 pages/s, where the part itself programs a page in well
                // under a millisecond. At 400 Hz that left about five
                // percent of margin nobody had measured, and any higher
                // rate silently dropped most of the log.
                //
                // The bound keeps this task from starving the rest of the
                // system on a full queue.
                let mut drained = 0;
                while drained < FLASH_RECORD_DRAIN_PER_PASS {
                    // Write a finished page before taking another record.
                    // The assembler holds one page, so a record taken while
                    // it is full was dropped: with a page written only once
                    // a pass, a full-rate log lost about a third of its
                    // records that way. A store that programs in the
                    // background (SPI NOR) takes the next page once it is
                    // idle, and the records wait queued until then; in an SD
                    // card's window a page write is a RAM copy, so a pass can
                    // write several. A failed write stays pending for the
                    // check above to report.
                    if pending_page.is_none() {
                        *pending_page = assembler.take_ready_page();
                    }
                    if let Some(page) = pending_page.as_ref() {
                        if flash_device.busy().unwrap_or(true) {
                            break;
                        }
                        let Some(address) = layout.log_page_address(*next_page_index) else {
                            break;
                        };
                        if flash_device.page_program(address, page).is_err() {
                            break;
                        }
                        *pending_page = None;
                        *next_page_index = next_page_index.saturating_add(1);
                        FLASH_PAGES_WRITTEN.fetch_add(1, Ordering::Relaxed);
                    }
                    let Some(record) = flash_record_consumer.dequeue() else {
                        break;
                    };
                    drained += 1;
                    // Bench-only: record while disarmed so the flash write
                    // path can be exercised without flying. The record's own
                    // armed flag stays honest, so a capture made this way is
                    // still identifiable as ground data.
                    #[cfg(feature = "bench_blackbox")]
                    let armed = true;
                    #[cfg(not(feature = "bench_blackbox"))]
                    let armed = record.flags & 1 != 0;
                    if armed && !assembler.recording() {
                        assembler.start(*next_flight_id, *boot_session_start_pending);
                        *boot_session_start_pending = false;
                        *next_flight_id = next_flight_id.wrapping_add(1).max(1);
                    }
                    if armed {
                        if assembler.push(record).is_err() {
                            FLASH_RECORDS_DROPPED.fetch_add(1, Ordering::Relaxed);
                        }
                    } else if assembler.recording() && assembler.stop().is_err() {
                        FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
                    }
                }
            } else if !cfg!(feature = "bench_blackbox")
                // A bench image records disarmed throughout; ending its
                // flight whenever the queue ran dry split one run into
                // hundreds of tiny flights.
                && !SAFETY_ARMED.load(Ordering::Acquire)
                && assembler.recording()
                && assembler.stop().is_err()
            {
                FLASH_WRITE_FAULTS.fetch_add(1, Ordering::Relaxed);
            }
        }

        Mono::delay(1u64.millis()).await;
    }
}
