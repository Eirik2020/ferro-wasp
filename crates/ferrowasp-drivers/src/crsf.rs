//! CRSF receiver-to-flight-controller frame decoding.
//!
//! CRSF is the serial protocol ExpressLRS and TBS Crossfire receivers both
//! speak to a flight controller: 420 000 baud, 8N1, not inverted. A frame is
//! an address byte, a length byte counting the type, payload and CRC, a type
//! byte, the payload, and a CRC-8/DVB-S2 over the type and payload.
//!
//! RC channels arrive as sixteen 11-bit values packed least-significant bit
//! first, on the same 172..=1811 scale SBUS uses, so channel consumers do not
//! need to know which protocol delivered them.
//!
//! CRSF has no failsafe flag in the channel frame. An ExpressLRS receiver stops
//! sending channels when its link drops, so the consumer's frame timeout is the
//! primary loss detector; link statistics add an explicit zero-quality signal.

/// Longest frame on the wire: address, length, and up to 62 counted bytes.
pub const MAX_FRAME_LEN: usize = 64;
/// Channels carried by one RC channels frame.
pub const CHANNEL_COUNT: usize = 16;

/// Address a receiver uses for frames sent to the flight controller.
pub const ADDRESS_FLIGHT_CONTROLLER: u8 = 0xC8;
/// Receiver and transmitter-module addresses seen on the FC link.
pub const ADDRESS_RECEIVER: u8 = 0xEC;
pub const ADDRESS_TRANSMITTER: u8 = 0xEE;

pub const FRAME_TYPE_BATTERY_SENSOR: u8 = 0x08;
pub const FRAME_TYPE_LINK_STATISTICS: u8 = 0x14;
pub const FRAME_TYPE_RC_CHANNELS_PACKED: u8 = 0x16;

const RC_CHANNELS_PAYLOAD_LEN: usize = 22;
const LINK_STATISTICS_PAYLOAD_LEN: usize = 10;
/// The length byte counts the type and CRC, so two is the empty frame.
const MIN_LENGTH_FIELD: u8 = 2;
const MAX_LENGTH_FIELD: u8 = (MAX_FRAME_LEN - 2) as u8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkStatistics {
    /// Uplink RSSI of the first antenna, in negative dBm.
    pub uplink_rssi_ant1: u8,
    /// Uplink packet success rate, 0..=100 percent.
    pub uplink_link_quality: u8,
    pub uplink_snr: i8,
    pub rf_mode: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Frame {
    RcChannels([u16; CHANNEL_COUNT]),
    LinkStatistics(LinkStatistics),
    /// A well-formed frame of a type this decoder does not interpret.
    Other {
        frame_type: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    /// The length byte is outside what a CRSF frame can carry.
    Length(u8),
    Crc,
    /// A known frame type with the wrong payload length.
    PayloadLength {
        frame_type: u8,
        len: u8,
    },
}

/// CRC-8/DVB-S2 (polynomial 0xD5), as CRSF uses over type and payload.
pub fn crc8_dvb_s2(bytes: &[u8]) -> u8 {
    let mut crc = 0_u8;
    for byte in bytes {
        crc ^= *byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0xD5
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Unpack sixteen 11-bit channels, least-significant bit first.
pub fn unpack_channels(payload: &[u8; RC_CHANNELS_PAYLOAD_LEN]) -> [u16; CHANNEL_COUNT] {
    let mut channels = [0_u16; CHANNEL_COUNT];
    let mut accumulator = 0_u32;
    let mut bits = 0_u32;
    let mut index = 0;
    for byte in payload {
        accumulator |= u32::from(*byte) << bits;
        bits += 8;
        if bits >= 11 {
            channels[index] = (accumulator & 0x07FF) as u16;
            accumulator >>= 11;
            bits -= 11;
            index += 1;
        }
    }
    channels
}

/// Pack sixteen channels into a payload; values are truncated to 11 bits.
pub fn pack_channels(channels: &[u16; CHANNEL_COUNT]) -> [u8; RC_CHANNELS_PAYLOAD_LEN] {
    let mut payload = [0_u8; RC_CHANNELS_PAYLOAD_LEN];
    let mut accumulator = 0_u32;
    let mut bits = 0_u32;
    let mut index = 0;
    for channel in channels {
        accumulator |= u32::from(*channel & 0x07FF) << bits;
        bits += 11;
        while bits >= 8 {
            payload[index] = accumulator as u8;
            accumulator >>= 8;
            bits -= 8;
            index += 1;
        }
    }
    payload
}

/// Decode the type and payload of a frame whose CRC already passed.
fn decode_body(frame_type: u8, payload: &[u8]) -> Result<Frame, DecodeError> {
    let wrong_len = DecodeError::PayloadLength {
        frame_type,
        len: payload.len() as u8,
    };
    match frame_type {
        FRAME_TYPE_RC_CHANNELS_PACKED => {
            let payload: &[u8; RC_CHANNELS_PAYLOAD_LEN] =
                payload.try_into().map_err(|_| wrong_len)?;
            Ok(Frame::RcChannels(unpack_channels(payload)))
        }
        FRAME_TYPE_LINK_STATISTICS => {
            let payload: &[u8; LINK_STATISTICS_PAYLOAD_LEN] =
                payload.try_into().map_err(|_| wrong_len)?;
            Ok(Frame::LinkStatistics(LinkStatistics {
                uplink_rssi_ant1: payload[0],
                uplink_link_quality: payload[2],
                uplink_snr: payload[3] as i8,
                rf_mode: payload[5],
            }))
        }
        frame_type => Ok(Frame::Other { frame_type }),
    }
}

const fn is_frame_address(byte: u8) -> bool {
    matches!(
        byte,
        ADDRESS_FLIGHT_CONTROLLER | ADDRESS_RECEIVER | ADDRESS_TRANSMITTER
    )
}

/// Byte-at-a-time CRSF decoder with a fixed buffer and bounded work per byte.
///
/// Bytes before an address byte are discarded. A bad length or CRC drops the
/// frame and returns to address search, so the next frame resynchronizes.
#[derive(Clone, Debug)]
pub struct StreamingParser {
    buffer: [u8; MAX_FRAME_LEN],
    pos: usize,
}

impl Default for StreamingParser {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingParser {
    pub const fn new() -> Self {
        Self {
            buffer: [0; MAX_FRAME_LEN],
            pos: 0,
        }
    }

    pub fn reset(&mut self) {
        self.pos = 0;
    }

    pub fn push_byte(&mut self, byte: u8) -> Option<Result<Frame, DecodeError>> {
        match self.pos {
            0 => {
                if is_frame_address(byte) {
                    self.buffer[0] = byte;
                    self.pos = 1;
                }
                None
            }
            1 => {
                if !(MIN_LENGTH_FIELD..=MAX_LENGTH_FIELD).contains(&byte) {
                    self.pos = 0;
                    return Some(Err(DecodeError::Length(byte)));
                }
                self.buffer[1] = byte;
                self.pos = 2;
                None
            }
            pos => {
                self.buffer[pos] = byte;
                self.pos += 1;
                let frame_len = usize::from(self.buffer[1]) + 2;
                if self.pos < frame_len {
                    return None;
                }
                self.pos = 0;
                let body = &self.buffer[2..frame_len - 1];
                if crc8_dvb_s2(body) != self.buffer[frame_len - 1] {
                    return Some(Err(DecodeError::Crc));
                }
                Some(decode_body(body[0], &body[1..]))
            }
        }
    }
}

const BATTERY_PAYLOAD_LEN: usize = 8;
/// Address, length, type, payload and CRC.
pub const BATTERY_FRAME_LEN: usize = BATTERY_PAYLOAD_LEN + 4;

/// A battery sensor report for the receiver to relay to the radio.
///
/// Every field is in CRSF's own units. A value the flight controller does not
/// measure is sent as zero, which radios show as zero rather than as missing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatteryTelemetry {
    /// Pack voltage in tenths of a volt.
    pub voltage_dv: u16,
    /// Pack current in tenths of an amp.
    pub current_da: u16,
    /// Charge drawn so far in mAh; CRSF carries 24 bits and larger saturates.
    pub used_mah: u32,
    /// Estimated remaining charge in percent, 0-100.
    pub remaining_percent: u8,
}

/// Encode one battery sensor frame addressed to the receiver.
pub fn encode_battery(telemetry: BatteryTelemetry) -> [u8; BATTERY_FRAME_LEN] {
    let used = telemetry.used_mah.min(0x00FF_FFFF).to_be_bytes();
    let voltage = telemetry.voltage_dv.to_be_bytes();
    let current = telemetry.current_da.to_be_bytes();
    let mut frame = [
        ADDRESS_FLIGHT_CONTROLLER,
        (BATTERY_PAYLOAD_LEN + 2) as u8,
        FRAME_TYPE_BATTERY_SENSOR,
        voltage[0],
        voltage[1],
        current[0],
        current[1],
        used[1],
        used[2],
        used[3],
        telemetry.remaining_percent.min(100),
        0,
    ];
    frame[BATTERY_FRAME_LEN - 1] = crc8_dvb_s2(&frame[2..BATTERY_FRAME_LEN - 1]);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(frame_type: u8, payload: &[u8]) -> ([u8; MAX_FRAME_LEN], usize) {
        let mut bytes = [0_u8; MAX_FRAME_LEN];
        bytes[0] = ADDRESS_FLIGHT_CONTROLLER;
        bytes[1] = (payload.len() + 2) as u8;
        bytes[2] = frame_type;
        bytes[3..3 + payload.len()].copy_from_slice(payload);
        let crc_index = 3 + payload.len();
        bytes[crc_index] = crc8_dvb_s2(&bytes[2..crc_index]);
        (bytes, crc_index + 1)
    }

    fn feed(parser: &mut StreamingParser, bytes: &[u8]) -> Option<Result<Frame, DecodeError>> {
        let mut last = None;
        for byte in bytes {
            if let Some(result) = parser.push_byte(*byte) {
                last = Some(result);
            }
        }
        last
    }

    #[test]
    fn crc_matches_the_dvb_s2_check_value() {
        assert_eq!(crc8_dvb_s2(b"123456789"), 0xBC);
    }

    #[test]
    fn channels_round_trip_through_the_packed_payload() {
        let mut channels = [0_u16; CHANNEL_COUNT];
        for (index, channel) in channels.iter_mut().enumerate() {
            *channel = 172 + (index as u16) * 109;
        }
        assert_eq!(unpack_channels(&pack_channels(&channels)), channels);
    }

    #[test]
    fn first_channel_occupies_the_low_bits_of_the_first_byte() {
        let mut channels = [0_u16; CHANNEL_COUNT];
        channels[0] = 0x07FF;
        let payload = pack_channels(&channels);
        assert_eq!(payload[0], 0xFF);
        assert_eq!(payload[1], 0x07);
        assert!(payload[2..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn rc_frame_decodes_after_leading_noise() {
        let mut channels = [992_u16; CHANNEL_COUNT];
        channels[2] = 172;
        channels[4] = 1811;
        let (bytes, len) = frame(FRAME_TYPE_RC_CHANNELS_PACKED, &pack_channels(&channels));
        let mut parser = StreamingParser::new();
        assert_eq!(feed(&mut parser, &[0x55, 0x12]), None);
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Ok(Frame::RcChannels(channels)))
        );
    }

    #[test]
    fn link_statistics_expose_uplink_quality() {
        let payload = [60, 70, 0, (-5_i8) as u8, 0, 4, 2, 50, 100, 8];
        let (bytes, len) = frame(FRAME_TYPE_LINK_STATISTICS, &payload);
        let mut parser = StreamingParser::new();
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Ok(Frame::LinkStatistics(LinkStatistics {
                uplink_rssi_ant1: 60,
                uplink_link_quality: 0,
                uplink_snr: -5,
                rf_mode: 4,
            })))
        );
    }

    #[test]
    fn corrupt_crc_is_reported_and_the_next_frame_still_decodes() {
        let channels = [992_u16; CHANNEL_COUNT];
        let (good, len) = frame(FRAME_TYPE_RC_CHANNELS_PACKED, &pack_channels(&channels));
        let mut bad = good;
        bad[5] ^= 0x01;
        let mut parser = StreamingParser::new();
        assert_eq!(feed(&mut parser, &bad[..len]), Some(Err(DecodeError::Crc)));
        assert_eq!(
            feed(&mut parser, &good[..len]),
            Some(Ok(Frame::RcChannels(channels)))
        );
    }

    #[test]
    fn impossible_length_is_rejected_without_buffering() {
        let mut parser = StreamingParser::new();
        assert_eq!(parser.push_byte(ADDRESS_FLIGHT_CONTROLLER), None);
        assert_eq!(parser.push_byte(63), Some(Err(DecodeError::Length(63))));
        assert_eq!(parser.push_byte(ADDRESS_FLIGHT_CONTROLLER), None);
        assert_eq!(parser.push_byte(1), Some(Err(DecodeError::Length(1))));
    }

    #[test]
    fn short_rc_payload_is_an_error_not_a_frame() {
        let (bytes, len) = frame(FRAME_TYPE_RC_CHANNELS_PACKED, &[0; 10]);
        let mut parser = StreamingParser::new();
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Err(DecodeError::PayloadLength {
                frame_type: FRAME_TYPE_RC_CHANNELS_PACKED,
                len: 10,
            }))
        );
    }

    #[test]
    fn unknown_types_pass_through_as_other() {
        let (bytes, len) = frame(0x28, &[0xEA, 0xC8]);
        let mut parser = StreamingParser::new();
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Ok(Frame::Other { frame_type: 0x28 }))
        );
    }

    #[test]
    fn a_battery_frame_is_big_endian_and_round_trips_through_the_parser() {
        let frame = encode_battery(BatteryTelemetry {
            voltage_dv: 168,
            current_da: 0x0102,
            used_mah: 0x0003_0405,
            remaining_percent: 87,
        });
        assert_eq!(
            frame[..BATTERY_FRAME_LEN - 1],
            [0xC8, 10, 0x08, 0, 168, 0x01, 0x02, 0x03, 0x04, 0x05, 87]
        );

        let mut parser = StreamingParser::new();
        let mut decoded = None;
        for byte in frame {
            if let Some(result) = parser.push_byte(byte) {
                decoded = Some(result);
            }
        }
        assert_eq!(
            decoded,
            Some(Ok(Frame::Other {
                frame_type: FRAME_TYPE_BATTERY_SENSOR
            }))
        );
    }

    #[test]
    fn battery_fields_saturate_at_what_crsf_carries() {
        let frame = encode_battery(BatteryTelemetry {
            used_mah: u32::MAX,
            remaining_percent: 250,
            ..BatteryTelemetry::default()
        });
        assert_eq!(frame[7..11], [0xFF, 0xFF, 0xFF, 100]);
    }
}
