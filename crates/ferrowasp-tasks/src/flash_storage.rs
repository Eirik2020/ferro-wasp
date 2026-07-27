//! Portable state and bounded channels for onboard SPI-NOR storage.

use ferrowasp_core::blackbox::{
    FLASH_PAGE_LEN, FLIGHT_RECORD_FLAG_BOOT_SESSION_START, FlightRecord, RECORDS_PER_PAGE,
    encode_page,
};
pub use ferrowasp_core::config::ConfigKey;
use heapless::String;
use heapless::spsc::{Consumer, Producer, Queue};

#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2::rpc;

use crate::drone_toolbox::{
    ActualRateAxis, PidGains, RC_RATE_PROFILE, RateControllerGains, RcRateProfile, TuningProfile,
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
    ConfigGet(ConfigKey),
    ConfigSet(ConfigKey, f32),
    ConfigSave,
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
        _ => Err(CommandParseError::UnknownCommand),
    }
}

pub const LEGACY_STORED_CONFIG_LEN: usize = 44;
pub const STORED_CONFIG_LEN: usize = 84;
const STORED_CONFIG_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredConfig {
    pub tuning: TuningProfile,
    pub log_rate_divisor: u16,
}

impl StoredConfig {
    pub const fn first_hop_default() -> Self {
        Self {
            tuning: TuningProfile::default_first_hop(),
            log_rate_divisor: 1,
        }
    }

    pub const fn foxeer_f405_v2_default() -> Self {
        Self {
            tuning: TuningProfile::default_foxeer_f405_v2(),
            log_rate_divisor: 1,
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
            ConfigKey::ImuLpfAlpha => candidate.tuning.imu_lpf_alpha = value,
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
            ConfigKey::ImuLpfAlpha => self.tuning.imu_lpf_alpha,
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
            self.tuning.imu_lpf_alpha,
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
        output
    }

    pub fn decode(input: &[u8]) -> Option<Self> {
        if input.len() != LEGACY_STORED_CONFIG_LEN && input.len() != STORED_CONFIG_LEN {
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
        let rc_rates = if input.len() == LEGACY_STORED_CONFIG_LEN {
            RC_RATE_PROFILE
        } else {
            if u16::from_le_bytes([input[42], input[43]]) != STORED_CONFIG_SCHEMA_VERSION {
                return None;
            }
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
                imu_lpf_alpha: values[9],
                rc_rates,
            },
            log_rate_divisor: u16::from_le_bytes([input[40], input[41]]),
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
            imu_lpf_alpha: self.get(ConfigKey::ImuLpfAlpha),
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
                ConfigKey::ImuLpfAlpha,
                config.imu_lpf_alpha,
                rpc::ConfigFieldId::ImuLpfAlpha,
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
        if capacity_bytes <= LOG_START_ADDRESS
            || capacity_bytes > 0x0100_0000
            || !capacity_bytes.is_multiple_of(CONFIG_SECTOR_SIZE)
        {
            return None;
        }
        Some(Self {
            capacity_bytes,
            config_slot_addresses: [0, CONFIG_SECTOR_SIZE],
            log_start_address: LOG_START_ADDRESS,
            log_page_count: (capacity_bytes - LOG_START_ADDRESS) / FLASH_PAGE_LEN as u32,
        })
    }

    pub const fn log_page_address(self, page_index: u32) -> Option<u32> {
        if page_index >= self.log_page_count {
            None
        } else {
            Some(self.log_start_address + page_index * FLASH_PAGE_LEN as u32)
        }
    }

    pub const fn log_sector_count(self) -> u32 {
        (self.capacity_bytes - self.log_start_address) / CONFIG_SECTOR_SIZE
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

#[cfg(test)]
mod tests {
    use super::*;
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
            Some(16 * 1024 * 1024 - 256)
        );
        assert_eq!(layout.log_page_address(layout.log_page_count), None);
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

    #[test]
    fn legacy_stored_config_migrates_with_current_rc_defaults() {
        let current = StoredConfig::first_hop_default().encode();
        let mut legacy = [0u8; LEGACY_STORED_CONFIG_LEN];
        legacy.copy_from_slice(&current[..LEGACY_STORED_CONFIG_LEN]);
        legacy[42..44].fill(0);

        let migrated = StoredConfig::decode(&legacy).unwrap();

        assert_eq!(migrated.tuning.rc_rates, RC_RATE_PROFILE);
        assert_eq!(migrated.log_rate_divisor, 1);
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
        invalid.imu_lpf_alpha = 1.1;
        assert_eq!(
            StoredConfig::from_rpc(invalid),
            Err(rpc::ConfigFieldId::ImuLpfAlpha)
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
