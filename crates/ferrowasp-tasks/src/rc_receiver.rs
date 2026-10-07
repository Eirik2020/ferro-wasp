//! One receiver input for either wire protocol.
//!
//! SBUS and CRSF both carry sixteen channels on the 172..=1811 scale, so the
//! RC task reads [`RcFrame`] and never needs to know which protocol the board
//! wired. The firmware chooses the protocol when it builds the receiver, to
//! match the UART configuration it selected for the receiver port.

use ferrowasp_drivers::crsf;
use sbus_rs::StreamingParser as SbusParser;

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
