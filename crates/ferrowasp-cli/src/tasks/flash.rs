//! The storage owner: the flight log, saving the configuration, log sync and
//! the storage commands from USB, the configurator link and MSPv2. One task
//! for every board, over any store with NOR semantics; a board supplies its
//! store and its defaults. Init chooses the boot's configuration
//! ([`boot_config`]); the coordinator owns it from then on, so a change is a
//! request this task makes for both command lines, and `config save` writes
//! the next boot's configuration as the ledger shows it.
//!
//! Requirements: REBOOT-001, REBOOT-004

use super::prelude::*;
use crate::command_link::{CommandLink, CommandSources, ResponseRouter};
#[cfg(feature = "mspv2_configurator")]
use crate::commands::StoredConfigRpc;
#[cfg(feature = "mspv2_configurator")]
use crate::rpc_commands::{self, RpcApply, RpcContext};
use crate::storage_commands::{self, CommandAction, CommandContext};
use core::fmt::Write as _;
use ferrowasp_blackbox::log_writer::{LogWriter, PageWrite};
use ferrowasp_blackbox::storage_maintenance::{
    Maintenance, MaintenanceDone, MaintenanceFailure, MaintenanceStep,
};
use ferrowasp_boundary_contracts::requests::{Outcome, RebootTarget, Refusal, Request, Requester};
use ferrowasp_config::boot::{BootConfig, read_slot, select};
use ferrowasp_io_contracts::link::Pushed;
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2 as mspv2;
use ferrowasp_serial::{SerialBindings, SerialRoute};
use ferrowasp_storage::config_save::{ConfigSave, SaveFailure, SaveStep};
use ferrowasp_storage::nor_storage::NorStorage;

/// How long a reboot waits for the store to finish its last program, in
/// milliseconds. With maintenance refused, that is one page program, a few
/// milliseconds on SPI NOR; a 4 KiB sector erase takes at most 400.
pub const REBOOT_STORAGE_SETTLE_MS: u32 = 500;

/// How long a reboot waits after answering, so the answer leaves over USB
/// before the port goes away.
pub const REBOOT_ANSWER_MS: u64 = 100;

/// How long a reboot waits for the safety master's decision, ten
/// coordinator runs.
pub const REBOOT_DECISION_MS: u64 = 100;

/// How long a configuration change waits for its result: a whole
/// transaction and ten coordinator runs.
pub const CONFIG_DECISION_MS: u64 = ferrowasp_border::coordinator::CONFIG_TRANSACTION_MS + 100;

/// How long a calibration waits for its result: the grant, the sampling
/// window, the change that applies the level, and ten coordinator runs.
pub const CALIBRATION_DECISION_MS: u64 = 2 * ferrowasp_border::coordinator::CONFIG_TRANSACTION_MS
    + ferrowasp_border::coordinator::CALIBRATION_SAMPLING_MS
    + 100;

/// A commit or reset MSPv2 left for this task to carry out.
#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcApply = Option<RpcApply>;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcApply = ();
#[cfg(feature = "mspv2_configurator")]
const NO_RPC_APPLY: FlashRpcApply = None;
#[cfg(not(feature = "mspv2_configurator"))]
const NO_RPC_APPLY: FlashRpcApply = ();

pub use ferrowasp_storage::config_save::ConfigRpcCompletion;

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
/// The board's answer to the configurator's `Hello`, given the arm state.
pub type RpcDeviceInfoFn = fn(bool) -> mspv2::rpc::DeviceInfo;
#[cfg(not(feature = "mspv2_configurator"))]
pub type RpcDeviceInfoFn = ();

pub fn scan_flash_log<F: NorStorage>(
    flash: &mut F,
    layout: flash_task::StorageLayout,
) -> Result<(u32, u32, bool), flash_task::StorageReadError<F::Error>> {
    flash_task::scan_log(layout, |address, page| flash.read(address, page))
}

/// The configuration the board boots with, read from both slots at init;
/// `default` is the board's for a store that holds none. A store that is
/// missing or cannot be read boots `default` as a fallback, which the pilot
/// acknowledges before arming.
pub fn boot_config<F: NorStorage>(
    flash: &mut F,
    probe: &FlashProbe,
    default: flash_task::StoredConfig,
) -> BootConfig {
    let layout = probe
        .ready
        .then(|| flash_task::StorageLayout::new(probe.capacity_bytes))
        .flatten();
    let Some(layout) = layout else {
        return BootConfig::unreadable(default);
    };
    let mut slots = [ferrowasp_config::boot::SlotRead::Erased; 2];
    for (slot, address) in slots.iter_mut().zip(layout.config_slot_addresses) {
        let mut page = [0xff; ferrowasp_config::page::FLASH_PAGE_LEN];
        if flash.read(address, &mut page).is_err() {
            return BootConfig::unreadable(default);
        }
        *slot = read_slot(&page);
    }
    select(slots, default)
}

/// Ask the coordinator, as the storage owner, and wait for the result on the
/// ledger; `None` when the link refused it or no result came in
/// `timeout_ms`. `delay` waits one millisecond.
async fn ask<Wait: core::future::Future<Output = ()>>(
    link: &mut LinkProducer<'static, Request>,
    ledger: &mut impl rtic::Mutex<T = Ledger>,
    at: TimestampMicros,
    request: Request,
    timeout_ms: u64,
    mut delay: impl FnMut() -> Wait,
) -> Option<Outcome> {
    if link.push(at, request) == Pushed::Refused {
        return None;
    }
    let request_seq = link.sequence();
    for _ in 0..timeout_ms {
        let result = ledger.lock(|ledger| ledger.result(Requester::Storage));
        if result.seq != 0 && result.value.request_seq == request_seq {
            return Some(result.value.outcome);
        }
        delay().await;
    }
    None
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
        responses.wake_usb();
    }
    true
}

#[cfg(feature = "mspv2_configurator")]
pub fn queue_rpc_response(
    wake_usb: fn(),
    producer: &mut flash_task::RpcResponseProducer,
    response: mspv2::rpc::RpcResponse,
) -> bool {
    if producer.enqueue(response).is_err() {
        return false;
    }
    wake_usb();
    true
}

#[cfg(feature = "mspv2_configurator")]
pub fn finish_config_rpc_error(
    wake_usb: fn(),
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
    queue_rpc_response(wake_usb, producer, mspv2::rpc::error(request_id, error))
}

/// What init found on the storage bus: whether the store is supported, its
/// JEDEC manufacturer, memory type and capacity code, and its capacity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlashProbe {
    pub ready: bool,
    pub jedec: [u8; 3],
    pub capacity_bytes: u32,
}

/// Own the store: recover the log and configuration, write the flight log,
/// and answer every storage command, one bounded pass a millisecond. Lowest
/// priority: it never preempts flight work.
#[ferroforge::task(
    bounds = [flash_device: NorStorage],
    local = [
        flash_device,
        flash_record_producer: flash_task::RecordProducer,
        flash_record_consumer: flash_task::RecordConsumer,
        blackbox_stream: LinkConsumer<'static, observed::ControlTick>,
        storage_status_link: LinkProducer<'static, observed::StorageStatus>,
        // A gate: storage writes only while this reads disarmed.
        flash_safety_arm_reader: signals::SafetyArmReader,
        // What init found on the bus; `ready` drops when storage fails.
        flash_probe: FlashProbe,
        log_rate_divisor: u32 = 1,
        pages_written: u32 = 0,
        write_faults: u32 = 0,
        records_dropped: u32 = 0,
        flash_commands: CommandSources,
        flash_responses: ResponseRouter,
        flash_rpc_command_consumer: FlashRpcCommandConsumer,
        flash_rpc_response_producer: FlashRpcResponseProducer,
        flash_rpc_device_info: RpcDeviceInfoFn,
        log: LogWriter = LogWriter::new(),
        initialized: bool = false,
        // The saved configuration's sequence and slot, from init's choice.
        config_sequence: u32,
        config_active_slot: u8,
        maintenance: Maintenance = Maintenance::Idle,
        config_save: ConfigSave = ConfigSave::idle(),
        staged_rpc_config: Option<flash_task::StoredConfig> = None,
        config_rpc_completion: Option<ConfigRpcCompletion> = None,
        rpc_apply: FlashRpcApply = NO_RPC_APPLY,
        // The link to the coordinator: configuration changes for both
        // command lines, and a reboot once the store is settled.
        storage_request_link: LinkProducer<'static, Request>,
    ],
    shared = [ledger: Ledger],
    config = [
        wake_usb: fn(),
        default_serial_bindings: SerialBindings,
        serial_routes: &'static [SerialRoute],
    ],
    monotonic = Mono,
)]
pub async fn flash_manager_task(mut cx: flash_manager_task::Context) {
    let flash_device = cx.local.flash_device;
    let flash_record_producer = cx.local.flash_record_producer;
    let flash_record_consumer = cx.local.flash_record_consumer;
    let blackbox_stream = cx.local.blackbox_stream;
    let storage_status_link = cx.local.storage_status_link;
    let flash_safety_arm_reader = cx.local.flash_safety_arm_reader;
    let flash_probe = cx.local.flash_probe;
    let log_rate_divisor = cx.local.log_rate_divisor;
    let pages_written = cx.local.pages_written;
    let write_faults = cx.local.write_faults;
    let records_dropped = cx.local.records_dropped;
    let flash_commands = cx.local.flash_commands;
    let flash_responses = cx.local.flash_responses;
    let flash_rpc_command_consumer = cx.local.flash_rpc_command_consumer;
    let flash_rpc_response_producer = cx.local.flash_rpc_response_producer;
    let flash_rpc_device_info = cx.local.flash_rpc_device_info;
    let log = cx.local.log;
    let initialized = cx.local.initialized;
    let config_sequence = cx.local.config_sequence;
    let config_active_slot = cx.local.config_active_slot;
    let maintenance = cx.local.maintenance;
    let config_save = cx.local.config_save;
    let staged_rpc_config = cx.local.staged_rpc_config;
    let config_rpc_completion = cx.local.config_rpc_completion;
    let rpc_apply = cx.local.rpc_apply;
    let storage_request_link = cx.local.storage_request_link;
    #[cfg(not(feature = "mspv2_configurator"))]
    let _ = (
        flash_rpc_command_consumer,
        flash_rpc_response_producer,
        flash_rpc_device_info,
        staged_rpc_config,
        config_rpc_completion,
        rpc_apply,
    );
    let at = || TimestampMicros(Mono::now().duration_since_epoch().to_micros());
    loop {
        *log_rate_divisor = cx
            .shared
            .ledger
            .lock(|ledger| u32::from(ledger.config().value.active.log_rate_divisor));
        // Logged control ticks from the observer's stream become records,
        // as many as the staging queue takes, at the stored log rate; the
        // rest wait in the stream, which rides out a storage stall.
        while flash_record_producer.ready() {
            let Some(tick) = blackbox_stream.oldest() else {
                break;
            };
            if let Some(record) = flash_task::record_for_tick(&tick, *log_rate_divisor) {
                let _ = flash_record_producer.enqueue(record);
            }
        }
        storage_status_link.push(
            TimestampMicros(Mono::now().duration_since_epoch().to_micros()),
            observed::StorageStatus {
                ready: flash_probe.ready,
                pages_written: *pages_written,
                write_faults: *write_faults,
                records_dropped: *records_dropped,
                log_rate_divisor: *log_rate_divisor as u8,
            },
        );

        // Log pages a store holds back in RAM (an SD card's write-back
        // window) are written out at the top of every pass once they are old
        // enough, or when no flight is recording (`LogWriter`).
        let now_ms = Mono::now().duration_since_epoch().to_millis();
        if log.flush_write_back(flash_device, now_ms).is_err() {
            *write_faults = write_faults.wrapping_add(1);
            flash_probe.ready = false;
            warn!("Storage flush failed; recording stopped");
        }
        {
            if !flash_probe.ready {
                #[cfg(feature = "mspv2_configurator")]
                if let Some(request) = flash_rpc_command_consumer.dequeue() {
                    let _ = queue_rpc_response(
                        CONFIG::WAKE_USB,
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
                            flash_probe.jedec[0],
                            flash_probe.jedec[1],
                            flash_probe.jedec[2],
                        ],
                    );
                    queue_storage_response(flash_responses, response.as_str());
                }
                Mono::delay(100u64.millis()).await;
                continue;
            }
            let Some(layout) = flash_task::StorageLayout::new(flash_probe.capacity_bytes) else {
                flash_probe.ready = false;
                #[cfg(feature = "mspv2_configurator")]
                if let Some(request) = flash_rpc_command_consumer.dequeue() {
                    let _ = queue_rpc_response(
                        CONFIG::WAKE_USB,
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
                    flash_probe.ready = false;
                    warn!("Storage log recovery failed; storage disabled");
                    continue;
                };
                log.recover(page, flight, writable);
                *initialized = true;
                info!(
                    "Storage recovered at log page {}, next flight {}, writable {}, config seq {}",
                    page, flight, writable, *config_sequence
                );
            }

            let busy = match flash_device.busy() {
                Ok(busy) => busy,
                Err(_) => {
                    *write_faults = write_faults.wrapping_add(1);
                    flash_probe.ready = false;
                    warn!("Storage status read failed; storage disabled");
                    #[cfg(feature = "mspv2_configurator")]
                    let _ = finish_config_rpc_error(
                        CONFIG::WAKE_USB,
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

            if flash_safety_arm_reader.read()
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
                            flash_probe.ready = false;
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
                    CONFIG::WAKE_USB,
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
                                *write_faults = write_faults.wrapping_add(1);
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
                    *write_faults = write_faults.wrapping_add(1);
                    #[cfg(feature = "mspv2_configurator")]
                    let rpc_notified = finish_config_rpc_error(
                        CONFIG::WAKE_USB,
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
                    *config_active_slot = slot;
                    *config_sequence = sequence;
                    #[cfg(feature = "mspv2_configurator")]
                    let rpc_notified = if let Some(completion) = config_rpc_completion.take() {
                        let crc = config.crc32();
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
                            CONFIG::WAKE_USB,
                            flash_rpc_response_producer,
                            mspv2::rpc::ok(request_id, response),
                        )
                    } else {
                        false
                    };
                    #[cfg(not(feature = "mspv2_configurator"))]
                    let rpc_notified = false;
                    #[cfg(not(feature = "mspv2_configurator"))]
                    let _ = config;
                    if !rpc_notified {
                        queue_storage_response(flash_responses, "OK config saved\r\n");
                    }
                }
            }

            if let Some((command, link)) = flash_commands.take_next() {
                flash_responses.answer(link);
                let config = cx.shared.ledger.lock(|ledger| ledger.config().value);
                let action = storage_commands::handle(
                    command,
                    &mut CommandContext {
                        flash: &mut *flash_device,
                        layout,
                        responses: &mut *flash_responses,
                        link,
                        armed: flash_safety_arm_reader.read(),
                        log: &*log,
                        maintenance: &mut *maintenance,
                        config_save: &mut *config_save,
                        stored_config: config.next_boot,
                        fallback: config.fallback,
                        config_sequence: *config_sequence,
                        config_active_slot: *config_active_slot,
                        jedec: [
                            flash_probe.jedec[0],
                            flash_probe.jedec[1],
                            flash_probe.jedec[2],
                        ],
                        default_serial_bindings: CONFIG::DEFAULT_SERIAL_BINDINGS,
                        serial_routes: CONFIG::SERIAL_ROUTES,
                    },
                );
                match action {
                    CommandAction::None => {}
                    CommandAction::Request(request) => {
                        let outcome = ask(
                            storage_request_link,
                            &mut cx.shared.ledger,
                            at(),
                            request,
                            CONFIG_DECISION_MS,
                            || Mono::delay(1u64.millis()),
                        )
                        .await;
                        queue_storage_response(
                            flash_responses,
                            storage_commands::request_answer(request, outcome),
                        );
                    }
                    // The coordinator calibrates, with arming held off, and
                    // applies the level at once; it is then saved with the
                    // next boot's configuration, like `config save`.
                    CommandAction::StartCalibration => {
                        queue_storage_response(
                            flash_responses,
                            "OK imu calibration started; keep the craft still\r\n",
                        );
                        let request = Request::CalibrateAccel;
                        let outcome = ask(
                            storage_request_link,
                            &mut cx.shared.ledger,
                            at(),
                            request,
                            CALIBRATION_DECISION_MS,
                            || Mono::delay(1u64.millis()),
                        )
                        .await;
                        let saved = match outcome {
                            Some(Outcome::Accepted) => {
                                let next_boot = cx
                                    .shared
                                    .ledger
                                    .lock(|ledger| ledger.config().value.next_boot);
                                if config_save.is_active() || maintenance.is_active() {
                                    Err("ERR flash maintenance already active\r\n")
                                } else {
                                    storage_commands::begin_config_save(
                                        config_save,
                                        next_boot,
                                        *config_sequence,
                                        *config_active_slot,
                                        flash_responses,
                                    )
                                    .map(|()| next_boot.accel_level)
                                }
                            }
                            outcome => Err(storage_commands::request_answer(request, outcome)),
                        };
                        match saved {
                            Ok(level) => {
                                let [x, y, z] = level.offset_mg;
                                let mut line = heapless::String::<
                                    { flash_task::USB_RESPONSE_CAPACITY },
                                >::new();
                                let _ = write!(line, "OK imu calibrated x={x} y={y} z={z} mg\r\n");
                                queue_storage_response(flash_responses, &line);
                            }
                            Err(answer) => {
                                queue_storage_response(flash_responses, answer);
                            }
                        }
                    }
                    // The store is settled before restarting: written-back
                    // pages flushed, and the last program or erase finished.
                    CommandAction::Reboot { rom_bootloader } => {
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
                            // port goes away, then ask the safety master,
                            // through the coordinator, to restart. An
                            // accepted reboot does not come back.
                            Mono::delay(REBOOT_ANSWER_MS.millis()).await;
                            let target = if rom_bootloader {
                                RebootTarget::RomBootloader
                            } else {
                                RebootTarget::Firmware
                            };
                            let outcome = ask(
                                storage_request_link,
                                &mut cx.shared.ledger,
                                at(),
                                Request::Reboot(target),
                                REBOOT_DECISION_MS,
                                || Mono::delay(1u64.millis()),
                            )
                            .await;
                            queue_storage_response(
                                flash_responses,
                                match outcome {
                                    Some(Outcome::Refused(Refusal::Armed)) => {
                                        "ERR reboot cancelled; system armed\r\n"
                                    }
                                    Some(Outcome::Refused(Refusal::ArmingInProgress)) => {
                                        "ERR reboot cancelled; arming in progress\r\n"
                                    }
                                    Some(_) => "ERR reboot refused\r\n",
                                    None => "ERR reboot no answer\r\n",
                                },
                            );
                        }
                    }
                }
            }

            #[cfg(feature = "mspv2_configurator")]
            if let Some(request) = flash_rpc_command_consumer.dequeue() {
                let config = cx.shared.ledger.lock(|ledger| ledger.config().value);
                let response = rpc_commands::handle(
                    request,
                    &mut RpcContext {
                        flash: &mut *flash_device,
                        layout,
                        armed: flash_safety_arm_reader.read(),
                        log: &*log,
                        maintenance: &*maintenance,
                        config_save: &mut *config_save,
                        active_config: config.active,
                        stored_config: config.next_boot,
                        staged: &mut *staged_rpc_config,
                        completion: &mut *config_rpc_completion,
                        apply: &mut *rpc_apply,
                        device_info: *flash_rpc_device_info,
                    },
                );
                if let Some(response) = response {
                    let _ =
                        queue_rpc_response(CONFIG::WAKE_USB, flash_rpc_response_producer, response);
                }
            }
            // A commit or reset: each change through the coordinator, in
            // order, then a save of the next boot's configuration.
            #[cfg(feature = "mspv2_configurator")]
            if let Some(apply) = rpc_apply.take() {
                let next_boot = cx
                    .shared
                    .ledger
                    .lock(|ledger| ledger.config().value.next_boot);
                let requests = match apply.target {
                    Some(target) => rpc_commands::requests_for(
                        &next_boot,
                        &target,
                        CONFIG::DEFAULT_SERIAL_BINDINGS,
                    ),
                    None => {
                        heapless::Vec::from_slice(&[Request::ConfigDefaults]).unwrap_or_default()
                    }
                };
                let mut refused = None;
                for request in requests {
                    let outcome = ask(
                        storage_request_link,
                        &mut cx.shared.ledger,
                        at(),
                        request,
                        CONFIG_DECISION_MS,
                        || Mono::delay(1u64.millis()),
                    )
                    .await;
                    match outcome {
                        Some(Outcome::Accepted | Outcome::Pending) => {}
                        Some(Outcome::Refused(Refusal::Armed | Refusal::ArmingInProgress)) => {
                            refused = Some(mspv2::rpc::DeviceError::Armed);
                            break;
                        }
                        Some(Outcome::Refused(Refusal::Invalid)) => {
                            refused = Some(mspv2::rpc::DeviceError::InvalidConfiguration);
                            break;
                        }
                        Some(Outcome::Refused(Refusal::Busy)) => {
                            refused = Some(mspv2::rpc::DeviceError::Busy);
                            break;
                        }
                        Some(Outcome::Refused(_)) | None => {
                            refused = Some(mspv2::rpc::DeviceError::Internal);
                            break;
                        }
                    }
                }
                *config_rpc_completion = Some(apply.completion);
                let next_boot = cx
                    .shared
                    .ledger
                    .lock(|ledger| ledger.config().value.next_boot);
                let refused = refused.or_else(|| {
                    config_save
                        .begin(next_boot, *config_sequence, *config_active_slot)
                        .err()
                        .map(|_| mspv2::rpc::DeviceError::Internal)
                });
                if let Some(error) = refused {
                    let _ = finish_config_rpc_error(
                        CONFIG::WAKE_USB,
                        flash_rpc_response_producer,
                        config_rpc_completion,
                        error,
                    );
                }
            }
            match log.write_pending(flash_device, layout) {
                PageWrite::Nothing => {}
                PageWrite::Written => {
                    *pages_written = pages_written.wrapping_add(1);
                    // Fall through to the record drain rather than yielding
                    // here: a pass that either wrote a page or consumed
                    // records, never both, halved the store's throughput.
                }
                PageWrite::RegionFull => {
                    flash_probe.ready = false;
                    warn!("Storage blackbox region full; recording stopped");
                    continue;
                }
                PageWrite::ProgramFailed => {
                    *write_faults = write_faults.wrapping_add(1);
                    flash_probe.ready = false;
                    warn!("Storage page program failed; recording stopped");
                    continue;
                }
            }
            let drained = log.drain(
                flash_device,
                layout,
                flash_record_consumer,
                flash_safety_arm_reader.read(),
                // Bench-only: record while disarmed, so the flash write path
                // can be exercised without flying.
                cfg!(feature = "bench_blackbox"),
            );
            *pages_written = pages_written.wrapping_add(drained.pages_written);
            *records_dropped = records_dropped.wrapping_add(drained.records_dropped);
            *write_faults = write_faults.wrapping_add(drained.write_faults);
        }

        Mono::delay(1u64.millis()).await;
    }
}
