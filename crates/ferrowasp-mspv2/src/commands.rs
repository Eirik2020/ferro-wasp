//! Small read-only MSP interoperability subset.

use crate::{MspDirection, MspPacket, MspPacketError, encode};

pub const MSP_API_VERSION: u16 = 1;
pub const MSP_FC_VARIANT: u16 = 2;
pub const MSP_FC_VERSION: u16 = 3;
pub const MSP_BOARD_INFO: u16 = 4;
pub const MSP_BUILD_INFO: u16 = 5;
pub const MSP_STATUS: u16 = 101;
pub const MSP_UID: u16 = 160;

pub const MSP_PROTOCOL_VERSION: u8 = 0;
pub const MSP_API_MAJOR: u8 = 1;
pub const MSP_API_MINOR: u8 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommonInfo<'a> {
    pub firmware_version: [u8; 3],
    pub board_identifier: [u8; 4],
    pub board_name: &'a [u8],
    pub target_name: &'a [u8],
    pub build_date: [u8; 11],
    pub build_time: [u8; 8],
    pub git_revision: [u8; 7],
    pub uid: [u8; 12],
    pub cycle_time_us: u16,
    pub sensors: u16,
    pub armed: bool,
}

pub fn encode_common_response(
    request: &MspPacket,
    info: &CommonInfo<'_>,
    output: &mut [u8],
) -> Result<Option<usize>, MspPacketError> {
    let mut payload = [0u8; 96];
    let Some(len) = encode_common_payload(request, info, &mut payload) else {
        return Ok(None);
    };
    encode(
        MspDirection::FromFlightController,
        request.flags,
        request.function,
        &payload[..len],
        output,
    )
    .map(Some)
}

pub fn encode_common_payload(
    request: &MspPacket,
    info: &CommonInfo<'_>,
    payload: &mut [u8; 96],
) -> Option<usize> {
    if request.direction != MspDirection::ToFlightController || !request.payload().is_empty() {
        return None;
    }
    let len = match request.function {
        MSP_API_VERSION => {
            payload[..3].copy_from_slice(&[MSP_PROTOCOL_VERSION, MSP_API_MAJOR, MSP_API_MINOR]);
            3
        }
        MSP_FC_VARIANT => {
            payload[..4].copy_from_slice(b"FWSP");
            4
        }
        MSP_FC_VERSION => {
            payload[..3].copy_from_slice(&info.firmware_version);
            3
        }
        MSP_BOARD_INFO => write_board_info(payload, info),
        MSP_BUILD_INFO => {
            payload[..11].copy_from_slice(&info.build_date);
            payload[11..19].copy_from_slice(&info.build_time);
            payload[19..26].copy_from_slice(&info.git_revision);
            26
        }
        MSP_STATUS => {
            payload[..2].copy_from_slice(&info.cycle_time_us.to_le_bytes());
            payload[2..4].copy_from_slice(&0u16.to_le_bytes());
            payload[4..6].copy_from_slice(&info.sensors.to_le_bytes());
            payload[6..10].copy_from_slice(&u32::from(info.armed).to_le_bytes());
            payload[10] = 0;
            11
        }
        MSP_UID => {
            payload[..12].copy_from_slice(&info.uid);
            12
        }
        _ => return None,
    };
    Some(len)
}

fn write_board_info(output: &mut [u8; 96], info: &CommonInfo<'_>) -> usize {
    let mut cursor = 0;
    output[cursor..cursor + 4].copy_from_slice(&info.board_identifier);
    cursor += 4;
    output[cursor..cursor + 2].copy_from_slice(&0u16.to_le_bytes());
    cursor += 2;
    output[cursor] = 0;
    cursor += 1;
    output[cursor] = 1; // target has USB VCP
    cursor += 1;
    cursor += write_pstring(&mut output[cursor..], info.target_name);
    cursor += write_pstring(&mut output[cursor..], info.board_name);
    cursor += write_pstring(&mut output[cursor..], b"");
    output[cursor..cursor + 32].fill(0); // no writable signature
    cursor += 32;
    cursor
}

fn write_pstring(output: &mut [u8], value: &[u8]) -> usize {
    let len = value
        .len()
        .min(output.len().saturating_sub(1))
        .min(u8::MAX as usize);
    output[0] = len as u8;
    output[1..1 + len].copy_from_slice(&value[..len]);
    len + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAX_FRAME_LEN, MspParser};

    fn request(function: u16) -> MspPacket {
        let mut encoded = [0; MAX_FRAME_LEN];
        let len = encode(
            MspDirection::ToFlightController,
            0,
            function,
            &[],
            &mut encoded,
        )
        .unwrap();
        let mut parser = MspParser::new();
        encoded[..len]
            .iter()
            .find_map(|byte| parser.parse(*byte).unwrap())
            .unwrap()
    }

    fn info() -> CommonInfo<'static> {
        CommonInfo {
            firmware_version: [0, 1, 0],
            board_identifier: *b"FXR2",
            board_name: b"Foxeer F405 V2",
            target_name: b"foxeer_f405_v2",
            build_date: *b"Jul 22 2026",
            build_time: *b"00:00:00",
            git_revision: *b"unknown",
            uid: [7; 12],
            cycle_time_us: 2500,
            sensors: 1,
            armed: true,
        }
    }

    fn response_payload(function: u16) -> heapless::Vec<u8, 96> {
        let mut output = [0; MAX_FRAME_LEN];
        let len = encode_common_response(&request(function), &info(), &mut output)
            .unwrap()
            .unwrap();
        let mut parser = MspParser::new();
        let packet = output[..len]
            .iter()
            .find_map(|byte| parser.parse(*byte).unwrap())
            .unwrap();
        heapless::Vec::from_slice(packet.payload()).unwrap()
    }

    #[test]
    fn reports_ferrowasp_identity_without_claiming_betaflight() {
        assert_eq!(response_payload(MSP_API_VERSION).as_slice(), &[0, 1, 0]);
        assert_eq!(response_payload(MSP_FC_VARIANT).as_slice(), b"FWSP");
        assert_eq!(response_payload(MSP_FC_VERSION).as_slice(), &[0, 1, 0]);
    }

    #[test]
    fn status_and_uid_use_standard_minimum_layouts() {
        let status = response_payload(MSP_STATUS);
        assert_eq!(status.len(), 11);
        assert_eq!(&status[..2], &2500u16.to_le_bytes());
        assert_eq!(&status[6..10], &1u32.to_le_bytes());
        assert_eq!(response_payload(MSP_UID).as_slice(), &[7; 12]);
    }

    #[test]
    fn unknown_commands_are_not_reported_as_supported() {
        let mut output = [0; MAX_FRAME_LEN];
        assert_eq!(
            encode_common_response(&request(999), &info(), &mut output),
            Ok(None)
        );
    }
}
