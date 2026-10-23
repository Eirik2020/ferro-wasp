//! The text command line's vocabulary: the commands it parses, the bounded
//! queues between a link and the storage task, the response frames, and the
//! configurator RPC's view of the stored configuration.

pub use ferrowasp_config::keys::ConfigKey;
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_config::stored::StoredConfig;
use ferrowasp_protocol::blackbox::FLASH_PAGE_LEN;
use ferrowasp_safety::BenchMotorRequest;
use ferrowasp_serial::{LogicalSerialPort, SerialFunction};
use heapless::String;
use heapless::spsc::{Consumer, Producer, Queue};

#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2::rpc;

pub const COMMAND_QUEUE_CAPACITY: usize = 8;

pub const RESPONSE_QUEUE_CAPACITY: usize = 32;

pub const USB_COMMAND_LINE_CAPACITY: usize = 96;

pub const USB_RESPONSE_CAPACITY: usize = 64;

/// The answer to a storage command while storage is unavailable: no card or
/// flash, or recovery failed. `flash info` reports what the probe saw, not
/// ready; anything else is refused. Answering at all matters: an unanswered
/// command left the host waiting out its timeout.
pub fn storage_unavailable_response(
    command: &StorageCommand,
    jedec: [u8; 3],
) -> String<USB_RESPONSE_CAPACITY> {
    use core::fmt::Write;

    let mut response = String::new();
    let _ = match command {
        StorageCommand::FlashInfo => write!(
            response,
            "OK jedec={:02x}:{:02x}:{:02x} bytes=0 ready=0\r\n",
            jedec[0], jedec[1], jedec[2]
        ),
        _ => response.write_str("ERR storage unavailable\r\n"),
    };
    response
}

/// Format the storage catalogue summary without exceeding one USB response
/// frame, even when every counter reaches its full `u32` width.
pub fn format_log_info_response(
    used_pages: u32,
    next_flight: u32,
    total_pages: u32,
    writable: bool,
) -> Option<String<USB_RESPONSE_CAPACITY>> {
    use core::fmt::Write;

    let mut response = String::new();
    write!(
        response,
        "OK u={used_pages} n={next_flight} t={total_pages} w={}\r\n",
        u8::from(writable)
    )
    .ok()?;
    Some(response)
}

pub type CommandQueue = Queue<StorageCommand, COMMAND_QUEUE_CAPACITY>;

pub type CommandProducer = Producer<'static, StorageCommand>;

pub type CommandConsumer = Consumer<'static, StorageCommand>;

pub type ResponseQueue = Queue<ResponseFrame, RESPONSE_QUEUE_CAPACITY>;

pub type ResponseProducer = Producer<'static, ResponseFrame>;

pub type ResponseConsumer = Consumer<'static, ResponseFrame>;

#[cfg(feature = "mspv2_configurator")]
pub const RPC_COMMAND_QUEUE_CAPACITY: usize = 4;

#[cfg(feature = "mspv2_configurator")]
pub const RPC_RESPONSE_QUEUE_CAPACITY: usize = 2;

#[cfg(feature = "mspv2_configurator")]
pub type RpcCommandQueue = Queue<rpc::RpcRequest, RPC_COMMAND_QUEUE_CAPACITY>;

#[cfg(feature = "mspv2_configurator")]
pub type RpcCommandProducer = Producer<'static, rpc::RpcRequest>;

#[cfg(feature = "mspv2_configurator")]
pub type RpcCommandConsumer = Consumer<'static, rpc::RpcRequest>;

#[cfg(feature = "mspv2_configurator")]
pub type RpcResponseQueue = Queue<rpc::RpcResponse, RPC_RESPONSE_QUEUE_CAPACITY>;

#[cfg(feature = "mspv2_configurator")]
pub type RpcResponseProducer = Producer<'static, rpc::RpcResponse>;

#[cfg(feature = "mspv2_configurator")]
pub type RpcResponseConsumer = Consumer<'static, rpc::RpcResponse>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StorageCommand {
    Help,
    FlashInfo,
    FlashTestConfirmed,
    LogsList,
    LogsReadPage(u32),
    LogsEraseConfirmed,
    /// Complete flights no host has acknowledged storing.
    LogsUnsynced,
    /// A host stored `flight`, `pages` long. The firmware checks the length
    /// against the flight on the device, then records it in the sync ledger.
    LogsAck {
        flight: u32,
        pages: u32,
    },
    /// Erase the log, refused while any flight is unacknowledged.
    LogsEraseSyncedConfirmed,
    ConfigGet(ConfigKey),
    /// Set a value: at once, under an arm inhibit, for a live key; after
    /// `config save` and a reboot for a boot-only one.
    ConfigSet(ConfigKey, f32),
    ConfigSave,
    /// Whether the boot fell back from the saved configuration.
    ConfigFallback,
    /// Acknowledge the boot's fallback, so the craft may arm; disarmed only.
    ConfigFallbackAck,
    /// Show each port's saved function.
    SerialShow,
    /// Stage a port's function; it applies after `config save` and a reboot.
    SerialSet(LogicalSerialPort, SerialFunction),
    /// Attitude and motor activity, answered at once by the USB task rather
    /// than waiting for the next periodic status line.
    Live,
    /// The last receiver frame's sixteen channels, answered at once like
    /// `Live`, so a configurator can show the sticks as they move.
    Rc,
    /// Each ESC output's UART and bidirectional telemetry, answered at once
    /// by the command link, for props-off bench checks over USB.
    Esc,
    /// What the firmware was built from: commit, dirty tree, and features.
    Build,
    /// A props-off motor check. Handled by the USB task, which hands it to
    /// the safety master; never by the storage task.
    Motor(BenchMotorRequest),
    /// Restart the controller once it is disarmed and storage is idle: the
    /// firmware again, or, with `dfu`, the chip's ROM bootloader, which a
    /// configurator flashes over USB.
    Reboot {
        rom_bootloader: bool,
    },
    /// Calibrate the accelerometer's level on a still, level craft, and save
    /// it with the configuration. Disarmed only.
    ImuCalibrate,
    /// Show the stored accelerometer level offsets.
    ImuLevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandParseError {
    LineTooLong,
    InvalidUtf8,
    UnknownCommand,
    MissingArgument,
    InvalidArgument,
    ConfirmationRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseFrame {
    bytes: [u8; USB_RESPONSE_CAPACITY],
    len: u8,
}

impl ResponseFrame {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > USB_RESPONSE_CAPACITY {
            return None;
        }
        let mut frame = Self {
            bytes: [0; USB_RESPONSE_CAPACITY],
            len: bytes.len() as u8,
        };
        frame.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(frame)
    }

    pub fn from_text(value: &str) -> Option<Self> {
        Self::from_bytes(value.as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

pub struct CommandParser {
    line: String<USB_COMMAND_LINE_CAPACITY>,
    overflowed: bool,
}

impl CommandParser {
    pub const fn new() -> Self {
        Self {
            line: String::new(),
            overflowed: false,
        }
    }

    /// Drops a partially received line.
    ///
    /// The host may close the port mid-line, and reopening it can deliver a
    /// stray byte. Either way the next real command would be prefixed with
    /// junk and rejected, so the USB task clears the parser whenever the
    /// device leaves the configured state.
    pub fn clear(&mut self) {
        self.line.clear();
        self.overflowed = false;
    }

    pub fn ingest(&mut self, byte: u8) -> Option<Result<StorageCommand, CommandParseError>> {
        if byte != b'\r' && byte != b'\n' {
            if self.line.push(byte as char).is_err() {
                self.overflowed = true;
            }
            return None;
        }
        if self.line.is_empty() && !self.overflowed {
            return None;
        }
        if self.overflowed {
            self.line.clear();
            self.overflowed = false;
            return Some(Err(CommandParseError::LineTooLong));
        }
        let result = parse_command(self.line.as_str());
        self.line.clear();
        Some(result)
    }
}

impl Default for CommandParser {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_command(line: &str) -> Result<StorageCommand, CommandParseError> {
    let mut words = line.split_ascii_whitespace();
    match (words.next(), words.next()) {
        (Some("help"), None) => Ok(StorageCommand::Help),
        (Some("flash"), Some("info")) if words.next().is_none() => Ok(StorageCommand::FlashInfo),
        (Some("flash"), Some("test")) => match (words.next(), words.next()) {
            (Some("CONFIRM"), None) => Ok(StorageCommand::FlashTestConfirmed),
            _ => Err(CommandParseError::ConfirmationRequired),
        },
        (Some("logs"), Some("list")) if words.next().is_none() => Ok(StorageCommand::LogsList),
        (Some("logs"), Some("read-page")) => {
            let page = words.next().ok_or(CommandParseError::MissingArgument)?;
            if words.next().is_some() {
                return Err(CommandParseError::InvalidArgument);
            }
            page.parse::<u32>()
                .map(StorageCommand::LogsReadPage)
                .map_err(|_| CommandParseError::InvalidArgument)
        }
        (Some("logs"), Some("erase")) => match (words.next(), words.next()) {
            (Some("CONFIRM"), None) => Ok(StorageCommand::LogsEraseConfirmed),
            _ => Err(CommandParseError::ConfirmationRequired),
        },
        (Some("logs"), Some("unsynced")) if words.next().is_none() => {
            Ok(StorageCommand::LogsUnsynced)
        }
        (Some("logs"), Some("ack")) => {
            let mut number = || -> Result<u32, CommandParseError> {
                words
                    .next()
                    .ok_or(CommandParseError::MissingArgument)?
                    .parse::<u32>()
                    .map_err(|_| CommandParseError::InvalidArgument)
            };
            let flight = number()?;
            let pages = number()?;
            if words.next().is_some() {
                return Err(CommandParseError::InvalidArgument);
            }
            Ok(StorageCommand::LogsAck { flight, pages })
        }
        (Some("logs"), Some("erase-synced")) => match (words.next(), words.next()) {
            (Some("CONFIRM"), None) => Ok(StorageCommand::LogsEraseSyncedConfirmed),
            _ => Err(CommandParseError::ConfirmationRequired),
        },
        (Some("config"), Some("get")) => {
            let key = words.next().ok_or(CommandParseError::MissingArgument)?;
            if words.next().is_some() {
                return Err(CommandParseError::InvalidArgument);
            }
            ConfigKey::parse(key)
                .map(StorageCommand::ConfigGet)
                .ok_or(CommandParseError::InvalidArgument)
        }
        (Some("config"), Some("set")) => {
            let key = ConfigKey::parse(words.next().ok_or(CommandParseError::MissingArgument)?)
                .ok_or(CommandParseError::InvalidArgument)?;
            let value = words
                .next()
                .ok_or(CommandParseError::MissingArgument)?
                .parse::<f32>()
                .map_err(|_| CommandParseError::InvalidArgument)?;
            if words.next().is_some() || !value.is_finite() {
                return Err(CommandParseError::InvalidArgument);
            }
            Ok(StorageCommand::ConfigSet(key, value))
        }
        (Some("config"), Some("save")) if words.next().is_none() => Ok(StorageCommand::ConfigSave),
        (Some("config"), Some("fallback")) => match (words.next(), words.next()) {
            (None, _) => Ok(StorageCommand::ConfigFallback),
            (Some("ack"), None) => Ok(StorageCommand::ConfigFallbackAck),
            _ => Err(CommandParseError::InvalidArgument),
        },
        (Some("serial"), None) => Ok(StorageCommand::SerialShow),
        (Some("live"), None) => Ok(StorageCommand::Live),
        (Some("rc"), None) => Ok(StorageCommand::Rc),
        (Some("esc"), None) => Ok(StorageCommand::Esc),
        (Some("build"), None) => Ok(StorageCommand::Build),
        (Some("imu"), Some("calibrate")) if words.next().is_none() => {
            Ok(StorageCommand::ImuCalibrate)
        }
        (Some("imu"), Some("level")) if words.next().is_none() => Ok(StorageCommand::ImuLevel),
        (Some("reboot"), None) => Ok(StorageCommand::Reboot {
            rom_bootloader: false,
        }),
        (Some("reboot"), Some("dfu")) if words.next().is_none() => Ok(StorageCommand::Reboot {
            rom_bootloader: true,
        }),
        (Some("motor"), Some("stop")) if words.next().is_none() => {
            Ok(StorageCommand::Motor(BenchMotorRequest::Stop))
        }
        (Some("motor"), Some("spin")) => {
            let logical_motor = parse_motor(words.next())?;
            match (words.next(), words.next()) {
                (Some("CONFIRM"), None) => Ok(StorageCommand::Motor(BenchMotorRequest::Spin {
                    logical_motor,
                })),
                _ => Err(CommandParseError::ConfirmationRequired),
            }
        }
        (Some("motor"), Some("dir")) => {
            let logical_motor = parse_motor(words.next())?;
            let reversed = match words.next() {
                Some("normal") => false,
                Some("reversed") => true,
                Some(_) => return Err(CommandParseError::InvalidArgument),
                None => return Err(CommandParseError::MissingArgument),
            };
            match (words.next(), words.next()) {
                (Some("CONFIRM"), None) => {
                    Ok(StorageCommand::Motor(BenchMotorRequest::Direction {
                        logical_motor,
                        reversed,
                    }))
                }
                _ => Err(CommandParseError::ConfirmationRequired),
            }
        }
        (Some("serial"), Some(port)) => {
            let port = LogicalSerialPort::parse(port).ok_or(CommandParseError::InvalidArgument)?;
            let function =
                SerialFunction::parse(words.next().ok_or(CommandParseError::MissingArgument)?)
                    .ok_or(CommandParseError::InvalidArgument)?;
            if words.next().is_some() {
                return Err(CommandParseError::InvalidArgument);
            }
            Ok(StorageCommand::SerialSet(port, function))
        }
        _ => Err(CommandParseError::UnknownCommand),
    }
}

fn parse_motor(word: Option<&str>) -> Result<u8, CommandParseError> {
    match word
        .ok_or(CommandParseError::MissingArgument)?
        .parse::<u8>()
    {
        Ok(motor @ 1..=4) => Ok(motor),
        _ => Err(CommandParseError::InvalidArgument),
    }
}

/// The configurator RPC's view of the stored configuration (MSPv2), kept
/// outside the safety-critical configuration crate, which cannot depend on
/// the wire protocol.
#[cfg(feature = "mspv2_configurator")]
pub trait StoredConfigRpc: Sized {
    fn to_rpc(self) -> rpc::ConfigV1;
    fn from_rpc(config: rpc::ConfigV1) -> Result<Self, rpc::ConfigFieldId>;
    fn apply_rpc(self, config: rpc::ConfigV1) -> Result<Self, rpc::ConfigFieldId>;
    fn crc32(self) -> u32;
}

#[cfg(feature = "mspv2_configurator")]
impl StoredConfigRpc for StoredConfig {
    fn to_rpc(self) -> rpc::ConfigV1 {
        rpc::ConfigV1 {
            roll_p: self.get(ConfigKey::RollP),
            roll_i: self.get(ConfigKey::RollI),
            roll_d: self.get(ConfigKey::RollD),
            pitch_p: self.get(ConfigKey::PitchP),
            pitch_i: self.get(ConfigKey::PitchI),
            pitch_d: self.get(ConfigKey::PitchD),
            yaw_p: self.get(ConfigKey::YawP),
            yaw_i: self.get(ConfigKey::YawI),
            yaw_d: self.get(ConfigKey::YawD),
            imu_lpf_hz: self.get(ConfigKey::ImuLpfHz),
            log_rate_divisor: self.log_rate_divisor,
        }
    }

    fn from_rpc(config: rpc::ConfigV1) -> Result<Self, rpc::ConfigFieldId> {
        Self::first_hop_default().apply_rpc(config)
    }

    /// Applies the legacy configurator schema without discarding newer fields
    /// that are currently available through the USB text CLI only.
    fn apply_rpc(mut self, config: rpc::ConfigV1) -> Result<Self, rpc::ConfigFieldId> {
        let fields = [
            (ConfigKey::RollP, config.roll_p, rpc::ConfigFieldId::RollP),
            (ConfigKey::RollI, config.roll_i, rpc::ConfigFieldId::RollI),
            (ConfigKey::RollD, config.roll_d, rpc::ConfigFieldId::RollD),
            (
                ConfigKey::PitchP,
                config.pitch_p,
                rpc::ConfigFieldId::PitchP,
            ),
            (
                ConfigKey::PitchI,
                config.pitch_i,
                rpc::ConfigFieldId::PitchI,
            ),
            (
                ConfigKey::PitchD,
                config.pitch_d,
                rpc::ConfigFieldId::PitchD,
            ),
            (ConfigKey::YawP, config.yaw_p, rpc::ConfigFieldId::YawP),
            (ConfigKey::YawI, config.yaw_i, rpc::ConfigFieldId::YawI),
            (ConfigKey::YawD, config.yaw_d, rpc::ConfigFieldId::YawD),
            (
                ConfigKey::ImuLpfHz,
                config.imu_lpf_hz,
                rpc::ConfigFieldId::ImuLpfHz,
            ),
            (
                ConfigKey::LogRateDivisor,
                config.log_rate_divisor as f32,
                rpc::ConfigFieldId::LogRateDivisor,
            ),
        ];
        for (key, value, field) in fields {
            if !self.set(key, value) {
                return Err(field);
            }
        }
        Ok(self)
    }

    fn crc32(self) -> u32 {
        ferrowasp_protocol::blackbox::crc32(&self.encode())
    }
}

/// Emits one bounded text line for each 16-byte slice of a flash page.
pub fn emit_page_hex_lines<Emit>(
    page_index: u32,
    page: &[u8; FLASH_PAGE_LEN],
    mut emit: Emit,
) -> bool
where
    Emit: FnMut(&str) -> bool,
{
    use core::fmt::Write;

    const HEX: &[u8; 16] = b"0123456789abcdef";
    for chunk_index in 0..16 {
        let offset = chunk_index * 16;
        let mut line = String::<USB_RESPONSE_CAPACITY>::new();
        if write!(line, "PAGE {} {:03} ", page_index, offset).is_err() {
            return false;
        }
        for byte in &page[offset..offset + 16] {
            if line.push(HEX[(byte >> 4) as usize] as char).is_err()
                || line.push(HEX[(byte & 0x0f) as usize] as char).is_err()
            {
                return false;
            }
        }
        if line.push_str("\r\n").is_err() || !emit(line.as_str()) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_cli_requires_explicit_erase_confirmation() {
        assert_eq!(parse_command("flash info"), Ok(StorageCommand::FlashInfo));
        assert_eq!(
            storage_unavailable_response(&StorageCommand::FlashInfo, [0, 0, 0]).as_str(),
            "OK jedec=00:00:00 bytes=0 ready=0\r\n"
        );
        assert_eq!(
            storage_unavailable_response(&parse_command("logs list").unwrap(), [0, 0, 0]).as_str(),
            "ERR storage unavailable\r\n"
        );
        assert_eq!(
            parse_command("flash test"),
            Err(CommandParseError::ConfirmationRequired)
        );
        assert_eq!(
            parse_command("flash test CONFIRM"),
            Ok(StorageCommand::FlashTestConfirmed)
        );
        assert_eq!(parse_command("logs list"), Ok(StorageCommand::LogsList));
        assert_eq!(
            parse_command("logs read-page 17"),
            Ok(StorageCommand::LogsReadPage(17))
        );
        assert_eq!(
            parse_command("logs erase"),
            Err(CommandParseError::ConfirmationRequired)
        );
        assert_eq!(
            parse_command("logs erase CONFIRM"),
            Ok(StorageCommand::LogsEraseConfirmed)
        );
    }

    #[test]
    fn log_sync_commands_parse_and_erase_still_needs_confirmation() {
        assert_eq!(
            parse_command("logs unsynced"),
            Ok(StorageCommand::LogsUnsynced)
        );
        assert_eq!(
            parse_command("logs ack 7 412"),
            Ok(StorageCommand::LogsAck {
                flight: 7,
                pages: 412
            })
        );
        assert_eq!(
            parse_command("logs ack 7"),
            Err(CommandParseError::MissingArgument)
        );
        assert_eq!(
            parse_command("logs ack 7 -1"),
            Err(CommandParseError::InvalidArgument)
        );
        assert_eq!(
            parse_command("logs erase-synced"),
            Err(CommandParseError::ConfirmationRequired)
        );
        assert_eq!(
            parse_command("logs erase-synced CONFIRM"),
            Ok(StorageCommand::LogsEraseSyncedConfirmed)
        );
    }

    #[test]
    fn usb_cli_parses_only_whitelisted_config_keys() {
        assert_eq!(
            parse_command("config set pitch_p 0.30"),
            Ok(StorageCommand::ConfigSet(ConfigKey::PitchP, 0.30))
        );
        assert_eq!(
            parse_command("config set roll_expo 0.50"),
            Ok(StorageCommand::ConfigSet(ConfigKey::RollExpo, 0.50))
        );
        assert_eq!(
            parse_command("config get rc_deadband"),
            Ok(StorageCommand::ConfigGet(ConfigKey::RcDeadband))
        );
        assert_eq!(
            parse_command("config get motor_authority"),
            Err(CommandParseError::InvalidArgument)
        );
    }

    #[test]
    fn motor_commands_need_confirmation_and_a_motor_number() {
        assert_eq!(
            parse_command("motor spin 3 CONFIRM"),
            Ok(StorageCommand::Motor(BenchMotorRequest::Spin {
                logical_motor: 3
            }))
        );
        assert_eq!(
            parse_command("motor spin 3"),
            Err(CommandParseError::ConfirmationRequired)
        );
        assert_eq!(
            parse_command("motor spin 5 CONFIRM"),
            Err(CommandParseError::InvalidArgument)
        );
        assert_eq!(
            parse_command("motor dir 2 reversed CONFIRM"),
            Ok(StorageCommand::Motor(BenchMotorRequest::Direction {
                logical_motor: 2,
                reversed: true
            }))
        );
        assert_eq!(
            parse_command("motor dir 2 backwards CONFIRM"),
            Err(CommandParseError::InvalidArgument)
        );
        assert_eq!(
            parse_command("motor stop"),
            Ok(StorageCommand::Motor(BenchMotorRequest::Stop))
        );
        assert_eq!(parse_command("live"), Ok(StorageCommand::Live));
        assert_eq!(parse_command("rc"), Ok(StorageCommand::Rc));
        assert_eq!(parse_command("esc"), Ok(StorageCommand::Esc));
        assert_eq!(parse_command("build"), Ok(StorageCommand::Build));
    }

    #[test]
    fn imu_commands_parse() {
        assert_eq!(
            parse_command("imu calibrate"),
            Ok(StorageCommand::ImuCalibrate)
        );
        assert_eq!(parse_command("imu level"), Ok(StorageCommand::ImuLevel));
        assert_eq!(
            parse_command("imu calibrate now"),
            Err(CommandParseError::UnknownCommand)
        );
    }

    #[test]
    fn reboot_commands_parse() {
        assert_eq!(
            parse_command("reboot"),
            Ok(StorageCommand::Reboot {
                rom_bootloader: false
            })
        );
        assert_eq!(
            parse_command("reboot dfu"),
            Ok(StorageCommand::Reboot {
                rom_bootloader: true
            })
        );
        assert_eq!(
            parse_command("reboot dfu now"),
            Err(CommandParseError::UnknownCommand)
        );
        assert_eq!(
            parse_command("reboot bootloader"),
            Err(CommandParseError::UnknownCommand)
        );
    }

    #[test]
    fn serial_commands_parse() {
        assert_eq!(parse_command("serial"), Ok(StorageCommand::SerialShow));
        assert_eq!(
            parse_command("serial uart4 esc_telemetry"),
            Ok(StorageCommand::SerialSet(
                LogicalSerialPort::Uart4,
                SerialFunction::EscTelemetry
            ))
        );
        assert_eq!(
            parse_command("serial uart9 rc"),
            Err(CommandParseError::InvalidArgument)
        );
        assert_eq!(
            parse_command("serial uart2"),
            Err(CommandParseError::MissingArgument)
        );
    }

    #[cfg(feature = "mspv2_configurator")]
    #[test]
    fn rpc_config_round_trips_and_reports_the_invalid_field() {
        let stored = StoredConfig::first_hop_default();
        assert_eq!(StoredConfig::from_rpc(stored.to_rpc()), Ok(stored));

        let mut with_usb_rates = stored;
        assert!(with_usb_rates.set(ConfigKey::YawMaxRate, 350.0));
        let mut rpc_update = with_usb_rates.to_rpc();
        rpc_update.roll_p = 0.8;
        let updated = with_usb_rates.apply_rpc(rpc_update).unwrap();
        assert_eq!(updated.get(ConfigKey::RollP), 0.8);
        assert_eq!(updated.get(ConfigKey::YawMaxRate), 350.0);

        let mut invalid = stored.to_rpc();
        invalid.imu_lpf_hz = 0.5;
        assert_eq!(
            StoredConfig::from_rpc(invalid),
            Err(rpc::ConfigFieldId::ImuLpfHz)
        );
    }

    #[test]
    fn streaming_command_parser_handles_crlf_without_duplicate_command() {
        let mut parser = CommandParser::new();
        let mut result = None;
        for byte in b"config get yaw_i\r\n" {
            if let Some(command) = parser.ingest(*byte) {
                assert!(result.is_none());
                result = Some(command);
            }
        }
        assert_eq!(result, Some(Ok(StorageCommand::ConfigGet(ConfigKey::YawI))));
    }

    /// FerroConfigurator opens the port with this line to flush whatever
    /// another program left half-sent. It must be refused after any prefix of
    /// any command: completing one would run it. That holds while every command
    /// takes a fixed number of words.
    #[test]
    fn the_configurator_resync_line_is_refused_after_any_partial_command() {
        use core::fmt::Write as _;

        fn refused_after_every_prefix(command: &str) {
            assert!(parse_command(command).is_ok(), "{command} should parse");
            for end in 0..=command.len() {
                let mut line: String<64> = String::new();
                line.push_str(&command[..end]).unwrap();
                line.push_str(" #resync").unwrap();
                assert!(parse_command(&line).is_err(), "`{line}` was accepted");
            }
        }

        for command in [
            "help",
            "flash info",
            "flash test CONFIRM",
            "logs list",
            "logs read-page 12",
            "logs erase CONFIRM",
            "logs unsynced",
            "logs ack 3 40",
            "logs erase-synced CONFIRM",
            "config save",
        ] {
            refused_after_every_prefix(command);
        }
        for key in ConfigKey::ALL {
            let mut get: String<48> = String::new();
            write!(get, "config get {}", key.name()).unwrap();
            refused_after_every_prefix(&get);
            let mut set: String<48> = String::new();
            write!(set, "config set {} 2.5", key.name()).unwrap();
            refused_after_every_prefix(&set);
        }
    }

    #[test]
    fn a_partial_line_left_by_a_closed_port_does_not_spoil_the_next_command() {
        let mut parser = CommandParser::new();
        // The host closed the port part-way through a line.
        for byte in b"config ge" {
            assert!(parser.ingest(*byte).is_none());
        }

        // Without the clear, this is what the operator saw: the leftover bytes
        // prefix the next command and the whole line is rejected once.
        let mut spoiled = CommandParser::new();
        for byte in b"config ge" {
            spoiled.ingest(*byte);
        }
        let mut first = None;
        for byte in b"config get roll_p\r\n" {
            if let Some(command) = spoiled.ingest(*byte) {
                first = Some(command);
            }
        }
        assert_eq!(first, Some(Err(CommandParseError::UnknownCommand)));

        // Clearing on deconfigure makes the next command parse first time.
        parser.clear();
        let mut result = None;
        for byte in b"config get roll_p\r\n" {
            if let Some(command) = parser.ingest(*byte) {
                assert!(result.is_none());
                result = Some(command);
            }
        }
        assert_eq!(
            result,
            Some(Ok(StorageCommand::ConfigGet(ConfigKey::RollP)))
        );
    }

    #[test]
    fn log_info_response_fits_with_maximum_width_counters() {
        let response = format_log_info_response(u32::MAX, u32::MAX, u32::MAX, true).unwrap();

        assert!(response.len() <= USB_RESPONSE_CAPACITY);
        assert!(response.ends_with("\r\n"));
        assert_eq!(
            response.as_str(),
            "OK u=4294967295 n=4294967295 t=4294967295 w=1\r\n"
        );
    }
}
