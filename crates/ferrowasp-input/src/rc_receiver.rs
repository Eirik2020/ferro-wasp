//! One receiver input for either wire protocol.
//!
//! SBUS and CRSF both carry sixteen channels on the 172..=1811 scale, so the
//! RC task reads [`RcFrame`] and never needs to know which protocol the board
//! wired. The firmware chooses the protocol when it builds the receiver, to
//! match the UART configuration it selected for the receiver port.
//!
//! Requirements: INPUT-003, INPUT-004, INPUT-005, INPUT-012
//! Safety-critical.

use ferrowasp_crsf as crsf;
use sbus_rs::StreamingParser as SbusParser;

/// A raw SBUS or CRSF channel value (172..=1811) as a pulse width in
/// microseconds, the scale radios and configurators show: 172 is 988 µs,
/// 992 is 1500 µs and 1811 is 2012 µs.
pub const fn channel_us(raw: u16) -> u16 {
    // 5/8 µs per count from 880 µs, rounded to the nearest microsecond. A
    // u16 raw value stays below u16::MAX after scaling.
    ((raw as u32 * 5 + 4) / 8 + 880) as u16
}

/// One frame of receiver channels and the link flags it carried.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RcFrame {
    pub channels: [u16; 16],
    pub failsafe: bool,
    pub frame_lost: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RcParseError {
    Sbus,
    Crsf(crsf::DecodeError),
}

/// A streaming receiver decoder for the protocol the firmware selected.
#[derive(Debug)]
pub enum RcReceiver {
    Sbus(SbusParser),
    Crsf {
        parser: crsf::StreamingParser,
        /// The latest uplink link-statistics report said no packets arrive.
        uplink_lost: bool,
    },
}

impl RcReceiver {
    pub fn sbus() -> Self {
        Self::Sbus(SbusParser::new())
    }

    pub const fn crsf() -> Self {
        Self::Crsf {
            parser: crsf::StreamingParser::new(),
            uplink_lost: false,
        }
    }

    /// Drop any partial frame after a transport discontinuity.
    pub fn reset(&mut self) {
        match self {
            Self::Sbus(parser) => parser.reset(),
            Self::Crsf {
                parser,
                uplink_lost,
            } => {
                parser.reset();
                *uplink_lost = false;
            }
        }
    }

    /// Feed one received byte; returns a channel frame or a parse error when
    /// one completes. CRSF telemetry and other frame types return `None`.
    ///
    /// CRSF has no per-frame failsafe flag. A zero uplink link quality in
    /// link statistics marks following channel frames as frame-lost until a
    /// report shows the uplink back, so a receiver that keeps repeating
    /// channels through a link loss still invalidates the RC link.
    pub fn push_byte(&mut self, byte: u8) -> Option<Result<RcFrame, RcParseError>> {
        match self {
            Self::Sbus(parser) => match parser.push_byte(byte) {
                Ok(Some(packet)) => Some(Ok(RcFrame {
                    channels: packet.channels,
                    failsafe: packet.flags.failsafe,
                    frame_lost: packet.flags.frame_lost,
                })),
                Ok(None) => None,
                Err(_) => Some(Err(RcParseError::Sbus)),
            },
            Self::Crsf {
                parser,
                uplink_lost,
            } => match parser.push_byte(byte)? {
                Ok(crsf::Frame::RcChannels(channels)) => Some(Ok(RcFrame {
                    channels,
                    failsafe: false,
                    frame_lost: *uplink_lost,
                })),
                Ok(crsf::Frame::LinkStatistics(stats)) => {
                    *uplink_lost = stats.uplink_link_quality == 0;
                    None
                }
                Ok(crsf::Frame::Other { .. }) => None,
                Err(error) => Some(Err(RcParseError::Crsf(error))),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_convert_to_the_microseconds_radios_show() {
        assert_eq!(channel_us(172), 988);
        assert_eq!(channel_us(992), 1500);
        assert_eq!(channel_us(1811), 2012);
        assert_eq!(channel_us(0), 880);
    }

    fn crsf_frame(frame_type: u8, payload: &[u8]) -> heapless::Vec<u8, 64> {
        let mut bytes = heapless::Vec::new();
        bytes.push(crsf::ADDRESS_FLIGHT_CONTROLLER).unwrap();
        bytes.push((payload.len() + 2) as u8).unwrap();
        bytes.push(frame_type).unwrap();
        bytes.extend_from_slice(payload).unwrap();
        let crc = crsf::crc8_dvb_s2(&bytes[2..]);
        bytes.push(crc).unwrap();
        bytes
    }

    fn feed(receiver: &mut RcReceiver, bytes: &[u8]) -> Option<Result<RcFrame, RcParseError>> {
        let mut last = None;
        for byte in bytes {
            if let Some(result) = receiver.push_byte(*byte) {
                last = Some(result);
            }
        }
        last
    }

    #[test]
    fn sbus_and_crsf_deliver_the_same_channels() {
        let mut channels = [992_u16; 16];
        channels[2] = 172;
        channels[4] = 1811;

        let mut sbus_frame = [0_u8; 25];
        sbus_frame[0] = 0x0F;
        sbus_rs::pack_channels(&mut sbus_frame, &channels);
        let mut sbus = RcReceiver::sbus();
        let from_sbus = feed(&mut sbus, &sbus_frame).unwrap().unwrap();

        let mut crsf = RcReceiver::crsf();
        let from_crsf = feed(
            &mut crsf,
            &crsf_frame(
                crsf::FRAME_TYPE_RC_CHANNELS_PACKED,
                &crsf::pack_channels(&channels),
            ),
        )
        .unwrap()
        .unwrap();

        assert_eq!(from_sbus.channels, channels);
        assert_eq!(from_sbus, from_crsf);
    }

    // Contract tests for the external SBUS parser (sbus-rs), at the interface
    // where its output enters FerroWasp: requirement INPUT-004.

    const SBUS_FRAME_LOST: u8 = 1 << 2;
    const SBUS_FAILSAFE: u8 = 1 << 3;

    fn sbus_frame(channels: &[u16; 16], flags: u8) -> [u8; 25] {
        let mut frame = [0_u8; 25];
        frame[0] = 0x0F;
        sbus_rs::pack_channels(&mut frame, channels);
        frame[23] = flags;
        frame
    }

    fn sbus_frames(
        receiver: &mut RcReceiver,
        bytes: &[u8],
    ) -> heapless::Vec<Result<RcFrame, RcParseError>, 8> {
        let mut results = heapless::Vec::new();
        for byte in bytes {
            if let Some(result) = receiver.push_byte(*byte) {
                results.push(result).unwrap();
            }
        }
        results
    }

    #[test]
    fn sbus_failsafe_and_frame_lost_flags_reach_the_frame() {
        let channels = [992_u16; 16];
        for (flags, failsafe, frame_lost) in [
            (0, false, false),
            (SBUS_FRAME_LOST, false, true),
            (SBUS_FAILSAFE, true, false),
            (SBUS_FAILSAFE | SBUS_FRAME_LOST, true, true),
        ] {
            let mut sbus = RcReceiver::sbus();
            let frame = feed(&mut sbus, &sbus_frame(&channels, flags))
                .unwrap()
                .unwrap();
            assert_eq!(frame.failsafe, failsafe, "flags {flags:#04x}");
            assert_eq!(frame.frame_lost, frame_lost, "flags {flags:#04x}");
            assert_eq!(frame.channels, channels);
        }
    }

    #[test]
    fn sbus_out_of_range_channels_pass_through_unclamped() {
        // Bounding is the stick mapping's job (INPUT-009); the parser must
        // deliver what the receiver sent, not a silently corrected value.
        let mut channels = [992_u16; 16];
        channels[0] = 0;
        channels[1] = 2047;
        let mut sbus = RcReceiver::sbus();
        let frame = feed(&mut sbus, &sbus_frame(&channels, 0)).unwrap().unwrap();
        assert_eq!(frame.channels, channels);
    }

    #[test]
    fn sbus_bad_footer_is_an_error_and_no_wrong_frame_follows() {
        let mut channels = [992_u16; 16];
        channels[3] = 1500;
        let good = sbus_frame(&channels, 0);
        let mut bad = good;
        bad[24] = 0x55;

        let mut stream: heapless::Vec<u8, 100> = heapless::Vec::new();
        stream.extend_from_slice(&bad).unwrap();
        stream.extend_from_slice(&good).unwrap();
        stream.extend_from_slice(&good).unwrap();

        let mut sbus = RcReceiver::sbus();
        let results = sbus_frames(&mut sbus, &stream);
        assert_eq!(results[0], Err(RcParseError::Sbus));
        let delivered: heapless::Vec<RcFrame, 8> = results.iter().filter_map(|r| r.ok()).collect();
        assert!(!delivered.is_empty(), "the stream resynchronises");
        assert!(
            delivered
                .iter()
                .all(|frame| frame.channels == channels && !frame.failsafe && !frame.frame_lost)
        );
    }

    #[test]
    fn sbus_partial_frame_is_dropped_on_reset() {
        let channels = [992_u16; 16];
        let frame = sbus_frame(&channels, 0);
        let mut sbus = RcReceiver::sbus();
        assert!(feed(&mut sbus, &frame[..12]).is_none());
        sbus.reset();
        assert!(
            sbus_frames(&mut sbus, &frame[12..])
                .iter()
                .all(|r| r.is_err()),
            "no frame spans the gap"
        );
        sbus.reset();
        assert_eq!(feed(&mut sbus, &frame).unwrap().unwrap().channels, channels);
    }

    #[test]
    fn sbus_skips_leading_noise_before_a_header() {
        let channels = [992_u16; 16];
        let mut stream: heapless::Vec<u8, 40> = heapless::Vec::new();
        stream.extend_from_slice(&[0x00, 0xFF, 0x55, 0xAA]).unwrap();
        stream.extend_from_slice(&sbus_frame(&channels, 0)).unwrap();
        let mut sbus = RcReceiver::sbus();
        let results = sbus_frames(&mut sbus, &stream);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().unwrap().channels, channels);
    }

    #[test]
    fn crsf_zero_uplink_quality_marks_frames_lost_until_it_recovers() {
        let channels = crsf::pack_channels(&[992; 16]);
        let rc = crsf_frame(crsf::FRAME_TYPE_RC_CHANNELS_PACKED, &channels);
        let lost = crsf_frame(
            crsf::FRAME_TYPE_LINK_STATISTICS,
            &[90, 90, 0, 0, 0, 4, 0, 90, 100, 5],
        );
        let back = crsf_frame(
            crsf::FRAME_TYPE_LINK_STATISTICS,
            &[60, 60, 100, 9, 0, 4, 0, 60, 100, 9],
        );
        let mut receiver = RcReceiver::crsf();

        assert!(!feed(&mut receiver, &rc).unwrap().unwrap().frame_lost);
        assert_eq!(feed(&mut receiver, &lost), None);
        assert!(feed(&mut receiver, &rc).unwrap().unwrap().frame_lost);
        assert_eq!(feed(&mut receiver, &back), None);
        assert!(!feed(&mut receiver, &rc).unwrap().unwrap().frame_lost);
    }

    #[test]
    fn crsf_crc_error_is_a_parse_error() {
        let mut bytes = crsf_frame(
            crsf::FRAME_TYPE_RC_CHANNELS_PACKED,
            &crsf::pack_channels(&[992; 16]),
        );
        bytes[4] ^= 0x80;
        let mut receiver = RcReceiver::crsf();
        assert_eq!(
            feed(&mut receiver, &bytes),
            Some(Err(RcParseError::Crsf(crsf::DecodeError::Crc)))
        );
    }
}
