//! BLHeli/KISS-style legacy ESC telemetry decoding.
//!
//! The UART frame is ten bytes at 115_200 baud. It has no sync byte or motor
//! identifier, so the caller must associate a validated frame with the DShot
//! lane whose telemetry bit was most recently requested.

pub const FRAME_LEN: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EscTelemetry {
    pub temperature_c: u8,
    pub voltage_cv: u16,
    pub current_ca: u16,
    pub consumption_mah: u16,
    /// Electrical RPM divided by 100, as carried on the wire.
    pub erpm_div100: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    Crc,
}

pub fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0_u8;
    for byte in bytes {
        crc ^= *byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

pub fn decode(frame: &[u8; FRAME_LEN]) -> Result<EscTelemetry, DecodeError> {
    if crc8(&frame[..FRAME_LEN - 1]) != frame[FRAME_LEN - 1] {
        return Err(DecodeError::Crc);
    }

    Ok(EscTelemetry {
        temperature_c: frame[0],
        voltage_cv: u16::from_be_bytes([frame[1], frame[2]]),
        current_ca: u16::from_be_bytes([frame[3], frame[4]]),
        consumption_mah: u16::from_be_bytes([frame[5], frame[6]]),
        erpm_div100: u16::from_be_bytes([frame[7], frame[8]]),
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParserStats {
    pub valid_frames: u32,
    pub crc_failures: u32,
    pub discarded_bytes: u32,
}

/// Fixed-storage byte-stream parser.
///
/// A CRC failure advances by one byte because the protocol has no sync byte.
/// This recovers from dropped/noisy bytes while keeping memory and work
/// bounded.
pub struct StreamParser {
    bytes: [u8; FRAME_LEN],
    len: usize,
    stats: ParserStats,
}

impl StreamParser {
    pub const fn new() -> Self {
        Self {
            bytes: [0; FRAME_LEN],
            len: 0,
            stats: ParserStats {
                valid_frames: 0,
                crc_failures: 0,
                discarded_bytes: 0,
            },
        }
    }

    pub fn push(&mut self, byte: u8) -> Option<EscTelemetry> {
        self.bytes[self.len] = byte;
        self.len += 1;
        if self.len != FRAME_LEN {
            return None;
        }

        match decode(&self.bytes) {
            Ok(sample) => {
                self.len = 0;
                self.stats.valid_frames = self.stats.valid_frames.saturating_add(1);
                Some(sample)
            }
            Err(DecodeError::Crc) => {
                self.bytes.copy_within(1..FRAME_LEN, 0);
                self.len = FRAME_LEN - 1;
                self.stats.crc_failures = self.stats.crc_failures.saturating_add(1);
                self.stats.discarded_bytes = self.stats.discarded_bytes.saturating_add(1);
                None
            }
        }
    }

    pub fn reset(&mut self) {
        self.len = 0;
    }

    pub const fn stats(&self) -> ParserStats {
        self.stats
    }
}

impl Default for StreamParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> [u8; FRAME_LEN] {
        let mut frame = [42, 0x06, 0x7c, 0x00, 0xfa, 0x01, 0x2c, 0x04, 0xd2, 0];
        frame[9] = crc8(&frame[..9]);
        frame
    }

    #[test]
    fn decodes_big_endian_wire_units() {
        assert_eq!(
            decode(&frame()),
            Ok(EscTelemetry {
                temperature_c: 42,
                voltage_cv: 1_660,
                current_ca: 250,
                consumption_mah: 300,
                erpm_div100: 1_234,
            })
        );
    }

    #[test]
    fn rejects_bad_crc() {
        let mut bad = frame();
        bad[3] ^= 1;
        assert_eq!(decode(&bad), Err(DecodeError::Crc));
    }

    #[test]
    fn stream_parser_recovers_after_one_noise_byte() {
        let expected = decode(&frame()).unwrap();
        let mut parser = StreamParser::new();
        assert_eq!(parser.push(0xaa), None);

        let mut decoded = None;
        for byte in frame() {
            decoded = parser.push(byte).or(decoded);
        }

        assert_eq!(decoded, Some(expected));
        assert_eq!(parser.stats().valid_frames, 1);
        assert_eq!(parser.stats().discarded_bytes, 1);
    }

    #[test]
    fn reset_discards_partial_frame() {
        let mut parser = StreamParser::new();
        for byte in frame().into_iter().take(5) {
            assert_eq!(parser.push(byte), None);
        }
        parser.reset();

        let mut decoded = None;
        for byte in frame() {
            decoded = parser.push(byte).or(decoded);
        }
        assert!(decoded.is_some());
    }
}
