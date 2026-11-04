//! The byte-at-a-time CRSF frame decoder.
//!
//! The parser takes one byte at a time, so it serves an interrupt or DMA
//! handler that has a chunk of bytes as well as the embedded-io reader. Its
//! state is one 64-byte frame buffer and a position, and each byte costs a
//! fixed amount of work: a frame's CRC is computed once, over at most 61
//! bytes, when its last byte arrives.
//!
//! Requirements: CRSF-001, CRSF-002, CRSF-003, CRSF-004, CRSF-005
//! Safety-critical.

use crate::channels::{CHANNEL_COUNT, RC_CHANNELS_PAYLOAD_LEN, unpack_channels};
use crate::frame::{
    FRAME_TYPE_LINK_STATISTICS, FRAME_TYPE_RC_CHANNELS_PACKED, MAX_FRAME_LEN, MAX_LENGTH_FIELD,
    MIN_LENGTH_FIELD, crc8_dvb_s2, is_frame_address,
};

/// Payload length of a link statistics frame.
pub const LINK_STATISTICS_PAYLOAD_LEN: usize = 10;

/// What a receiver reports about its radio link.
///
/// CRSF has no failsafe flag in the channel frame. An ExpressLRS receiver
/// stops sending channels when its link drops, so the consumer's frame
/// timeout is the primary loss detector; an uplink link quality of zero is an
/// explicit loss signal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LinkStatistics {
    /// Uplink RSSI of the first antenna, in negative dBm.
    pub uplink_rssi_ant1: u8,
    /// Uplink RSSI of the second antenna, in negative dBm.
    pub uplink_rssi_ant2: u8,
    /// Uplink packet success rate, 0..=100 percent.
    pub uplink_link_quality: u8,
    /// Uplink signal-to-noise ratio, in dB.
    pub uplink_snr: i8,
    /// The antenna the receiver is using.
    pub active_antenna: u8,
    /// The receiver's RF mode, as its firmware numbers them.
    pub rf_mode: u8,
    /// Uplink transmit power, as the protocol's power index.
    pub uplink_tx_power: u8,
    /// Downlink RSSI, in negative dBm.
    pub downlink_rssi: u8,
    /// Downlink packet success rate, 0..=100 percent.
    pub downlink_link_quality: u8,
    /// Downlink signal-to-noise ratio, in dB.
    pub downlink_snr: i8,
}

impl LinkStatistics {
    fn decode(payload: &[u8; LINK_STATISTICS_PAYLOAD_LEN]) -> Self {
        Self {
            uplink_rssi_ant1: payload[0],
            uplink_rssi_ant2: payload[1],
            uplink_link_quality: payload[2],
            uplink_snr: payload[3] as i8,
            active_antenna: payload[4],
            rf_mode: payload[5],
            uplink_tx_power: payload[6],
            downlink_rssi: payload[7],
            downlink_link_quality: payload[8],
            downlink_snr: payload[9] as i8,
        }
    }
}

/// A frame that passed its length and CRC checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Frame {
    /// Sixteen channels on the 172..=1811 scale.
    RcChannels([u16; CHANNEL_COUNT]),
    LinkStatistics(LinkStatistics),
    /// A well-formed frame of a type this decoder does not interpret.
    Other {
        frame_type: u8,
    },
}

/// Why a frame was dropped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    /// The length byte is outside what a CRSF frame can carry.
    Length(u8),
    /// The CRC over type and payload does not match.
    Crc,
    /// A known frame type with the wrong payload length.
    PayloadLength { frame_type: u8, len: u8 },
}

/// What one byte produced, if it completed or rejected a frame.
pub type ParseResult = Result<Frame, DecodeError>;

/// Decode the type and payload of a frame whose CRC already passed.
fn decode_body(frame_type: u8, payload: &[u8]) -> ParseResult {
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
            Ok(Frame::LinkStatistics(LinkStatistics::decode(payload)))
        }
        frame_type => Ok(Frame::Other { frame_type }),
    }
}

/// Byte-at-a-time CRSF decoder with a fixed buffer and bounded work per byte.
///
/// Only the flight controller, receiver and transmitter addresses start a
/// frame; every other byte before one is discarded. A bad length or CRC
/// drops the frame and returns to address search, so the next frame
/// resynchronizes. The byte that broke a frame is not rescanned as a
/// possible start.
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

    /// Drop any partial frame and return to address search, as after a gap
    /// or fault in the byte stream.
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Whether the parser is part-way through a frame.
    pub const fn in_frame(&self) -> bool {
        self.pos != 0
    }

    /// Feed one byte. Returns a result when the byte completes or rejects a
    /// frame, and `None` otherwise.
    pub fn push_byte(&mut self, byte: u8) -> Option<ParseResult> {
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
                // The length check above bounds a frame to the buffer, so
                // `pos` never reaches its end; a defensive reset keeps that
                // true for any state.
                let Some(slot) = self.buffer.get_mut(pos) else {
                    self.pos = 0;
                    return Some(Err(DecodeError::Length(self.buffer[1])));
                };
                *slot = byte;
                self.pos += 1;
                let frame_len = usize::from(self.buffer[1]) + 2;
                if self.pos < frame_len {
                    return None;
                }
                self.pos = 0;
                let (body, crc) = self.buffer[2..frame_len].split_at(frame_len - 3);
                if crc8_dvb_s2(body) != crc[0] {
                    return Some(Err(DecodeError::Crc));
                }
                Some(decode_body(body[0], &body[1..]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::pack_channels;
    use crate::frame::{ADDRESS_FLIGHT_CONTROLLER, ADDRESS_RECEIVER, ADDRESS_TRANSMITTER};

    fn frame(address: u8, frame_type: u8, payload: &[u8]) -> ([u8; MAX_FRAME_LEN], usize) {
        let mut bytes = [0_u8; MAX_FRAME_LEN];
        bytes[0] = address;
        bytes[1] = (payload.len() + 2) as u8;
        bytes[2] = frame_type;
        bytes[3..3 + payload.len()].copy_from_slice(payload);
        let crc_index = 3 + payload.len();
        bytes[crc_index] = crc8_dvb_s2(&bytes[2..crc_index]);
        (bytes, crc_index + 1)
    }

    fn feed(parser: &mut StreamingParser, bytes: &[u8]) -> Option<ParseResult> {
        let mut last = None;
        for byte in bytes {
            if let Some(result) = parser.push_byte(*byte) {
                last = Some(result);
            }
        }
        last
    }

    /// A deterministic xorshift generator, so a failure reproduces.
    struct Noise(u64);

    impl Noise {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn byte(&mut self) -> u8 {
            (self.next() >> 24) as u8
        }
    }

    #[test]
    fn rc_frame_decodes_after_leading_noise() {
        let mut channels = [992_u16; CHANNEL_COUNT];
        channels[2] = 172;
        channels[4] = 1811;
        let (bytes, len) = frame(
            ADDRESS_FLIGHT_CONTROLLER,
            FRAME_TYPE_RC_CHANNELS_PACKED,
            &pack_channels(&channels),
        );
        let mut parser = StreamingParser::new();
        assert_eq!(feed(&mut parser, &[0x55, 0x12, 0x00]), None);
        assert!(!parser.in_frame());
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Ok(Frame::RcChannels(channels)))
        );
    }

    #[test]
    fn only_the_three_link_addresses_start_a_frame() {
        let channels = [992_u16; CHANNEL_COUNT];
        let payload = pack_channels(&channels);
        for address in [
            ADDRESS_FLIGHT_CONTROLLER,
            ADDRESS_RECEIVER,
            ADDRESS_TRANSMITTER,
        ] {
            let (bytes, len) = frame(address, FRAME_TYPE_RC_CHANNELS_PACKED, &payload);
            let mut parser = StreamingParser::new();
            assert_eq!(
                feed(&mut parser, &bytes[..len]),
                Some(Ok(Frame::RcChannels(channels)))
            );
        }
        for address in (0..=u8::MAX).filter(|byte| !is_frame_address(*byte)) {
            let (bytes, len) = frame(address, FRAME_TYPE_RC_CHANNELS_PACKED, &payload);
            let mut parser = StreamingParser::new();
            assert_eq!(parser.push_byte(address), None);
            assert!(!parser.in_frame(), "address {address:#04x} started a frame");
            // The rest of the frame holds no address byte for these
            // channels, so nothing at all is delivered.
            assert_eq!(feed(&mut parser, &bytes[1..len]), None, "{address:#04x}");
        }
    }

    #[test]
    fn link_statistics_expose_every_field() {
        let payload = [60, 70, 0, (-5_i8) as u8, 1, 4, 2, 50, 100, (-8_i8) as u8];
        let (bytes, len) = frame(
            ADDRESS_FLIGHT_CONTROLLER,
            FRAME_TYPE_LINK_STATISTICS,
            &payload,
        );
        let mut parser = StreamingParser::new();
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Ok(Frame::LinkStatistics(LinkStatistics {
                uplink_rssi_ant1: 60,
                uplink_rssi_ant2: 70,
                uplink_link_quality: 0,
                uplink_snr: -5,
                active_antenna: 1,
                rf_mode: 4,
                uplink_tx_power: 2,
                downlink_rssi: 50,
                downlink_link_quality: 100,
                downlink_snr: -8,
            })))
        );
    }

    #[test]
    fn corrupt_crc_is_reported_and_the_next_frame_still_decodes() {
        let channels = [992_u16; CHANNEL_COUNT];
        let (good, len) = frame(
            ADDRESS_FLIGHT_CONTROLLER,
            FRAME_TYPE_RC_CHANNELS_PACKED,
            &pack_channels(&channels),
        );
        let mut bad = good;
        bad[5] ^= 0x01;
        let mut parser = StreamingParser::new();
        assert_eq!(feed(&mut parser, &bad[..len]), Some(Err(DecodeError::Crc)));
        assert!(!parser.in_frame());
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
        assert!(!parser.in_frame());
        assert_eq!(parser.push_byte(ADDRESS_FLIGHT_CONTROLLER), None);
        assert_eq!(parser.push_byte(1), Some(Err(DecodeError::Length(1))));
        assert_eq!(parser.push_byte(ADDRESS_FLIGHT_CONTROLLER), None);
        assert_eq!(parser.push_byte(0), Some(Err(DecodeError::Length(0))));
        assert_eq!(parser.push_byte(ADDRESS_FLIGHT_CONTROLLER), None);
        assert_eq!(parser.push_byte(0xFF), Some(Err(DecodeError::Length(0xFF))));
    }

    #[test]
    fn the_longest_and_shortest_frames_decode() {
        let (longest, len) = frame(ADDRESS_FLIGHT_CONTROLLER, 0x7B, &[0xA5; 60]);
        assert_eq!(len, MAX_FRAME_LEN);
        let (shortest, short_len) = frame(ADDRESS_FLIGHT_CONTROLLER, 0x7B, &[]);
        assert_eq!(short_len, 4);
        let mut parser = StreamingParser::new();
        assert_eq!(
            feed(&mut parser, &longest[..len]),
            Some(Ok(Frame::Other { frame_type: 0x7B }))
        );
        assert_eq!(
            feed(&mut parser, &shortest[..short_len]),
            Some(Ok(Frame::Other { frame_type: 0x7B }))
        );
    }

    #[test]
    fn short_rc_payload_is_an_error_not_a_frame() {
        let (bytes, len) = frame(
            ADDRESS_FLIGHT_CONTROLLER,
            FRAME_TYPE_RC_CHANNELS_PACKED,
            &[0; 10],
        );
        let mut parser = StreamingParser::new();
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Err(DecodeError::PayloadLength {
                frame_type: FRAME_TYPE_RC_CHANNELS_PACKED,
                len: 10,
            }))
        );
        let (bytes, len) = frame(
            ADDRESS_FLIGHT_CONTROLLER,
            FRAME_TYPE_LINK_STATISTICS,
            &[0; 11],
        );
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Err(DecodeError::PayloadLength {
                frame_type: FRAME_TYPE_LINK_STATISTICS,
                len: 11,
            }))
        );
    }

    #[test]
    fn unknown_types_pass_through_as_other() {
        let (bytes, len) = frame(ADDRESS_FLIGHT_CONTROLLER, 0x28, &[0xEA, 0xC8]);
        let mut parser = StreamingParser::new();
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Ok(Frame::Other { frame_type: 0x28 }))
        );
    }

    #[test]
    fn a_reset_drops_the_partial_frame() {
        let channels = [992_u16; CHANNEL_COUNT];
        let (bytes, len) = frame(
            ADDRESS_FLIGHT_CONTROLLER,
            FRAME_TYPE_RC_CHANNELS_PACKED,
            &pack_channels(&channels),
        );
        let mut parser = StreamingParser::new();
        assert_eq!(feed(&mut parser, &bytes[..10]), None);
        assert!(parser.in_frame());
        parser.reset();
        assert!(!parser.in_frame());
        assert_eq!(
            feed(&mut parser, &bytes[..len]),
            Some(Ok(Frame::RcChannels(channels)))
        );
    }

    #[test]
    fn random_bytes_never_panic_and_the_state_stays_bounded() {
        let mut noise = Noise(0x9E37_79B9_7F4A_7C15);
        let mut parser = StreamingParser::new();
        let mut results = 0_usize;
        let mut since_result = 0_usize;
        for _ in 0..2_000_000 {
            let byte = noise.byte();
            let result = parser.push_byte(byte);
            assert!(parser.pos < MAX_FRAME_LEN);
            if result.is_some() {
                results += 1;
                since_result = 0;
                assert!(!parser.in_frame());
            } else if parser.in_frame() {
                since_result += 1;
                // A frame in progress ends, one way or another, within the
                // longest frame.
                assert!(since_result < MAX_FRAME_LEN);
            } else {
                since_result = 0;
            }
        }
        // Noise holds an address byte about every 85 bytes, so results arrive.
        assert!(results > 1_000);
    }

    #[test]
    fn noise_dense_with_address_bytes_never_panics() {
        // Bias the generator towards address bytes and in-range lengths, the
        // inputs that drive the deepest paths through the parser.
        let mut noise = Noise(0x0123_4567_89AB_CDEF);
        let mut parser = StreamingParser::new();
        for _ in 0..2_000_000 {
            let raw = noise.next();
            let byte = match raw % 4 {
                0 => ADDRESS_FLIGHT_CONTROLLER,
                1 => (raw >> 8) as u8 % 64,
                _ => (raw >> 16) as u8,
            };
            let _ = parser.push_byte(byte);
            assert!(parser.pos < MAX_FRAME_LEN);
        }
    }

    #[test]
    fn valid_frames_after_noise_and_a_reset_always_decode() {
        let mut noise = Noise(0xDEAD_BEEF_CAFE_F00D);
        let mut parser = StreamingParser::new();
        for round in 0..2_000 {
            for _ in 0..(noise.next() % 200) {
                let _ = parser.push_byte(noise.byte());
            }
            // A gap in the stream is where a caller resets.
            parser.reset();
            let mut channels = [0_u16; CHANNEL_COUNT];
            for channel in &mut channels {
                *channel = 172 + (noise.next() % 1640) as u16;
            }
            let (bytes, len) = frame(
                ADDRESS_RECEIVER,
                FRAME_TYPE_RC_CHANNELS_PACKED,
                &pack_channels(&channels),
            );
            assert_eq!(
                feed(&mut parser, &bytes[..len]),
                Some(Ok(Frame::RcChannels(channels))),
                "round {round}"
            );
        }
    }
}
