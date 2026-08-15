use core::fmt::Write as _;

use ferrowasp_tasks::{
    drone_toolbox as dt,
    flash_storage::{
        CommandConsumer, GoldenFlashOperation, GoldenFlashState, RecordConsumer, ResponseFrame,
        ResponseProducer, StorageCommand, StorageLayout, emit_page_hex_lines,
        format_log_info_response, load_config, scan_log,
    },
    service_telemetry::{FlightServiceTelemetry, StorageStatus},
};
use fugit::MillisDurationU32;

use crate::{hardware_definitions::stm32f4::golden_service_authoring::Spi2Flash, rtic::task::Mono};

crate::reusable_task! {
    contract {
        local {
            /// Sole CPU-serviced owner of the SPI2 NOR device.
            flash: Spi2Flash,
            /// Bounded control-to-storage blackbox record consumer.
            records: RecordConsumer,
            /// Bounded USB-to-storage whitelisted command consumer.
            commands: CommandConsumer,
            /// Bounded storage-to-USB response producer.
            responses: ResponseProducer,
            /// Persistent recovery, assembly, and maintenance state.
            state: GoldenFlashState,
        }
        shared {
            /// Safety-owned arming observation used only to deny maintenance.
            telemetry: FlightServiceTelemetry,
            /// Runtime tuning applied by control and the disarmed OSD menu.
            tuning: dt::TuningProfile,
            /// Runtime tuning publication sequence.
            tuning_seq: u32,
            /// Bounded blackbox logging divisor.
            log_divisor: u32,
            /// Observation-only storage identity and health.
            status: StorageStatus,
        }
        config {
            /// Delay between bounded flash-owner service operations.
            interval: MillisDurationU32,
        }
        spawns {}
    }

    /// Owns bounded append-only blackbox and copy-on-write configuration storage.
    pub async fn golden_flash(mut cx: golden_flash::Context<'_>) {
        loop {
            let storage_status = cx.shared.status.lock(|status| *status);
            let mut queue_response = |text: &str| {
                ResponseFrame::from_text(text)
                    .is_some_and(|frame| cx.local.responses.enqueue(frame).is_ok())
            };

            if !storage_status.ready {
                if cx.local.commands.dequeue().is_some() {
                    let _ = queue_response("ERR storage unavailable\r\n");
                }
                Mono::delay(cx.config.interval).await;
                continue;
            }
            let Some(layout) = StorageLayout::new(storage_status.capacity_bytes) else {
                cx.shared.status.lock(|status| {
                    status.ready = false;
                    status.faults = status.faults.saturating_add(1);
                });
                Mono::delay(cx.config.interval).await;
                continue;
            };

            if !cx.local.state.initialized {
                let recovered_log = scan_log(layout, |address, page| {
                    cx.local.flash.read(address, page)
                });
                let recovered_config = load_config(
                    layout,
                    ferrowasp_tasks::flash_storage::StoredConfig::foxeer_f405_v2_default(),
                    |address, page| cx.local.flash.read(address, page),
                );
                match (recovered_log, recovered_config) {
                    (Ok((next_page, next_flight, writable)), Ok((config, sequence, slot))) => {
                        cx.local.state.layout = Some(layout);
                        cx.local.state.next_page = next_page;
                        cx.local.state.next_flight = next_flight;
                        cx.local.state.log_writable = writable;
                        cx.local.state.config = config;
                        cx.local.state.config_sequence = sequence;
                        cx.local.state.config_active_slot = slot;
                        cx.local.state.initialized = true;
                        cx.shared.tuning.lock(|profile| *profile = config.tuning);
                        cx.shared.log_divisor.lock(|divisor| {
                            *divisor = u32::from(config.log_rate_divisor).clamp(1, 16)
                        });
                        cx.shared
                            .tuning_seq
                            .lock(|seq| *seq = seq.wrapping_add(1).max(1));
                        cx.shared.status.lock(|status| {
                            status.next_page = next_page;
                            status.next_flight = next_flight;
                        });
                    }
                    _ => {
                        cx.shared.status.lock(|status| {
                            status.ready = false;
                            status.faults = status.faults.saturating_add(1);
                        });
                        defmt::warn!("SPI2 flash recovery failed; storage disabled");
                    }
                }
                Mono::delay(cx.config.interval).await;
                continue;
            }

            let status = match cx.local.flash.read_status() {
                Ok(status) => status,
                Err(_) => {
                    cx.shared.status.lock(|status| {
                        status.ready = false;
                        status.faults = status.faults.saturating_add(1);
                    });
                    defmt::warn!("SPI2 flash status read failed; storage disabled");
                    Mono::delay(cx.config.interval).await;
                    continue;
                }
            };
            if status.busy() {
                Mono::delay(cx.config.interval).await;
                continue;
            }

            let armed = cx.shared.telemetry.lock(|telemetry| telemetry.armed);
            if armed && cx.local.state.maintenance_busy() {
                cx.local.state.operation = GoldenFlashOperation::Idle;
                let _ = queue_response("ERR maintenance aborted because system armed\r\n");
            }

            match cx.local.state.operation {
                GoldenFlashOperation::EraseLogs { next_sector } => {
                    if next_sector >= layout.log_sector_count() {
                        cx.local.state.operation = GoldenFlashOperation::Idle;
                        cx.local.state.next_page = 0;
                        cx.local.state.next_flight = 1;
                        cx.local.state.log_writable = true;
                        cx.shared.status.lock(|status| {
                            status.next_page = 0;
                            status.next_flight = 1;
                        });
                        let _ = queue_response("OK logs erased\r\n");
                    } else {
                        let address = layout.log_start_address
                            + next_sector * ferrowasp_tasks::flash_storage::CONFIG_SECTOR_SIZE;
                        if cx.local.flash.erase_sector_4k(address).is_ok() {
                            cx.local.state.operation = GoldenFlashOperation::EraseLogs {
                                next_sector: next_sector + 1,
                            };
                        } else {
                            cx.local.state.operation = GoldenFlashOperation::Idle;
                            cx.shared
                                .status
                                .lock(|status| status.faults = status.faults.saturating_add(1));
                            let _ = queue_response("ERR log sector erase failed\r\n");
                        }
                    }
                    Mono::delay(cx.config.interval).await;
                    continue;
                }
                GoldenFlashOperation::SaveConfigErase { slot } => {
                    let address = layout.config_slot_addresses[slot as usize];
                    if cx
                        .local
                        .flash
                        .page_program(address, &cx.local.state.pending_config_page)
                        .is_ok()
                    {
                        cx.local.state.operation = GoldenFlashOperation::SaveConfigProgram { slot };
                    } else {
                        cx.local.state.operation = GoldenFlashOperation::Idle;
                        cx.shared
                            .status
                            .lock(|status| status.faults = status.faults.saturating_add(1));
                        let _ = queue_response("ERR config page program failed\r\n");
                    }
                    Mono::delay(cx.config.interval).await;
                    continue;
                }
                GoldenFlashOperation::SaveConfigProgram { slot } => {
                    let mut persisted = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                    let address = layout.config_slot_addresses[slot as usize];
                    let expected_sequence = cx.local.state.config_sequence.wrapping_add(1);
                    let expected = cx.local.state.config.encode();
                    let verified = cx.local.flash.read(address, &mut persisted).is_ok()
                        && matches!(
                            ferrowasp_core::blackbox::decode_config_page(&persisted),
                            Ok((sequence, payload))
                                if sequence == expected_sequence && payload == expected.as_slice()
                        );
                    cx.local.state.operation = GoldenFlashOperation::Idle;
                    if verified {
                        cx.local.state.config_sequence = expected_sequence;
                        cx.local.state.config_active_slot = slot;
                        cx.shared.tuning.lock(|profile| *profile = cx.local.state.config.tuning);
                        cx.shared.log_divisor.lock(|divisor| {
                            *divisor = u32::from(cx.local.state.config.log_rate_divisor).clamp(1, 16)
                        });
                        cx.shared
                            .tuning_seq
                            .lock(|seq| *seq = seq.wrapping_add(1).max(1));
                        let _ = queue_response("OK config saved\r\n");
                    } else {
                        cx.shared
                            .status
                            .lock(|status| status.faults = status.faults.saturating_add(1));
                        let _ = queue_response("ERR config persistence verification failed\r\n");
                    }
                    Mono::delay(cx.config.interval).await;
                    continue;
                }
                GoldenFlashOperation::Idle => {}
            }

            if let Some(command) = cx.local.commands.dequeue() {
                let mut response = heapless::String::<
                    { ferrowasp_tasks::flash_storage::USB_RESPONSE_CAPACITY },
                >::new();
                match command {
                    StorageCommand::Help => {
                        let _ = queue_response(
                            "OK flash info | logs list/read-page/erase | config get/set/save\r\n",
                        );
                    }
                    StorageCommand::FlashInfo => {
                        let _ = write!(
                            response,
                            "OK jedec={:02x}:{:02x}:{:02x} bytes={} ready=1\r\n",
                            storage_status.jedec[0],
                            storage_status.jedec[1],
                            storage_status.jedec[2],
                            layout.capacity_bytes,
                        );
                        let _ = queue_response(response.as_str());
                    }
                    StorageCommand::FlashTestConfirmed => {
                        let _ = queue_response("ERR scratch test unavailable in generated app\r\n");
                    }
                    StorageCommand::LogsList => {
                        if let Some(line) = format_log_info_response(
                            cx.local.state.next_page,
                            cx.local.state.next_flight,
                            layout.log_page_count,
                            cx.local.state.log_writable,
                        ) {
                            let _ = queue_response(line.as_str());
                        }
                    }
                    StorageCommand::LogsReadPage(page) => {
                        if armed {
                            let _ = queue_response("ERR log reads disabled while armed\r\n");
                        } else if page >= cx.local.state.next_page {
                            let _ = queue_response("ERR log page is not present\r\n");
                        } else if let Some(address) = layout.log_page_address(page) {
                            let mut bytes = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
                            if cx.local.flash.read(address, &mut bytes).is_ok() {
                                let mut frames = [None; 16];
                                let mut frame_count = 0_usize;
                                let emitted = emit_page_hex_lines(page, &bytes, |line| {
                                    let Some(frame) = ResponseFrame::from_text(line) else {
                                        return false;
                                    };
                                    if frame_count >= frames.len() {
                                        return false;
                                    }
                                    frames[frame_count] = Some(frame);
                                    frame_count += 1;
                                    true
                                });
                                drop(queue_response);
                                let queued = emitted
                                    && frames[..frame_count]
                                        .iter()
                                        .flatten()
                                        .all(|frame| cx.local.responses.enqueue(*frame).is_ok());
                                if !queued {
                                    if let Some(frame) =
                                        ResponseFrame::from_text("ERR log response queue full\r\n")
                                    {
                                        let _ = cx.local.responses.enqueue(frame);
                                    }
                                }
                            } else {
                                let _ = queue_response("ERR log page read failed\r\n");
                            }
                        }
                    }
                    StorageCommand::LogsEraseConfirmed => {
                        if armed {
                            let _ = queue_response("ERR log erase disabled while armed\r\n");
                        } else {
                            cx.local.state.operation =
                                GoldenFlashOperation::EraseLogs { next_sector: 0 };
                            let _ = queue_response("OK log erase started\r\n");
                        }
                    }
                    StorageCommand::ConfigGet(key) => {
                        let _ = write!(
                            response,
                            "OK {}={:.4}\r\n",
                            key.name(),
                            cx.local.state.config.get(key),
                        );
                        let _ = queue_response(response.as_str());
                    }
                    StorageCommand::ConfigSet(key, value) => {
                        if armed {
                            let _ = queue_response("ERR config changes disabled while armed\r\n");
                        } else if cx.local.state.config.set(key, value) {
                            let _ = queue_response("OK staged; use config save\r\n");
                        } else {
                            let _ = queue_response("ERR value outside allowed range\r\n");
                        }
                    }
                    StorageCommand::ConfigSave => {
                        if armed {
                            let _ = queue_response("ERR config save disabled while armed\r\n");
                        } else {
                            let sequence = cx.local.state.config_sequence.wrapping_add(1);
                            match ferrowasp_core::blackbox::encode_config_page(
                                sequence,
                                &cx.local.state.config.encode(),
                            ) {
                                Ok(page) => {
                                    let slot = 1 - cx.local.state.config_active_slot;
                                    cx.local.state.pending_config_page = page;
                                    if cx
                                        .local
                                        .flash
                                        .erase_sector_4k(layout.config_slot_addresses[slot as usize])
                                        .is_ok()
                                    {
                                        cx.local.state.operation =
                                            GoldenFlashOperation::SaveConfigErase { slot };
                                        let _ = queue_response("OK config save started\r\n");
                                    } else {
                                        let _ = queue_response("ERR config slot erase failed\r\n");
                                    }
                                }
                                Err(_) => {
                                    let _ = queue_response("ERR config encoding failed\r\n");
                                }
                            }
                        }
                    }
                }
            }

            if let Some(record) = cx.local.records.dequeue() {
                if armed && !cx.local.state.assembler.recording() {
                    cx.local.state.assembler.start(
                        cx.local.state.next_flight,
                        cx.local.state.boot_session_start_pending,
                    );
                    cx.local.state.next_flight = cx.local.state.next_flight.wrapping_add(1).max(1);
                    cx.local.state.boot_session_start_pending = false;
                }
                if armed
                    && cx.local.state.pending_log_page.is_none()
                    && matches!(cx.local.state.assembler.push(record), Ok(true))
                {
                    cx.local.state.pending_log_page = cx.local.state.assembler.take_ready_page();
                }
            }
            if !armed && cx.local.state.assembler.recording() {
                if matches!(cx.local.state.assembler.stop(), Ok(true)) {
                    cx.local.state.pending_log_page = cx.local.state.assembler.take_ready_page();
                }
            }

            if let Some(page) = cx.local.state.pending_log_page
                && cx.local.state.log_writable
                && let Some(address) = layout.log_page_address(cx.local.state.next_page)
            {
                if cx.local.flash.page_program(address, &page).is_ok() {
                    cx.local.state.pending_log_page = None;
                    cx.local.state.next_page = cx.local.state.next_page.saturating_add(1);
                    if cx.local.state.next_page >= layout.log_page_count {
                        cx.local.state.log_writable = false;
                    }
                    cx.shared.status.lock(|status| {
                        status.next_page = cx.local.state.next_page;
                        status.next_flight = cx.local.state.next_flight;
                    });
                } else {
                    cx.local.state.log_writable = false;
                    cx.shared
                        .status
                        .lock(|status| status.faults = status.faults.saturating_add(1));
                }
            }

            Mono::delay(cx.config.interval).await;
        }
    }
}
