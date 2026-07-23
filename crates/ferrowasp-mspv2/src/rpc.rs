use heapless::Vec;
use serde::{Deserialize, Serialize};

use crate::MAX_PAYLOAD_LEN;

pub const MSP2_FWSP_RPC: u16 = 0x7a00;
pub const FWSP_RPC_VERSION: u16 = 1;
pub const MAX_BLACKBOX_CHUNK: usize = 512;
pub const MAX_BLACKBOX_LIST_ENTRIES: usize = 16;
pub const CONFIG_SCHEMA_VERSION: u16 = 1;

pub mod capabilities {
    pub const CONFIG_READ: u32 = 1 << 0;
    pub const CONFIG_WRITE: u32 = 1 << 1;
    pub const CONFIG_RESET: u32 = 1 << 2;
    pub const BLACKBOX_LIST: u32 = 1 << 3;
    pub const BLACKBOX_DOWNLOAD: u32 = 1 << 4;
    pub const BLACKBOX_ERASE: u32 = 1 << 5;
    pub const REBOOT: u32 = 1 << 6;
    pub const REBOOT_DFU: u32 = 1 << 7;
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfigV1 {
    pub roll_p: f32,
    pub roll_i: f32,
    pub roll_d: f32,
    pub pitch_p: f32,
    pub pitch_i: f32,
    pub pitch_d: f32,
    pub yaw_p: f32,
    pub yaw_i: f32,
    pub yaw_d: f32,
    pub imu_lpf_alpha: f32,
    pub log_rate_divisor: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlackboxId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RebootMode {
    Normal,
    RomDfu,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Hello,
    GetConfig,
    StageConfig(ConfigV1),
    CommitConfig {
        expected_staged_crc32: u32,
    },
    ResetConfigToDefaults,
    ListBlackboxes,
    GetBlackboxInfo {
        id: BlackboxId,
    },
    ReadBlackboxChunk {
        id: BlackboxId,
        offset: u32,
        requested_length: u16,
    },
    EraseBlackbox {
        id: BlackboxId,
    },
    Reboot {
        mode: RebootMode,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcRequest {
    pub protocol_version: u16,
    pub request_id: u16,
    pub operation: Request,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcResponse {
    pub protocol_version: u16,
    pub request_id: u16,
    pub result: RpcResult,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// The no_std transport intentionally keeps its largest bounded response inline;
// allocation or pointer ownership would weaken the fixed-memory contract.
#[allow(clippy::large_enum_variant)]
pub enum RpcResult {
    Ok(Response),
    Err(ErrorDetail),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeviceError {
    UnsupportedProtocolVersion,
    UnsupportedOperation,
    InvalidPayload,
    InvalidConfiguration,
    ConfigurationConflict,
    Armed,
    Busy,
    RebootRequired,
    StorageUnavailable,
    BlackboxNotFound,
    InvalidOffset,
    ReadFailure,
    WriteFailure,
    ChecksumMismatch,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: DeviceError,
    pub field: Option<ConfigFieldId>,
    pub argument: Option<u32>,
}

impl ErrorDetail {
    pub const fn new(code: DeviceError) -> Self {
        Self {
            code,
            field: None,
            argument: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConfigFieldId {
    RollP,
    RollI,
    RollD,
    PitchP,
    PitchI,
    PitchD,
    YawP,
    YawI,
    YawD,
    ImuLpfAlpha,
    LogRateDivisor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum McuKind {
    Stm32F405,
    Unknown(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub protocol_version: u16,
    pub firmware_version: [u8; 3],
    pub git_revision: [u8; 8],
    pub board_id: [u8; 16],
    pub board_id_len: u8,
    pub mcu: McuKind,
    pub device_serial: [u8; 12],
    pub armed: bool,
    pub capabilities: u32,
    pub max_blackbox_chunk: u16,
    pub config_schema_version: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub schema_version: u16,
    pub active_crc32: u32,
    pub persisted_crc32: u32,
    pub config: ConfigV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BlackboxState {
    Complete,
    Incomplete,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlackboxInfo {
    pub id: BlackboxId,
    pub size_bytes: u32,
    pub state: BlackboxState,
    pub created_unix_s: Option<u64>,
    pub file_crc32: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlackboxList {
    pub entries: Vec<BlackboxInfo, MAX_BLACKBOX_LIST_ENTRIES>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlackboxChunk {
    pub id: BlackboxId,
    pub offset: u32,
    pub data: Vec<u8, MAX_BLACKBOX_CHUNK>,
    pub chunk_crc32: u32,
    pub end_of_file: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Hello(DeviceInfo),
    Config(ConfigSnapshot),
    ConfigStaged {
        staged_crc32: u32,
        reboot_required: bool,
    },
    ConfigCommitted {
        persisted_crc32: u32,
        reboot_required: bool,
    },
    DefaultsRestored {
        persisted_crc32: u32,
        reboot_required: bool,
    },
    BlackboxList(BlackboxList),
    BlackboxInfo(BlackboxInfo),
    BlackboxChunk(BlackboxChunk),
    BlackboxErased,
    RebootAccepted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    InvalidPayload,
    PayloadTooLong,
}

pub fn decode_request(payload: &[u8]) -> Result<RpcRequest, CodecError> {
    postcard::from_bytes(payload).map_err(|_| CodecError::InvalidPayload)
}

pub fn encode_request<'a>(
    request: &RpcRequest,
    output: &'a mut [u8; MAX_PAYLOAD_LEN],
) -> Result<&'a [u8], CodecError> {
    postcard::to_slice(request, output)
        .map(|encoded| &*encoded)
        .map_err(|_| CodecError::PayloadTooLong)
}

pub fn encode_response<'a>(
    response: &RpcResponse,
    output: &'a mut [u8; MAX_PAYLOAD_LEN],
) -> Result<&'a [u8], CodecError> {
    postcard::to_slice(response, output)
        .map(|encoded| &*encoded)
        .map_err(|_| CodecError::PayloadTooLong)
}

pub fn ok(request_id: u16, response: Response) -> RpcResponse {
    RpcResponse {
        protocol_version: FWSP_RPC_VERSION,
        request_id,
        result: RpcResult::Ok(response),
    }
}

pub fn error(request_id: u16, code: DeviceError) -> RpcResponse {
    RpcResponse {
        protocol_version: FWSP_RPC_VERSION,
        request_id,
        result: RpcResult::Err(ErrorDetail::new(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ConfigV1 {
        ConfigV1 {
            roll_p: 0.2,
            roll_i: 0.0,
            roll_d: 0.0,
            pitch_p: 0.25,
            pitch_i: 0.0,
            pitch_d: 0.0,
            yaw_p: 0.3,
            yaw_i: 0.04,
            yaw_d: 0.0,
            imu_lpf_alpha: 0.55,
            log_rate_divisor: 1,
        }
    }

    #[test]
    fn request_round_trip_uses_the_versioned_postcard_envelope() {
        let request = RpcRequest {
            protocol_version: FWSP_RPC_VERSION,
            request_id: 42,
            operation: Request::StageConfig(config()),
        };
        let mut bytes = [0; MAX_PAYLOAD_LEN];
        let encoded = encode_request(&request, &mut bytes).unwrap();
        assert_eq!(decode_request(encoded), Ok(request));
    }

    #[test]
    fn maximum_blackbox_response_fits_in_one_msp_payload() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0xa5; MAX_BLACKBOX_CHUNK]).unwrap();
        let response = ok(
            u16::MAX,
            Response::BlackboxChunk(BlackboxChunk {
                id: BlackboxId(u32::MAX),
                offset: u32::MAX,
                data,
                chunk_crc32: u32::MAX,
                end_of_file: true,
            }),
        );
        let mut bytes = [0; MAX_PAYLOAD_LEN];
        assert!(encode_response(&response, &mut bytes).is_ok());
    }

    #[test]
    fn malformed_payload_is_rejected_without_panicking() {
        assert_eq!(decode_request(&[0xff; 8]), Err(CodecError::InvalidPayload));
    }
}
