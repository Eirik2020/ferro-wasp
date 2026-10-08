//! Portable state and bounded channels for onboard SPI-NOR storage.

use ferrowasp_core::blackbox::{
    FLASH_PAGE_LEN, FLIGHT_RECORD_FLAG_BOOT_SESSION_START, FlightRecord, RECORDS_PER_PAGE,
    decode_page, encode_page,
};
pub use ferrowasp_core::config::ConfigKey;
use ferrowasp_io_core::serial::{
    LogicalSerialPort, RcProtocol, SERIAL_PORT_SLOTS, SerialBindings, SerialFunction,
};
use heapless::String;
use heapless::spsc::{Consumer, Producer, Queue};

#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2::rpc;

use ferrowasp_core::actuator::MotorOutputMap;
use ferrowasp_core::safety::BenchMotorRequest;

use crate::drone_toolbox::{
    ActualRateAxis, PidGains, RC_RATE_PROFILE, RateControllerGains, RcChannelMap, RcRateProfile,
    TuningProfile, gyro_lpf_corner_hz,
};

pub const RECORD_QUEUE_CAPACITY: usize = 64;
pub const CONFIG_SLOT_COUNT: u32 = 2;
pub const CONFIG_SECTOR_SIZE: u32 = 4096;
/// Dedicated destructive-test sector. It is never used for configuration or logs.
pub const SCRATCH_SECTOR_ADDRESS: u32 = CONFIG_SLOT_COUNT * CONFIG_SECTOR_SIZE;
pub const LOG_START_ADDRESS: u32 = SCRATCH_SECTOR_ADDRESS + CONFIG_SECTOR_SIZE;

pub type RecordQueue = Queue<FlightRecord, RECORD_QUEUE_CAPACITY>;
pub type RecordProducer = Producer<'static, FlightRecord>;
pub type RecordConsumer = Consumer<'static, FlightRecord>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordEnqueueOutcome {
    Skipped,
    Enqueued,
    Full,
}

pub fn enqueue_rate_record(
    producer: &mut RecordProducer,
    sample: crate::drone_toolbox::CompactRateBlackboxSample,
    timestamp_us: u64,
    divisor: u32,
) -> RecordEnqueueOutcome {
    let divisor = divisor.clamp(1, 16);
    if !sample.seq.is_multiple_of(divisor) {
        return RecordEnqueueOutcome::Skipped;
    }
    let record = FlightRecord {
        // The log format keeps a 32-bit microsecond stamp, which wraps every
        // 71.6 minutes; the configurator's ULog conversion unwraps it from
        // consecutive records.
        timestamp_us: timestamp_us as u32,
        control_sequence: sample.seq,
        imu_sequence: sample.imu_seq,
        flags: u16::from(sample.flags),
        raw_gyro_dps10: sample.raw_gyro_dps10,
        filtered_gyro_dps10: sample.gyro_dps10,
        command_dps10: sample.command_dps10,
        pid: sample.pid,
        throttle: sample.throttle,
        motors: sample.motors,
    };
    if producer.enqueue(record).is_ok() {
        RecordEnqueueOutcome::Enqueued
    } else {
        RecordEnqueueOutcome::Full
    }
}

pub const COMMAND_QUEUE_CAPACITY: usize = 8;
pub const RESPONSE_QUEUE_CAPACITY: usize = 32;
pub const USB_COMMAND_LINE_CAPACITY: usize = 96;
pub const USB_RESPONSE_CAPACITY: usize = 64;

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
    ConfigSet(ConfigKey, f32),
    ConfigSave,
    /// Show each port's saved function.
    SerialShow,
    /// Stage a port's function; it applies after `config save` and a reboot.
    SerialSet(LogicalSerialPort, SerialFunction),
    /// Attitude and motor activity, answered at once by the USB task rather
    /// than waiting for the next periodic status line.
    Live,
    /// A props-off motor check. Handled by the USB task, which hands it to
    /// the safety master; never by the storage task.
    Motor(BenchMotorRequest),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageReadError<E> {
    InvalidLayout,
    Device(E),
}

/// Finds the append point and next flight identifier in an append-only log.
///
/// Pages are only ever written in order, so the append point is the first
/// blank page. A page that does not decode, right after one that does, is a
/// page program cut short by power loss: it stays as the last page of its
/// flight, and the next flight follows it (see [`page_flight_id`]). Anything
/// else that does not decode means the region holds something other than
/// FerroWasp's log; it reads as empty and stays read-only until erased.
pub fn scan_log<E, Read>(
    layout: StorageLayout,
    mut read: Read,
) -> Result<(u32, u32, bool), StorageReadError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let mut page = [0xff; FLASH_PAGE_LEN];
    let mut read_page = |index: u32, page: &mut [u8; FLASH_PAGE_LEN]| {
        let address = layout
            .log_page_address(index)
            .ok_or(StorageReadError::InvalidLayout)?;
        read(address, page).map_err(StorageReadError::Device)
    };

    let mut low = 0u32;
    let mut high = layout.log_page_count;
    while low < high {
        let middle = low + (high - low) / 2;
        read_page(middle, &mut page)?;
        if page.iter().all(|byte| *byte == 0xff) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let next_page = low;
    if next_page == 0 {
        return Ok((0, 1, true));
    }

    const FOREIGN: (u32, u32, bool) = (0, 1, false);
    read_page(0, &mut page)?;
    if decode_page(&page).is_err() {
        return Ok(FOREIGN);
    }
    let newest = match page_flight_id(next_page - 1, |index, page| read_page(index, page))? {
        Some(flight_id) => flight_id,
        None => return Ok(FOREIGN),
    };
    // A full log has no blank page left to write.
    let writable = next_page < layout.log_page_count;
    Ok((next_page, newest.wrapping_add(1).max(1), writable))
}

/// The flight that log page `index` belongs to. A page that does not decode
/// right after one that does is a torn last page of that flight; `None` for
/// anything else that does not decode.
pub fn page_flight_id<E, Read>(index: u32, mut read: Read) -> Result<Option<u32>, E>
where
    Read: FnMut(u32, &mut [u8; FLASH_PAGE_LEN]) -> Result<(), E>,
{
    let mut page = [0xff; FLASH_PAGE_LEN];
    read(index, &mut page)?;
    if let Ok(metadata) = decode_page(&page) {
        return Ok(Some(metadata.flight_id));
    }
    let Some(previous) = index.checked_sub(1) else {
        return Ok(None);
    };
    read(previous, &mut page)?;
    Ok(decode_page(&page).ok().map(|metadata| metadata.flight_id))
}

/// Loads the newest valid copy-on-write configuration slot.
pub fn load_config<E, Read>(
    layout: StorageLayout,
    default: StoredConfig,
    mut read: Read,
) -> Result<(StoredConfig, u32, u8), StorageReadError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    use ferrowasp_core::blackbox::decode_config_page;

    let mut selected: Option<(StoredConfig, u32, u8)> = None;
    for slot in 0..2u8 {
        let mut page = [0xff; FLASH_PAGE_LEN];
        read(layout.config_slot_addresses[slot as usize], &mut page)
            .map_err(StorageReadError::Device)?;
        let Ok((sequence, payload)) = decode_config_page(&page) else {
            continue;
        };
        let Some(config) = StoredConfig::decode(payload) else {
            continue;
        };
        if selected
            .as_ref()
            .is_none_or(|(_, current, _)| sequence_is_newer(sequence, *current))
        {
            selected = Some((config, sequence, slot));
        }
    }
    Ok(selected.unwrap_or((default, 0, 1)))
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
        (Some("serial"), None) => Ok(StorageCommand::SerialShow),
        (Some("live"), None) => Ok(StorageCommand::Live),
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

pub const LEGACY_STORED_CONFIG_LEN: usize = 44;
/// Versions 2 and 3.
pub const V3_STORED_CONFIG_LEN: usize = 84;
/// Version 4.
pub const V4_STORED_CONFIG_LEN: usize = V3_STORED_CONFIG_LEN + SERIAL_PORT_SLOTS;
/// Where version 5's receiver fields start: stick order (two bytes), arm
/// channel, and RC protocol.
const RC_FIELDS_OFFSET: usize = V4_STORED_CONFIG_LEN;
/// Version 5.
pub const V5_STORED_CONFIG_LEN: usize = RC_FIELDS_OFFSET + 4;
/// Where version 6's motor order starts (two bytes).
const MOTOR_MAP_OFFSET: usize = V5_STORED_CONFIG_LEN;
pub const STORED_CONFIG_LEN: usize = MOTOR_MAP_OFFSET + 2;
/// Version 5 adds the RC channel map and RC protocol after the version 4
/// payload. Every earlier payload decodes with [`RcChannelMap::AETR_ARM_CH9`]
/// and SBUS, the fixed map and protocol firmware used before, so updating
/// never moves a pilot's arm switch.
///
/// Version 4 adds the serial port bindings after the version 3 payload.
/// Version 3 stores the gyro filter as a corner in hertz. Versions 1 and 2
/// stored a one-pole smoothing factor, which only means what it is meant to
/// mean at one loop rate. A version 2 payload is still read: its coefficient
/// is converted at the rate it was authored for, so a tune saved before this
/// change keeps the filter it had.
/// Version 6 adds the motor order after the version 5 payload; earlier
/// payloads decode with [`MotorOutputMap::IDENTITY`], the board's wiring.
const STORED_CONFIG_SCHEMA_VERSION: u16 = 6;
const STORED_CONFIG_SCHEMA_VERSION_RC: u16 = 5;
const STORED_CONFIG_SCHEMA_VERSION_BINDINGS: u16 = 4;
const STORED_CONFIG_SCHEMA_VERSION_CORNER: u16 = 3;
const STORED_CONFIG_SCHEMA_VERSION_ALPHA: u16 = 2;
/// Bindings bytes meaning "never set": the board's defaults apply.
const SERIAL_BINDINGS_UNSET: [u8; SERIAL_PORT_SLOTS] = [0xff; SERIAL_PORT_SLOTS];

/// The loop rate every version 1 and 2 coefficient was authored for.
const LEGACY_LPF_SAMPLE_RATE_HZ: f32 = 400.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredConfig {
    pub tuning: TuningProfile,
    pub log_rate_divisor: u16,
    /// Which function each serial port serves; `None` until a pilot sets
    /// one, and then the board's own defaults apply. Read once at boot.
    pub serial_bindings: Option<SerialBindings>,
    /// The receiver protocol on the port bound to RC input. Read once at
    /// boot, since it sets that port's line settings.
    pub rc_protocol: RcProtocol,
}

impl StoredConfig {
    pub const fn first_hop_default() -> Self {
        Self {
            tuning: TuningProfile::default_first_hop(),
            log_rate_divisor: 1,
            serial_bindings: None,
            rc_protocol: RcProtocol::Sbus,
        }
    }

    pub fn set(&mut self, key: ConfigKey, value: f32) -> bool {
        if !key.value_spec().accepts(value) {
            return false;
        }
        let mut candidate = *self;
        match key {
            ConfigKey::RollP => candidate.tuning.rate_gains.roll.p = value,
            ConfigKey::RollI => candidate.tuning.rate_gains.roll.i = value,
            ConfigKey::RollD => candidate.tuning.rate_gains.roll.d = value,
            ConfigKey::PitchP => candidate.tuning.rate_gains.pitch.p = value,
            ConfigKey::PitchI => candidate.tuning.rate_gains.pitch.i = value,
            ConfigKey::PitchD => candidate.tuning.rate_gains.pitch.d = value,
            ConfigKey::YawP => candidate.tuning.rate_gains.yaw.p = value,
            ConfigKey::YawI => candidate.tuning.rate_gains.yaw.i = value,
            ConfigKey::YawD => candidate.tuning.rate_gains.yaw.d = value,
            ConfigKey::ImuLpfHz => candidate.tuning.imu_lpf_hz = value,
            ConfigKey::LogRateDivisor => {
                let integer = value as u16;
                if !(1.0..=16.0).contains(&value) || integer as f32 != value {
                    return false;
                }
                candidate.log_rate_divisor = integer;
            }
            ConfigKey::RcDeadband => {
                let integer = value as u16;
                if !(0.0..=100.0).contains(&value) || integer as f32 != value {
                    return false;
                }
                candidate.tuning.rc_rates.deadband = integer;
            }
            ConfigKey::RollCenterRate => {
                candidate.tuning.rc_rates.roll.center_sensitivity_dps = value
            }
            ConfigKey::RollMaxRate => candidate.tuning.rc_rates.roll.max_rate_dps = value,
            ConfigKey::RollExpo => candidate.tuning.rc_rates.roll.expo = value,
            ConfigKey::PitchCenterRate => {
                candidate.tuning.rc_rates.pitch.center_sensitivity_dps = value
            }
            ConfigKey::PitchMaxRate => candidate.tuning.rc_rates.pitch.max_rate_dps = value,
            ConfigKey::PitchExpo => candidate.tuning.rc_rates.pitch.expo = value,
            ConfigKey::YawCenterRate => {
                candidate.tuning.rc_rates.yaw.center_sensitivity_dps = value
            }
            ConfigKey::YawMaxRate => candidate.tuning.rc_rates.yaw.max_rate_dps = value,
            ConfigKey::YawExpo => candidate.tuning.rc_rates.yaw.expo = value,
            ConfigKey::RcMap => {
                match RcChannelMap::from_config(value as u16, candidate.tuning.rc_map.arm_channel())
                {
                    Some(map) => candidate.tuning.rc_map = map,
                    None => return false,
                }
            }
            ConfigKey::RcArmChannel => {
                match RcChannelMap::from_config(candidate.tuning.rc_map.stick_order(), value as u8)
                {
                    Some(map) => candidate.tuning.rc_map = map,
                    None => return false,
                }
            }
            ConfigKey::RcProtocol => match RcProtocol::from_u8(value as u8) {
                Some(protocol) => candidate.rc_protocol = protocol,
                None => return false,
            },
            ConfigKey::MotorMap => match MotorOutputMap::from_config(value as u16) {
                Some(map) => candidate.tuning.motor_map = map,
                None => return false,
            },
        }
        if candidate.tuning.sanitized() != candidate.tuning {
            return false;
        }
        *self = candidate;
        true
    }

    pub fn get(self, key: ConfigKey) -> f32 {
        match key {
            ConfigKey::RollP => self.tuning.rate_gains.roll.p,
            ConfigKey::RollI => self.tuning.rate_gains.roll.i,
            ConfigKey::RollD => self.tuning.rate_gains.roll.d,
            ConfigKey::PitchP => self.tuning.rate_gains.pitch.p,
            ConfigKey::PitchI => self.tuning.rate_gains.pitch.i,
            ConfigKey::PitchD => self.tuning.rate_gains.pitch.d,
            ConfigKey::YawP => self.tuning.rate_gains.yaw.p,
            ConfigKey::YawI => self.tuning.rate_gains.yaw.i,
            ConfigKey::YawD => self.tuning.rate_gains.yaw.d,
            ConfigKey::ImuLpfHz => self.tuning.imu_lpf_hz,
            ConfigKey::LogRateDivisor => self.log_rate_divisor as f32,
            ConfigKey::RcDeadband => self.tuning.rc_rates.deadband as f32,
            ConfigKey::RollCenterRate => self.tuning.rc_rates.roll.center_sensitivity_dps,
            ConfigKey::RollMaxRate => self.tuning.rc_rates.roll.max_rate_dps,
            ConfigKey::RollExpo => self.tuning.rc_rates.roll.expo,
            ConfigKey::PitchCenterRate => self.tuning.rc_rates.pitch.center_sensitivity_dps,
            ConfigKey::PitchMaxRate => self.tuning.rc_rates.pitch.max_rate_dps,
            ConfigKey::PitchExpo => self.tuning.rc_rates.pitch.expo,
            ConfigKey::YawCenterRate => self.tuning.rc_rates.yaw.center_sensitivity_dps,
            ConfigKey::YawMaxRate => self.tuning.rc_rates.yaw.max_rate_dps,
            ConfigKey::YawExpo => self.tuning.rc_rates.yaw.expo,
            ConfigKey::RcMap => self.tuning.rc_map.stick_order() as f32,
            ConfigKey::RcArmChannel => self.tuning.rc_map.arm_channel() as f32,
            ConfigKey::RcProtocol => self.rc_protocol as u8 as f32,
            ConfigKey::MotorMap => self.tuning.motor_map.to_config() as f32,
        }
    }

    pub fn encode(self) -> [u8; STORED_CONFIG_LEN] {
        let values = [
            self.tuning.rate_gains.roll.p,
            self.tuning.rate_gains.roll.i,
            self.tuning.rate_gains.roll.d,
            self.tuning.rate_gains.pitch.p,
            self.tuning.rate_gains.pitch.i,
            self.tuning.rate_gains.pitch.d,
            self.tuning.rate_gains.yaw.p,
            self.tuning.rate_gains.yaw.i,
            self.tuning.rate_gains.yaw.d,
            self.tuning.imu_lpf_hz,
        ];
        let mut output = [0u8; STORED_CONFIG_LEN];
        for (index, value) in values.iter().enumerate() {
            let start = index * 4;
            output[start..start + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        output[40..42].copy_from_slice(&self.log_rate_divisor.to_le_bytes());
        output[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION.to_le_bytes());
        let rate_values = [
            self.tuning.rc_rates.roll.center_sensitivity_dps,
            self.tuning.rc_rates.roll.max_rate_dps,
            self.tuning.rc_rates.roll.expo,
            self.tuning.rc_rates.pitch.center_sensitivity_dps,
            self.tuning.rc_rates.pitch.max_rate_dps,
            self.tuning.rc_rates.pitch.expo,
            self.tuning.rc_rates.yaw.center_sensitivity_dps,
            self.tuning.rc_rates.yaw.max_rate_dps,
            self.tuning.rc_rates.yaw.expo,
        ];
        for (index, value) in rate_values.iter().enumerate() {
            let start = 44 + index * 4;
            output[start..start + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        output[80..82].copy_from_slice(&self.tuning.rc_rates.deadband.to_le_bytes());
        output[V3_STORED_CONFIG_LEN..V4_STORED_CONFIG_LEN].copy_from_slice(
            &self
                .serial_bindings
                .map_or(SERIAL_BINDINGS_UNSET, |bindings| bindings.encode()),
        );
        output[RC_FIELDS_OFFSET..RC_FIELDS_OFFSET + 2]
            .copy_from_slice(&self.tuning.rc_map.stick_order().to_le_bytes());
        output[RC_FIELDS_OFFSET + 2] = self.tuning.rc_map.arm_channel();
        output[RC_FIELDS_OFFSET + 3] = self.rc_protocol as u8;
        output[MOTOR_MAP_OFFSET..STORED_CONFIG_LEN]
            .copy_from_slice(&self.tuning.motor_map.to_config().to_le_bytes());
        output
    }

    pub fn decode(input: &[u8]) -> Option<Self> {
        if ![
            LEGACY_STORED_CONFIG_LEN,
            V3_STORED_CONFIG_LEN,
            V4_STORED_CONFIG_LEN,
            V5_STORED_CONFIG_LEN,
            STORED_CONFIG_LEN,
        ]
        .contains(&input.len())
        {
            return None;
        }
        let mut values = [0.0f32; 10];
        for (index, value) in values.iter_mut().enumerate() {
            let start = index * 4;
            *value = f32::from_bits(u32::from_le_bytes([
                input[start],
                input[start + 1],
                input[start + 2],
                input[start + 3],
            ]));
        }
        // Version 1 payloads carry no version field; they are coefficients.
        let mut schema_version = STORED_CONFIG_SCHEMA_VERSION_ALPHA;
        let rc_rates = if input.len() == LEGACY_STORED_CONFIG_LEN {
            RC_RATE_PROFILE
        } else {
            let stored_version = u16::from_le_bytes([input[42], input[43]]);
            let expected_len = match stored_version {
                STORED_CONFIG_SCHEMA_VERSION => STORED_CONFIG_LEN,
                STORED_CONFIG_SCHEMA_VERSION_RC => V5_STORED_CONFIG_LEN,
                STORED_CONFIG_SCHEMA_VERSION_BINDINGS => V4_STORED_CONFIG_LEN,
                STORED_CONFIG_SCHEMA_VERSION_CORNER | STORED_CONFIG_SCHEMA_VERSION_ALPHA => {
                    V3_STORED_CONFIG_LEN
                }
                _ => return None,
            };
            if input.len() != expected_len {
                return None;
            }
            schema_version = stored_version;
            let mut rate_values = [0.0f32; 9];
            for (index, value) in rate_values.iter_mut().enumerate() {
                let start = 44 + index * 4;
                *value = f32::from_bits(u32::from_le_bytes([
                    input[start],
                    input[start + 1],
                    input[start + 2],
                    input[start + 3],
                ]));
            }
            RcRateProfile {
                roll: ActualRateAxis::new(rate_values[0], rate_values[1], rate_values[2]),
                pitch: ActualRateAxis::new(rate_values[3], rate_values[4], rate_values[5]),
                yaw: ActualRateAxis::new(rate_values[6], rate_values[7], rate_values[8]),
                deadband: u16::from_le_bytes([input[80], input[81]]),
            }
        };
        let (rc_map, rc_protocol) = if schema_version >= STORED_CONFIG_SCHEMA_VERSION_RC {
            let fields = &input[RC_FIELDS_OFFSET..V5_STORED_CONFIG_LEN];
            (
                RcChannelMap::from_config(u16::from_le_bytes([fields[0], fields[1]]), fields[2])?,
                RcProtocol::from_u8(fields[3])?,
            )
        } else {
            (RcChannelMap::AETR_ARM_CH9, RcProtocol::Sbus)
        };
        let motor_map = if schema_version == STORED_CONFIG_SCHEMA_VERSION {
            MotorOutputMap::from_config(u16::from_le_bytes([
                input[MOTOR_MAP_OFFSET],
                input[MOTOR_MAP_OFFSET + 1],
            ]))?
        } else {
            MotorOutputMap::IDENTITY
        };
        let candidate = Self {
            tuning: TuningProfile {
                rate_gains: RateControllerGains {
                    roll: PidGains {
                        p: values[0],
                        i: values[1],
                        d: values[2],
                    },
                    pitch: PidGains {
                        p: values[3],
                        i: values[4],
                        d: values[5],
                    },
                    yaw: PidGains {
                        p: values[6],
                        i: values[7],
                        d: values[8],
                    },
                },
                // Older payloads stored a one-pole coefficient. Convert it at
                // the rate it was authored for, so the filter a stored tune
                // had is the filter it keeps.
                imu_lpf_hz: if schema_version != STORED_CONFIG_SCHEMA_VERSION_ALPHA {
                    values[9]
                } else {
                    gyro_lpf_corner_hz(values[9], LEGACY_LPF_SAMPLE_RATE_HZ)
                },
                rc_rates,
                rc_map,
                motor_map,
            },
            log_rate_divisor: u16::from_le_bytes([input[40], input[41]]),
            rc_protocol,
            serial_bindings: match input.get(V3_STORED_CONFIG_LEN..V4_STORED_CONFIG_LEN) {
                Some(bytes) if bytes.len() == SERIAL_PORT_SLOTS => {
                    let bytes: &[u8; SERIAL_PORT_SLOTS] = bytes.try_into().ok()?;
                    if *bytes == SERIAL_BINDINGS_UNSET {
                        None
                    } else {
                        // A function this firmware does not know rejects the
                        // whole payload, like any other invalid field.
                        Some(SerialBindings::decode(bytes)?)
                    }
                }
                _ => None,
            },
        };
        if candidate.log_rate_divisor == 0
            || candidate.log_rate_divisor > 16
            || candidate.tuning.sanitized() != candidate.tuning
        {
            None
        } else {
            Some(candidate)
        }
    }

    #[cfg(feature = "mspv2_configurator")]
    pub fn to_rpc(self) -> rpc::ConfigV1 {
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

    #[cfg(feature = "mspv2_configurator")]
    pub fn from_rpc(config: rpc::ConfigV1) -> Result<Self, rpc::ConfigFieldId> {
        Self::first_hop_default().apply_rpc(config)
    }

    /// Applies the legacy configurator schema without discarding newer fields
    /// that are currently available through the USB text CLI only.
    #[cfg(feature = "mspv2_configurator")]
    pub fn apply_rpc(mut self, config: rpc::ConfigV1) -> Result<Self, rpc::ConfigFieldId> {
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

    #[cfg(feature = "mspv2_configurator")]
    pub fn crc32(self) -> u32 {
        ferrowasp_core::blackbox::crc32(&self.encode())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageLayout {
    pub capacity_bytes: u32,
    pub config_slot_addresses: [u32; 2],
    pub log_start_address: u32,
    pub log_page_count: u32,
}

impl StorageLayout {
    pub const fn new(capacity_bytes: u32) -> Option<Self> {
        if capacity_bytes <= LOG_START_ADDRESS + CONFIG_SECTOR_SIZE
            || capacity_bytes > 0x0100_0000
            || !capacity_bytes.is_multiple_of(CONFIG_SECTOR_SIZE)
        {
            return None;
        }
        Some(Self {
            capacity_bytes,
            config_slot_addresses: [0, CONFIG_SECTOR_SIZE],
            log_start_address: LOG_START_ADDRESS,
            // The last sector holds the sync ledger, not log pages.
            log_page_count: (capacity_bytes - LOG_START_ADDRESS - CONFIG_SECTOR_SIZE)
                / FLASH_PAGE_LEN as u32,
        })
    }

    pub const fn log_page_address(self, page_index: u32) -> Option<u32> {
        if page_index >= self.log_page_count {
            None
        } else {
            Some(self.log_start_address + page_index * FLASH_PAGE_LEN as u32)
        }
    }

    /// Sectors a log erase clears: every log sector plus the sync ledger.
    pub const fn log_sector_count(self) -> u32 {
        (self.capacity_bytes - self.log_start_address) / CONFIG_SECTOR_SIZE
    }

    /// The last sector of the flash, which records synced flights.
    pub const fn sync_ledger_address(self) -> u32 {
        self.capacity_bytes - CONFIG_SECTOR_SIZE
    }

    /// The sector a log erase clears at `step`, from the end of the flash down.
    ///
    /// The ledger goes first so that a power cut part-way through leaves old
    /// flights looking unsynced. Erasing upward would leave the old ledger
    /// marking the next flights, whose identifiers restart at 1, as synced.
    pub const fn log_erase_sector_address(self, step: u32) -> Option<u32> {
        if step >= self.log_sector_count() {
            None
        } else {
            Some(self.capacity_bytes - (step + 1) * CONFIG_SECTOR_SIZE)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssembleError {
    NotRecording,
    PageNotConsumed,
    Format,
}

pub struct PageAssembler {
    records: [FlightRecord; RECORDS_PER_PAGE],
    record_count: usize,
    flight_id: u32,
    page_sequence: u32,
    recording: bool,
    boot_session_start_pending: bool,
    ready_page: [u8; FLASH_PAGE_LEN],
    page_ready: bool,
}

impl PageAssembler {
    pub const fn new() -> Self {
        Self {
            records: [FlightRecord {
                timestamp_us: 0,
                control_sequence: 0,
                imu_sequence: 0,
                flags: 0,
                raw_gyro_dps10: [0; 3],
                filtered_gyro_dps10: [0; 3],
                command_dps10: [0; 3],
                pid: [0; 3],
                throttle: 0,
                motors: [0; 4],
            }; RECORDS_PER_PAGE],
            record_count: 0,
            flight_id: 0,
            page_sequence: 0,
            recording: false,
            boot_session_start_pending: false,
            ready_page: [0xff; FLASH_PAGE_LEN],
            page_ready: false,
        }
    }

    pub fn start(&mut self, flight_id: u32, boot_session_start: bool) {
        self.record_count = 0;
        self.flight_id = flight_id;
        self.page_sequence = 0;
        self.recording = true;
        self.boot_session_start_pending = boot_session_start;
        self.page_ready = false;
    }

    pub const fn recording(&self) -> bool {
        self.recording
    }

    /// Returns `true` when a complete page is available through
    /// [`Self::take_ready_page`]. The caller must consume that page before
    /// submitting another record.
    pub fn push(&mut self, mut record: FlightRecord) -> Result<bool, AssembleError> {
        if !self.recording {
            return Err(AssembleError::NotRecording);
        }
        if self.page_ready {
            return Err(AssembleError::PageNotConsumed);
        }
        if self.boot_session_start_pending {
            record.flags |= FLIGHT_RECORD_FLAG_BOOT_SESSION_START;
            self.boot_session_start_pending = false;
        }
        self.records[self.record_count] = record;
        self.record_count += 1;
        if self.record_count < RECORDS_PER_PAGE {
            return Ok(false);
        }
        self.finish_page()?;
        Ok(true)
    }

    pub fn stop(&mut self) -> Result<bool, AssembleError> {
        self.recording = false;
        if self.record_count == 0 {
            Ok(self.page_ready)
        } else {
            self.finish_page()?;
            Ok(true)
        }
    }

    /// The flight not yet wholly on flash: still recording, its last page
    /// not taken, or, with `page_pending`, a taken page not yet written.
    /// Log sync leaves it alone until it is complete.
    pub const fn unfinished_flight(&self, page_pending: bool) -> Option<u32> {
        if self.flight_id != 0 && (self.recording || self.page_ready || page_pending) {
            Some(self.flight_id)
        } else {
            None
        }
    }

    pub fn take_ready_page(&mut self) -> Option<[u8; FLASH_PAGE_LEN]> {
        if !self.page_ready {
            return None;
        }
        self.page_ready = false;
        Some(self.ready_page)
    }

    fn finish_page(&mut self) -> Result<(), AssembleError> {
        if self.page_ready {
            return Err(AssembleError::PageNotConsumed);
        }
        self.ready_page = encode_page(
            self.flight_id,
            self.page_sequence,
            &self.records[..self.record_count],
        )
        .map_err(|_| AssembleError::Format)?;
        self.page_ready = true;
        self.page_sequence = self.page_sequence.wrapping_add(1);
        self.record_count = 0;
        Ok(())
    }
}

impl Default for PageAssembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Sequence comparison for two copy-on-write configuration slots.
pub const fn sequence_is_newer(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < 0x8000_0000
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

/// Produces the deterministic pattern used by destructive scratch-sector tests.
pub fn scratch_test_page() -> [u8; FLASH_PAGE_LEN] {
    let mut page = [0u8; FLASH_PAGE_LEN];
    for (index, byte) in page.iter_mut().enumerate() {
        *byte = (index as u8).rotate_left(1) ^ 0xa5;
    }
    page[..8].copy_from_slice(b"FWTEST01");
    page
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drone_toolbox::gyro_lpf_alpha;
    use ferrowasp_core::blackbox::{decode_page, record_from_page};

    fn record(sequence: u32) -> FlightRecord {
        FlightRecord {
            control_sequence: sequence,
            ..FlightRecord::default()
        }
    }

    #[test]
    fn sixteen_megabyte_layout_reserves_config_and_scratch_sectors() {
        let layout = StorageLayout::new(16 * 1024 * 1024).unwrap();
        assert_eq!(layout.config_slot_addresses, [0, 4096]);
        assert_eq!(SCRATCH_SECTOR_ADDRESS, 8192);
        assert_eq!(layout.log_start_address, 12288);
        assert_eq!(layout.log_page_address(0), Some(12288));
        assert_eq!(
            layout.log_page_address(layout.log_page_count - 1),
            Some(16 * 1024 * 1024 - 4096 - 256)
        );
        assert_eq!(layout.log_page_address(layout.log_page_count), None);
        assert_eq!(layout.sync_ledger_address(), 16 * 1024 * 1024 - 4096);
    }

    #[test]
    fn log_erase_clears_the_ledger_first_then_walks_down_to_the_log_start() {
        let layout = StorageLayout::new(64 * 1024).unwrap();
        let steps = layout.log_sector_count();
        assert_eq!(
            layout.log_erase_sector_address(0),
            Some(layout.sync_ledger_address())
        );
        assert_eq!(
            layout.log_erase_sector_address(steps - 1),
            Some(layout.log_start_address)
        );
        assert_eq!(layout.log_erase_sector_address(steps), None);
    }

    /// A 64 KiB flash holding `pages` from the log start: `Some((flight,
    /// sequence))` for a whole page, `None` for one torn by a power cut.
    fn log_with(layout: StorageLayout, pages: &[Option<(u32, u32)>]) -> [u8; 64 * 1024] {
        let mut flash = [0xff_u8; 64 * 1024];
        for (index, entry) in pages.iter().enumerate() {
            let address = layout.log_page_address(index as u32).unwrap() as usize;
            let (flight, sequence) = entry.unwrap_or((9, 9));
            let mut page = encode_page(flight, sequence, &[record(0); RECORDS_PER_PAGE]).unwrap();
            if entry.is_none() {
                // The program stopped part-way: the rest is still erased.
                page[100..].fill(0xff);
            }
            flash[address..address + FLASH_PAGE_LEN].copy_from_slice(&page);
        }
        flash
    }

    fn scan_flash(layout: StorageLayout, flash: &[u8]) -> (u32, u32, bool) {
        scan_log(layout, |address, output: &mut [u8]| {
            let start = address as usize;
            output.copy_from_slice(&flash[start..start + output.len()]);
            Ok::<(), ()>(())
        })
        .unwrap()
    }

    #[test]
    fn a_torn_page_ends_its_flight_and_logging_carries_on_after_it() {
        let layout = StorageLayout::new(64 * 1024).unwrap();
        let torn = [Some((1, 0)), Some((1, 1)), Some((1, 2)), None];
        assert_eq!(scan_flash(layout, &log_with(layout, &torn)), (4, 2, true));

        // The next flight, written after the torn page. The old scan stopped
        // at the torn page here, found data after it, and stopped logging.
        let mut later = torn.to_vec();
        later.extend([Some((2, 0)), Some((2, 1))]);
        assert_eq!(scan_flash(layout, &log_with(layout, &later)), (6, 3, true));
    }

    #[test]
    fn the_scan_tells_an_empty_full_or_foreign_region_apart() {
        let layout = StorageLayout::new(64 * 1024).unwrap();
        assert_eq!(scan_flash(layout, &log_with(layout, &[])), (0, 1, true));

        let mut foreign = [0xff_u8; 64 * 1024];
        let start = layout.log_start_address as usize;
        foreign[start..start + 3000].fill(0x5a);
        assert_eq!(scan_flash(layout, &foreign), (0, 1, false));

        // Two pages that do not decode in a row are not one torn page.
        let garbled = [Some((1, 0)), None, None];
        assert_eq!(
            scan_flash(layout, &log_with(layout, &garbled)),
            (0, 1, false)
        );

        let full: heapless::Vec<_, 256> = (0..layout.log_page_count)
            .map(|sequence| Some((4, sequence)))
            .collect();
        assert_eq!(
            scan_flash(layout, &log_with(layout, &full)),
            (layout.log_page_count, 5, false)
        );
    }

    #[test]
    fn assembler_emits_every_five_records_and_flushes_partial_page() {
        let mut assembler = PageAssembler::new();
        assembler.start(12, false);
        for sequence in 0..4 {
            assert_eq!(assembler.push(record(sequence)), Ok(false));
        }
        assert_eq!(assembler.push(record(4)), Ok(true));
        let full = assembler.take_ready_page().unwrap();
        assert_eq!(decode_page(&full).unwrap().record_count, 5);
        assert_eq!(record_from_page(&full, 4).unwrap().control_sequence, 4);

        assembler.push(record(5)).unwrap();
        assert!(assembler.stop().unwrap());
        let partial = assembler.take_ready_page().unwrap();
        assert_eq!(decode_page(&partial).unwrap().record_count, 1);
    }

    #[test]
    fn assembler_marks_only_first_record_of_first_flight_after_boot() {
        let mut assembler = PageAssembler::new();
        assembler.start(12, true);
        for sequence in 0..5 {
            assembler.push(record(sequence)).unwrap();
        }
        let first_page = assembler.take_ready_page().unwrap();
        assert_ne!(
            record_from_page(&first_page, 0).unwrap().flags & FLIGHT_RECORD_FLAG_BOOT_SESSION_START,
            0
        );
        assert_eq!(
            record_from_page(&first_page, 1).unwrap().flags & FLIGHT_RECORD_FLAG_BOOT_SESSION_START,
            0
        );
        assembler.stop().unwrap();

        assembler.start(13, false);
        assembler.push(record(5)).unwrap();
        assembler.stop().unwrap();
        let second_flight = assembler.take_ready_page().unwrap();
        assert_eq!(
            record_from_page(&second_flight, 0).unwrap().flags
                & FLIGHT_RECORD_FLAG_BOOT_SESSION_START,
            0
        );
    }

    #[test]
    fn a_flight_is_unfinished_until_its_last_page_is_written() {
        let mut assembler = PageAssembler::new();
        assert_eq!(assembler.unfinished_flight(false), None);
        assembler.start(4, false);
        assert_eq!(assembler.unfinished_flight(false), Some(4));
        assembler.push(record(1)).unwrap();
        assembler.stop().unwrap();
        assert_eq!(assembler.unfinished_flight(false), Some(4));
        assembler.take_ready_page().unwrap();
        assert_eq!(assembler.unfinished_flight(true), Some(4));
        assert_eq!(assembler.unfinished_flight(false), None);
    }

    #[test]
    fn configuration_sequence_comparison_handles_wrap() {
        assert!(sequence_is_newer(11, 10));
        assert!(!sequence_is_newer(10, 10));
        assert!(sequence_is_newer(1, u32::MAX));
        assert!(!sequence_is_newer(u32::MAX, 1));
    }

    #[test]
    fn usb_cli_requires_explicit_erase_confirmation() {
        assert_eq!(parse_command("flash info"), Ok(StorageCommand::FlashInfo));
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
    fn stored_config_round_trips_and_rejects_unsafe_values() {
        let mut config = StoredConfig::first_hop_default();
        assert!(config.set(ConfigKey::PitchP, 0.30));
        assert!(config.set(ConfigKey::LogRateDivisor, 2.0));
        assert!(config.set(ConfigKey::RcDeadband, 12.0));
        assert!(config.set(ConfigKey::RollMaxRate, 600.0));
        assert!(config.set(ConfigKey::RollCenterRate, 120.0));
        assert!(config.set(ConfigKey::RollExpo, 0.6));
        assert!(!config.set(ConfigKey::RollP, 100.0));
        assert!(!config.set(ConfigKey::RcDeadband, 12.5));
        assert!(!config.set(ConfigKey::RollCenterRate, 700.0));
        assert!(!config.set(ConfigKey::RollMaxRate, 100.0));
        assert!(!config.set(ConfigKey::RollExpo, 1.1));
        let encoded = config.encode();
        let decoded = StoredConfig::decode(&encoded).unwrap();
        assert_eq!(decoded.get(ConfigKey::PitchP), 0.30);
        assert_eq!(decoded.log_rate_divisor, 2);
        assert_eq!(decoded.get(ConfigKey::RcDeadband), 12.0);
        assert_eq!(decoded.get(ConfigKey::RollCenterRate), 120.0);
        assert_eq!(decoded.get(ConfigKey::RollMaxRate), 600.0);
        assert_eq!(decoded.get(ConfigKey::RollExpo), 0.6);

        let mut corrupt = encoded;
        corrupt[40] = 0;
        corrupt[41] = 0;
        assert_eq!(StoredConfig::decode(&corrupt), None);
    }

    /// A tune saved by the previous firmware must keep the filter it had.
    ///
    /// Version 2 stored a one-pole coefficient; version 3 stores the corner.
    /// Reading a version 2 payload converts at the rate the coefficient was
    /// authored for, or every saved tune silently changes.
    #[test]
    fn a_version_two_coefficient_survives_as_the_same_filter() {
        let mut payload = [0u8; V3_STORED_CONFIG_LEN];
        payload
            .copy_from_slice(&StoredConfig::first_hop_default().encode()[..V3_STORED_CONFIG_LEN]);
        payload[36..40].copy_from_slice(&0.55f32.to_bits().to_le_bytes());
        payload[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_ALPHA.to_le_bytes());

        let decoded = StoredConfig::decode(&payload).expect("version 2 must still be readable");
        let corner = decoded.tuning.imu_lpf_hz;
        assert!(
            (corner - 50.8).abs() < 0.2,
            "0.55 at 400 Hz is a 50.8 Hz corner, decoded {corner}"
        );
        let alpha = gyro_lpf_alpha(corner, LEGACY_LPF_SAMPLE_RATE_HZ);
        assert!(
            (alpha - 0.55).abs() < 0.005,
            "round trip should return 0.55, got {alpha}"
        );
    }

    /// The point of storing a frequency: it means the same filter at any rate.
    #[test]
    fn a_stored_corner_holds_its_meaning_across_loop_rates() {
        let corner = 50.8;
        let at_400 = gyro_lpf_alpha(corner, 400.0);
        let at_1000 = gyro_lpf_alpha(corner, 1000.0);
        assert!(
            at_1000 < at_400,
            "a faster loop needs a smaller coefficient"
        );
        for (alpha, rate) in [(at_400, 400.0), (at_1000, 1000.0)] {
            let back = gyro_lpf_corner_hz(alpha, rate);
            assert!((back - corner).abs() < 0.1, "{rate} Hz -> {back}");
        }
    }

    #[test]
    fn serial_bindings_round_trip_and_older_payloads_use_board_defaults() {
        let mut config = StoredConfig::first_hop_default();
        let encoded = config.encode();
        assert_eq!(
            StoredConfig::decode(&encoded).unwrap().serial_bindings,
            None
        );

        config.serial_bindings = Some(
            SerialBindings::none()
                .with(LogicalSerialPort::Uart2, SerialFunction::RcInput)
                .with(LogicalSerialPort::Uart4, SerialFunction::EscTelemetry),
        );
        let encoded = config.encode();
        assert_eq!(StoredConfig::decode(&encoded), Some(config));

        let mut version_three = [0u8; V3_STORED_CONFIG_LEN];
        version_three.copy_from_slice(&encoded[..V3_STORED_CONFIG_LEN]);
        version_three[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_CORNER.to_le_bytes());
        let decoded =
            StoredConfig::decode(&version_three).expect("version 3 must still be readable");
        assert_eq!(decoded.serial_bindings, None);
        assert_eq!(decoded.tuning, config.tuning);

        let mut unknown_function = encoded;
        unknown_function[V3_STORED_CONFIG_LEN] = 0x40;
        assert_eq!(StoredConfig::decode(&unknown_function), None);
        // A version 4 header on a version 3 length is not trusted.
        assert_eq!(StoredConfig::decode(&encoded[..V3_STORED_CONFIG_LEN]), None);
    }

    /// A configuration saved before the map was configurable armed on
    /// channel 9 over SBUS. Reading it must keep both: moving the arm switch
    /// under a pilot who has not changed their radio could arm from another
    /// switch.
    #[test]
    fn a_version_four_config_keeps_its_bindings_the_channel_nine_arm_switch_and_sbus() {
        let mut config = StoredConfig::first_hop_default();
        config.serial_bindings =
            Some(SerialBindings::none().with(LogicalSerialPort::Uart2, SerialFunction::RcInput));
        let mut version_four = [0u8; V4_STORED_CONFIG_LEN];
        version_four.copy_from_slice(&config.encode()[..V4_STORED_CONFIG_LEN]);
        version_four[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_BINDINGS.to_le_bytes());

        let decoded =
            StoredConfig::decode(&version_four).expect("version 4 must still be readable");
        assert_eq!(decoded.serial_bindings, config.serial_bindings);
        assert_eq!(decoded.tuning.rc_map, RcChannelMap::AETR_ARM_CH9);
        assert_eq!(decoded.get(ConfigKey::RcArmChannel), 9.0);
        assert_eq!(decoded.rc_protocol, RcProtocol::Sbus);
        // A version 5 header on a version 4 length is not trusted.
        let mut short = version_four;
        short[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION.to_le_bytes());
        assert_eq!(StoredConfig::decode(&short), None);
    }

    #[test]
    fn rc_map_and_protocol_are_configured_saved_and_validated() {
        let mut config = StoredConfig::first_hop_default();
        assert_eq!(config.get(ConfigKey::RcMap), 1234.0);
        assert_eq!(config.get(ConfigKey::RcArmChannel), 9.0);
        assert_eq!(config.get(ConfigKey::RcProtocol), 0.0);
        assert!(config.set(ConfigKey::RcMap, 2314.0));
        assert!(config.set(ConfigKey::RcArmChannel, 5.0));
        assert!(config.set(ConfigKey::RcProtocol, 1.0));
        assert!(!config.set(ConfigKey::RcMap, 1224.0));
        assert!(!config.set(ConfigKey::RcMap, 1234.5));
        assert!(!config.set(ConfigKey::RcArmChannel, 4.0));
        assert!(!config.set(ConfigKey::RcArmChannel, 17.0));
        assert!(!config.set(ConfigKey::RcProtocol, 2.0));

        let encoded = config.encode();
        let decoded = StoredConfig::decode(&encoded).unwrap();
        assert_eq!(decoded, config);
        assert_eq!(decoded.rc_protocol, RcProtocol::Crsf);
        assert_eq!(decoded.tuning.rc_map.stick_order(), 2314);

        let mut bad_protocol = encoded;
        bad_protocol[STORED_CONFIG_LEN - 1] = 7;
        assert_eq!(StoredConfig::decode(&bad_protocol), None);
        let mut bad_map = encoded;
        bad_map[RC_FIELDS_OFFSET..RC_FIELDS_OFFSET + 2].copy_from_slice(&1224u16.to_le_bytes());
        assert_eq!(StoredConfig::decode(&bad_map), None);
    }

    #[test]
    fn motor_order_is_saved_and_older_payloads_keep_the_board_wiring() {
        let mut config = StoredConfig::first_hop_default();
        assert_eq!(config.get(ConfigKey::MotorMap), 1234.0);
        assert!(config.set(ConfigKey::MotorMap, 2143.0));
        assert!(!config.set(ConfigKey::MotorMap, 1123.0));
        assert!(!config.set(ConfigKey::MotorMap, 1235.0));
        let encoded = config.encode();
        assert_eq!(StoredConfig::decode(&encoded), Some(config));

        let mut version_five = [0u8; V5_STORED_CONFIG_LEN];
        version_five.copy_from_slice(&encoded[..V5_STORED_CONFIG_LEN]);
        version_five[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_RC.to_le_bytes());
        let decoded =
            StoredConfig::decode(&version_five).expect("version 5 must still be readable");
        assert_eq!(decoded.tuning.motor_map, MotorOutputMap::IDENTITY);
        assert_eq!(decoded.tuning.rc_map, config.tuning.rc_map);

        let mut bad = encoded;
        bad[MOTOR_MAP_OFFSET..STORED_CONFIG_LEN].copy_from_slice(&1123u16.to_le_bytes());
        assert_eq!(StoredConfig::decode(&bad), None);
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

    #[test]
    fn legacy_stored_config_migrates_with_current_rc_defaults() {
        let current = StoredConfig::first_hop_default().encode();
        let mut legacy = [0u8; LEGACY_STORED_CONFIG_LEN];
        legacy.copy_from_slice(&current[..LEGACY_STORED_CONFIG_LEN]);
        legacy[42..44].fill(0);
        // A real legacy payload holds a one-pole coefficient here, not the
        // corner the current default encodes.
        legacy[36..40].copy_from_slice(&0.55f32.to_bits().to_le_bytes());

        let migrated = StoredConfig::decode(&legacy).unwrap();

        assert_eq!(migrated.tuning.rc_rates, RC_RATE_PROFILE);
        assert_eq!(migrated.log_rate_divisor, 1);
        // The coefficient becomes the corner it always described.
        assert!((migrated.tuning.imu_lpf_hz - 50.8).abs() < 0.2);
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
        invalid.imu_lpf_hz = 1.1;
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
