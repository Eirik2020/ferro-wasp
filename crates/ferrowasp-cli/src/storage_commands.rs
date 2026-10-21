//! The storage owner's answers to the command line: what each storage command
//! does, refuses, or starts.
//!
//! The owner reads a command off a link, calls [`handle`] with what it owns,
//! and does what the returned [`CommandAction`] asks. Two commands end there:
//! calibrating the accelerometer, which samples the IMU over later passes,
//! and rebooting, which waits for the store to settle.

use core::fmt::Write as _;

use crate::serial_reply::write_serial_bindings;
use ferrowasp_serial::{SerialBindings, SerialRoute};
use ferrowasp_storage::nor_storage::NorStorage;

use crate::command_link::{CommandLink, ResponseRouter, reboot_refusal};
use crate::commands::StorageCommand;
use crate::log_sync;
use ferrowasp_blackbox::log_writer::LogWriter;
use ferrowasp_blackbox::storage_maintenance::Maintenance;
use ferrowasp_config::stored::StoredConfig;
use ferrowasp_storage::config_save::ConfigSave;
use ferrowasp_storage::layout::StorageLayout;

/// What the owner does after [`handle`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandAction {
    None,
    /// Start sampling the IMU for a level calibration; nothing answered yet.
    StartCalibration,
    /// Settle the store and restart; nothing answered yet.
    Reboot {
        rom_bootloader: bool,
    },
}

/// What a command reads and may change.
pub struct CommandContext<'a, F: NorStorage> {
    pub flash: &'a mut F,
    pub layout: StorageLayout,
    /// Already answering `link`.
    pub responses: &'a mut ResponseRouter,
    pub link: CommandLink,
    pub armed: bool,
    pub log: &'a LogWriter,
    pub maintenance: &'a mut Maintenance,
    pub config_save: &'a mut ConfigSave,
    pub stored_config: &'a mut StoredConfig,
    pub config_sequence: u32,
    pub config_active_slot: u8,
    /// An accelerometer calibration is sampling.
    pub calibrating: bool,
    /// Manufacturer, memory type and capacity code, as the flash reported.
    pub jedec: [u8; 3],
    pub default_serial_bindings: SerialBindings,
    pub serial_routes: &'a [SerialRoute],
}

/// Queue one answer line; `false` when the link's queue is full.
pub fn answer(responses: &mut ResponseRouter, text: &str) -> bool {
    crate::commands::ResponseFrame::from_text(text).is_some_and(|frame| responses.send(frame))
}

/// A log page as hex lines.
fn page_hex(
    responses: &mut ResponseRouter,
    page_index: u32,
    page: &[u8; ferrowasp_protocol::blackbox::FLASH_PAGE_LEN],
) -> bool {
    crate::commands::emit_page_hex_lines(page_index, page, |line| answer(responses, line))
}

/// Starts saving `config`, answering the link that asked when it completes.
/// `Err` is the answer line when it cannot start.
pub fn begin_config_save(
    config_save: &mut ConfigSave,
    config: StoredConfig,
    config_sequence: u32,
    config_active_slot: u8,
    responses: &mut ResponseRouter,
) -> Result<(), &'static str> {
    config_save
        .begin(config, config_sequence, config_active_slot)
        .map_err(|_| "ERR config encoding failed\r\n")?;
    responses.start_maintenance();
    Ok(())
}

/// Answers one storage command, or says what the owner must do.
pub fn handle<F: NorStorage>(
    command: StorageCommand,
    ctx: &mut CommandContext<'_, F>,
) -> CommandAction {
    let mut response = heapless::String::<{ crate::commands::USB_RESPONSE_CAPACITY }>::new();
    let maintenance_busy =
        ctx.maintenance.is_active() || ctx.config_save.is_active() || ctx.calibrating;
    let unfinished_flight = ctx.log.unfinished_flight();
    match command {
        // The configurator link refuses these before queueing
        // them; this is the second check.
        command if ctx.link == CommandLink::Serial && command.usb_only() => {
            answer(ctx.responses, "ERR USB only for this command\r\n");
        }
        crate::commands::StorageCommand::Help => {
            answer(
                ctx.responses,
                "OK flash info | flash test CONFIRM | logs list\r\n",
            );
            answer(
                ctx.responses,
                "OK logs erase CONFIRM | config get/set KEY | config save\r\n",
            );
            answer(
                ctx.responses,
                "OK logs unsynced | logs ack FLIGHT PAGES\r\n",
            );
            answer(ctx.responses, "OK logs erase-synced CONFIRM\r\n");
            answer(ctx.responses, "OK serial | serial PORT FUNCTION\r\n");
            answer(
                ctx.responses,
                "OK FUNCTION none/rc/osd/esc_telemetry/configurator\r\n",
            );
            answer(
                ctx.responses,
                "OK live | rc | motor stop | motor spin N CONFIRM\r\n",
            );
            answer(ctx.responses, "OK motor dir N normal|reversed CONFIRM\r\n");
            answer(ctx.responses, "OK esc | build\r\n");
            answer(ctx.responses, "OK reboot | reboot dfu\r\n");
            answer(ctx.responses, "OK imu calibrate | imu level\r\n");
        }
        // Answered by the USB task; never queued here.
        crate::commands::StorageCommand::Live
        | crate::commands::StorageCommand::Rc
        | crate::commands::StorageCommand::Esc
        | crate::commands::StorageCommand::Build
        | crate::commands::StorageCommand::Motor(_) => {}
        crate::commands::StorageCommand::SerialShow => {
            write_serial_bindings(
                ctx.stored_config.serial_bindings,
                ctx.default_serial_bindings,
                ctx.serial_routes,
                |line| {
                    answer(ctx.responses, line);
                },
            );
        }
        crate::commands::StorageCommand::SerialSet(port, function) => {
            if ctx.armed {
                answer(ctx.responses, "ERR config changes disabled while armed\r\n");
            } else {
                let mut bindings = ctx
                    .stored_config
                    .serial_bindings
                    .unwrap_or(ctx.default_serial_bindings);
                bindings.set(port, function);
                ctx.stored_config.serial_bindings = Some(bindings);
                answer(ctx.responses, "OK staged; use config save, then reboot\r\n");
            }
        }
        crate::commands::StorageCommand::FlashInfo => {
            let _ = write!(
                response,
                "OK jedec={:02x}:{:02x}:{:02x} bytes={} ready=1\r\n",
                ctx.jedec[0], ctx.jedec[1], ctx.jedec[2], ctx.layout.capacity_bytes
            );
            answer(ctx.responses, response.as_str());
        }
        crate::commands::StorageCommand::FlashTestConfirmed => {
            if ctx.armed {
                answer(ctx.responses, "ERR flash test disabled while armed\r\n");
            } else if ctx.maintenance.is_active() || ctx.config_save.is_active() || ctx.calibrating
            {
                answer(ctx.responses, "ERR flash maintenance already active\r\n");
            } else {
                *ctx.maintenance = Maintenance::scratch_test();
                ctx.responses.start_maintenance();
                answer(ctx.responses, "OK flash scratch test started\r\n");
            }
        }
        crate::commands::StorageCommand::LogsList => {
            if let Some(response) = crate::commands::format_log_info_response(
                ctx.log.next_page(),
                ctx.log.next_flight(),
                ctx.layout.log_page_count,
                ctx.log.writable(),
            ) {
                answer(ctx.responses, response.as_str());
            } else {
                answer(ctx.responses, "ERR log summary formatting failed\r\n");
            }
        }
        crate::commands::StorageCommand::LogsReadPage(page_index) => {
            if ctx.armed {
                answer(ctx.responses, "ERR log reads disabled while armed\r\n");
            } else if page_index >= ctx.log.next_page() {
                answer(ctx.responses, "ERR log page is not present\r\n");
            } else if let Some(address) = ctx.layout.log_page_address(page_index) {
                let mut page = [0xff; ferrowasp_protocol::blackbox::FLASH_PAGE_LEN];
                if ctx.flash.read(address, &mut page).is_err()
                    || !page_hex(ctx.responses, page_index, &page)
                {
                    answer(ctx.responses, "ERR log page read/response failed\r\n");
                }
            }
        }
        crate::commands::StorageCommand::LogsEraseConfirmed => {
            if ctx.armed {
                answer(ctx.responses, "ERR log erase disabled while armed\r\n");
            } else if ctx.maintenance.is_active() || ctx.config_save.is_active() || ctx.calibrating
            {
                answer(ctx.responses, "ERR flash maintenance already active\r\n");
            } else {
                *ctx.maintenance = Maintenance::erase_log();
                ctx.responses.start_maintenance();
                answer(ctx.responses, "OK log erase started\r\n");
            }
        }
        crate::commands::StorageCommand::LogsUnsynced => {
            if ctx.armed {
                answer(ctx.responses, "ERR log reads disabled while armed\r\n");
            } else {
                let mut read = |address, output: &mut [u8]| ctx.flash.read(address, output);
                match log_sync::unsynced_flights(
                    ctx.layout,
                    &mut read,
                    ctx.log.next_page(),
                    unfinished_flight,
                ) {
                    Ok((flights, more)) => {
                        log_sync::emit_unsynced_lines(&flights, more, |line| {
                            answer(ctx.responses, line)
                        });
                    }
                    Err(_) => {
                        answer(ctx.responses, "ERR log scan failed\r\n");
                    }
                }
            }
        }
        crate::commands::StorageCommand::LogsAck { flight, pages } => {
            if ctx.armed {
                answer(ctx.responses, "ERR log sync disabled while armed\r\n");
            } else if maintenance_busy {
                answer(ctx.responses, "ERR flash maintenance already active\r\n");
            } else {
                let mut read = |address, output: &mut [u8]| ctx.flash.read(address, output);
                match log_sync::check_ack(
                    ctx.layout,
                    &mut read,
                    ctx.log.next_page(),
                    unfinished_flight,
                    flight,
                    pages,
                ) {
                    Ok(Ok(())) => {
                        *ctx.maintenance = Maintenance::sync_mark(flight);
                        ctx.responses.start_maintenance();
                    }
                    Ok(Err(refusal)) => {
                        answer(ctx.responses, refusal.response());
                    }
                    Err(_) => {
                        answer(ctx.responses, "ERR log scan failed\r\n");
                    }
                }
            }
        }
        crate::commands::StorageCommand::LogsEraseSyncedConfirmed => {
            if ctx.armed {
                answer(ctx.responses, "ERR log erase disabled while armed\r\n");
            } else if !ctx.log.writable() && ctx.log.next_page() < ctx.layout.log_page_count {
                // Something other than flights follows the
                // last one, so nothing says it was stored.
                answer(
                    ctx.responses,
                    "ERR log holds data that is not a flight; erase over USB\r\n",
                );
            } else if maintenance_busy {
                answer(ctx.responses, "ERR flash maintenance already active\r\n");
            } else {
                let mut read = |address, output: &mut [u8]| ctx.flash.read(address, output);
                match log_sync::all_synced(
                    ctx.layout,
                    &mut read,
                    ctx.log.next_page(),
                    unfinished_flight,
                ) {
                    Ok(true) => {
                        *ctx.maintenance = Maintenance::erase_log();
                        ctx.responses.start_maintenance();
                        answer(ctx.responses, "OK log erase started\r\n");
                    }
                    Ok(false) => {
                        answer(ctx.responses, "ERR unsynced flights remain\r\n");
                    }
                    Err(_) => {
                        answer(ctx.responses, "ERR log scan failed\r\n");
                    }
                }
            }
        }
        crate::commands::StorageCommand::ConfigGet(key) => {
            let _ = write!(
                response,
                "OK {}={:.4}\r\n",
                key.name(),
                ctx.stored_config.get(key)
            );
            answer(ctx.responses, response.as_str());
        }
        crate::commands::StorageCommand::ConfigSet(key, value) => {
            if ctx.armed {
                answer(ctx.responses, "ERR config changes disabled while armed\r\n");
            } else if ctx.stored_config.set(key, value) {
                answer(ctx.responses, "OK staged; use config save\r\n");
            } else {
                answer(ctx.responses, "ERR value outside allowed range\r\n");
            }
        }
        crate::commands::StorageCommand::ConfigSave => {
            if ctx.armed {
                answer(ctx.responses, "ERR config save disabled while armed\r\n");
            } else if ctx.maintenance.is_active() || ctx.config_save.is_active() || ctx.calibrating
            {
                answer(ctx.responses, "ERR flash maintenance already active\r\n");
            } else {
                let reply = match begin_config_save(
                    ctx.config_save,
                    *ctx.stored_config,
                    ctx.config_sequence,
                    ctx.config_active_slot,
                    ctx.responses,
                ) {
                    Ok(()) => "OK config save started\r\n",
                    Err(answer) => answer,
                };
                answer(ctx.responses, reply);
            }
        }
        crate::commands::StorageCommand::ImuLevel => {
            let [x, y, z] = ctx.stored_config.accel_level.offset_mg;
            let _ = write!(response, "OK imu level x={x} y={y} z={z} mg\r\n");
            answer(ctx.responses, &response);
        }
        crate::commands::StorageCommand::ImuCalibrate => {
            if ctx.armed {
                answer(
                    ctx.responses,
                    "ERR imu calibration disabled while armed\r\n",
                );
            } else if maintenance_busy {
                answer(ctx.responses, "ERR flash maintenance already active\r\n");
            } else {
                return CommandAction::StartCalibration;
            }
        }
        // Restarting cuts the store off: only once disarmed, with no
        // ctx.maintenance or flight log open. The owner then settles
        // the store and resets.
        crate::commands::StorageCommand::Reboot { rom_bootloader } => {
            if let Some(refusal) =
                reboot_refusal(ctx.armed, maintenance_busy, unfinished_flight.is_some())
            {
                answer(ctx.responses, refusal);
            } else {
                return CommandAction::Reboot { rom_bootloader };
            }
        }
    }
    CommandAction::None
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::commands::ResponseConsumer;
    use crate::commands::ResponseQueue;
    use crate::commands::parse_command;
    use ferrowasp_serial::{LogicalSerialPort, SerialCapabilities, SerialFunction, SerialProfile};
    use ferrowasp_storage::config_save::SaveStep;
    use ferrowasp_storage::test_nor::MemoryNor;
    use std::{boxed::Box, string::String, vec::Vec};

    const CAPACITY: u32 = 64 * 1024;

    const ROUTES: [SerialRoute; 1] = [SerialRoute {
        logical: LogicalSerialPort::Uart3,
        peripheral: "test",
        tx_pin: "test",
        rx_pin: "test",
        profile: SerialProfile::disabled(),
        rx_dma: "test",
        tx_dma: Some("test"),
        capabilities: SerialCapabilities {
            sbus: true,
            crsf: true,
            mavlink: false,
            msp: true,
            esc_telemetry: true,
            cli: true,
            tx: true,
        },
    }];

    /// The storage owner's state, with the answers it queued readable.
    struct Owner {
        nor: MemoryNor,
        responses: ResponseRouter,
        usb_out: ResponseConsumer,
        serial_out: ResponseConsumer,
        log: LogWriter,
        maintenance: Maintenance,
        config_save: ConfigSave,
        stored_config: StoredConfig,
        armed: bool,
        calibrating: bool,
    }

    impl Owner {
        fn new() -> Self {
            let (usb, usb_out) = Box::leak(Box::new(ResponseQueue::new())).split();
            let (serial, serial_out) = Box::leak(Box::new(ResponseQueue::new())).split();
            let mut log = LogWriter::new();
            log.recover(0, 1, true);
            Self {
                nor: MemoryNor::erased(CAPACITY),
                responses: ResponseRouter::new(usb, Some(serial), || {}),
                usb_out,
                serial_out,
                log,
                maintenance: Maintenance::Idle,
                config_save: ConfigSave::idle(),
                stored_config: StoredConfig::first_hop_default(),
                armed: false,
                calibrating: false,
            }
        }

        /// Sends one command line over `link`; the action and the answers.
        fn send_on(&mut self, link: CommandLink, line: &str) -> (CommandAction, Vec<String>) {
            let command = parse_command(line).expect("a valid command");
            self.responses.answer(link);
            let action = handle(
                command,
                &mut CommandContext {
                    flash: &mut self.nor,
                    layout: StorageLayout::new(CAPACITY).unwrap(),
                    responses: &mut self.responses,
                    link,
                    armed: self.armed,
                    log: &self.log,
                    maintenance: &mut self.maintenance,
                    config_save: &mut self.config_save,
                    stored_config: &mut self.stored_config,
                    config_sequence: 0,
                    config_active_slot: 1,
                    calibrating: self.calibrating,
                    jedec: [0xef, 0x40, 0x18],
                    default_serial_bindings: SerialBindings::none(),
                    serial_routes: &ROUTES,
                },
            );
            let queue = match link {
                CommandLink::Usb => &mut self.usb_out,
                CommandLink::Serial => &mut self.serial_out,
            };
            let mut lines = Vec::new();
            while let Some(frame) = queue.dequeue() {
                lines.push(String::from_utf8(frame.as_bytes().to_vec()).unwrap());
            }
            (action, lines)
        }

        fn send(&mut self, line: &str) -> Vec<String> {
            let (action, lines) = self.send_on(CommandLink::Usb, line);
            assert_eq!(action, CommandAction::None, "{line}");
            lines
        }
    }

    #[test]
    fn destructive_and_restarting_commands_stay_on_usb() {
        let mut owner = Owner::new();
        for line in [
            "logs erase CONFIRM",
            "flash test CONFIRM",
            "reboot",
            "reboot dfu",
        ] {
            let (action, lines) = owner.send_on(CommandLink::Serial, line);
            assert_eq!(action, CommandAction::None);
            assert_eq!(lines, ["ERR USB only for this command\r\n"], "{line}");
        }
        assert!(!owner.maintenance.is_active());
    }

    #[test]
    fn an_armed_craft_refuses_every_change() {
        let mut owner = Owner::new();
        owner.armed = true;
        let refusals = [
            (
                "config set roll_p 3",
                "ERR config changes disabled while armed\r\n",
            ),
            ("config save", "ERR config save disabled while armed\r\n"),
            (
                "serial uart3 rc",
                "ERR config changes disabled while armed\r\n",
            ),
            (
                "flash test CONFIRM",
                "ERR flash test disabled while armed\r\n",
            ),
            (
                "logs erase CONFIRM",
                "ERR log erase disabled while armed\r\n",
            ),
            (
                "logs erase-synced CONFIRM",
                "ERR log erase disabled while armed\r\n",
            ),
            ("logs read-page 0", "ERR log reads disabled while armed\r\n"),
            (
                "imu calibrate",
                "ERR imu calibration disabled while armed\r\n",
            ),
            ("reboot", "ERR reboot disabled while armed\r\n"),
        ];
        for (line, refusal) in refusals {
            assert_eq!(owner.send(line), [refusal], "{line}");
        }
        assert_eq!(owner.stored_config, StoredConfig::first_hop_default());
        assert!(!owner.maintenance.is_active() && !owner.config_save.is_active());
    }

    #[test]
    fn one_long_operation_at_a_time() {
        let mut owner = Owner::new();
        assert_eq!(
            owner.send("flash test CONFIRM"),
            ["OK flash scratch test started\r\n"]
        );
        for line in ["flash test CONFIRM", "logs erase CONFIRM", "config save"] {
            assert_eq!(
                owner.send(line),
                ["ERR flash maintenance already active\r\n"],
                "{line}"
            );
        }
        let (action, lines) = owner.send_on(CommandLink::Usb, "reboot");
        assert_eq!(action, CommandAction::None);
        assert_eq!(lines, ["ERR flash maintenance already active\r\n"]);

        let mut owner = Owner::new();
        owner.calibrating = true;
        assert_eq!(
            owner.send("config save"),
            ["ERR flash maintenance already active\r\n"]
        );
    }

    #[test]
    fn a_set_stages_and_a_save_persists() {
        let mut owner = Owner::new();
        assert_eq!(
            owner.send("config set roll_p 3"),
            ["OK staged; use config save\r\n"]
        );
        assert!(!owner.config_save.is_active(), "staged, not saved");
        assert_eq!(owner.send("config get roll_p"), ["OK roll_p=3.0000\r\n"]);
        assert_eq!(owner.send("config save"), ["OK config save started\r\n"]);
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let saved = loop {
            match owner.config_save.step(&mut owner.nor, layout) {
                SaveStep::Running => continue,
                step => break step,
            }
        };
        assert!(matches!(saved, SaveStep::Saved { config, .. } if config == owner.stored_config));
    }

    #[test]
    fn serial_ports_show_the_board_routes_and_stage_a_change() {
        let mut owner = Owner::new();
        assert_eq!(
            owner.send("serial"),
            [
                "OK serial default ports=1; applies after save and reboot\r\n",
                "OK uart3=none\r\n"
            ]
        );
        assert_eq!(
            owner.send("serial uart3 rc"),
            ["OK staged; use config save, then reboot\r\n"]
        );
        let bindings = owner.stored_config.serial_bindings.unwrap();
        assert_eq!(
            bindings.function(LogicalSerialPort::Uart3),
            SerialFunction::RcInput
        );
    }

    #[test]
    fn the_log_answers_from_the_writers_position() {
        let mut owner = Owner::new();
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let mut expected = heapless::String::<64>::new();
        let _ = write!(expected, "OK u=0 n=1 t={} w=1\r\n", layout.log_page_count);
        assert_eq!(owner.send("logs list"), [expected.as_str()]);
        assert_eq!(
            owner.send("logs read-page 0"),
            ["ERR log page is not present\r\n"]
        );
        assert_eq!(owner.send("logs ack 3 10"), ["ERR no such flight\r\n"]);
        assert_eq!(
            owner.send("logs erase-synced CONFIRM"),
            ["OK log erase started\r\n"]
        );
        assert!(owner.maintenance.is_erasing_log());
    }

    #[test]
    fn calibrating_and_rebooting_are_left_to_the_owner_unanswered() {
        let mut owner = Owner::new();
        assert_eq!(
            owner.send_on(CommandLink::Usb, "imu calibrate"),
            (CommandAction::StartCalibration, Vec::new())
        );
        assert_eq!(
            owner.send_on(CommandLink::Usb, "reboot dfu"),
            (
                CommandAction::Reboot {
                    rom_bootloader: true
                },
                Vec::new()
            )
        );
        assert_eq!(
            owner.send_on(CommandLink::Usb, "reboot"),
            (
                CommandAction::Reboot {
                    rom_bootloader: false
                },
                Vec::new()
            )
        );
    }

    #[test]
    fn help_and_flash_info_answer_at_once() {
        let mut owner = Owner::new();
        let help = owner.send("help");
        assert!(
            help.iter()
                .all(|line| line.starts_with("OK ") && line.ends_with("\r\n"))
        );
        assert!(help.contains(&String::from("OK reboot | reboot dfu\r\n")));
        assert!(help.contains(&String::from("OK imu calibrate | imu level\r\n")));
        assert_eq!(
            owner.send("flash info"),
            ["OK jedec=ef:40:18 bytes=65536 ready=1\r\n"]
        );
        assert_eq!(owner.send("imu level"), ["OK imu level x=0 y=0 z=0 mg\r\n"]);
    }
}
