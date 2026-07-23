pub const MAX_PAYLOAD_LEN: usize = 64;
pub const MAX_FRAME_LEN: usize = MAX_PAYLOAD_LEN + 6;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MspPacketError {
    OutputTooSmall,
    PayloadTooLong,
    Checksum { expected: u8, calculated: u8 },
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MspDirection {
    ToFlightController,
    FromFlightController,
    Error,
}

impl MspDirection {
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'<' => Some(Self::ToFlightController),
            b'>' => Some(Self::FromFlightController),
            b'!' => Some(Self::Error),
            _ => None,
        }
    }

    pub const fn as_byte(self) -> u8 {
        match self {
            Self::ToFlightController => b'<',
            Self::FromFlightController => b'>',
            Self::Error => b'!',
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MspPacket {
    pub cmd: u8,
    pub direction: MspDirection,
    pub payload_len: usize,
    pub payload: [u8; MAX_PAYLOAD_LEN],
}

impl MspPacket {
    #[allow(dead_code)]
    pub const fn empty(cmd: u8, direction: MspDirection) -> Self {
        Self {
            cmd,
            direction,
            payload_len: 0,
            payload: [0; MAX_PAYLOAD_LEN],
        }
    }

    #[allow(dead_code)]
    pub fn payload(&self) -> &[u8] {
        &self.payload[..self.payload_len]
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum ParserState {
    HeaderStart,
    HeaderVersion,
    Direction,
    Length,
    Command,
    Payload,
    Checksum,
}

pub struct MspParser {
    state: ParserState,
    direction: MspDirection,
    cmd: u8,
    payload_len: usize,
    payload_pos: usize,
    payload: [u8; MAX_PAYLOAD_LEN],
    checksum: u8,
}

impl MspParser {
    pub const fn new() -> Self {
        Self {
            state: ParserState::HeaderStart,
            direction: MspDirection::ToFlightController,
            cmd: 0,
            payload_len: 0,
            payload_pos: 0,
            payload: [0; MAX_PAYLOAD_LEN],
            checksum: 0,
        }
    }

    pub fn parse(&mut self, byte: u8) -> Result<Option<MspPacket>, MspPacketError> {
        match self.state {
            ParserState::HeaderStart => {
                if byte == b'$' {
                    self.state = ParserState::HeaderVersion;
                }
            }
            ParserState::HeaderVersion => {
                if byte == b'M' {
                    self.state = ParserState::Direction;
                } else {
                    self.reset();
                }
            }
            ParserState::Direction => {
                if let Some(direction) = MspDirection::from_byte(byte) {
                    self.direction = direction;
                    self.state = ParserState::Length;
                } else {
                    self.reset();
                }
            }
            ParserState::Length => {
                let len = byte as usize;
                if len > MAX_PAYLOAD_LEN {
                    self.reset();
                    return Err(MspPacketError::PayloadTooLong);
                }

                self.payload_len = len;
                self.payload_pos = 0;
                self.checksum = byte;
                self.state = ParserState::Command;
            }
            ParserState::Command => {
                self.cmd = byte;
                self.checksum ^= byte;
                self.state = if self.payload_len == 0 {
                    ParserState::Checksum
                } else {
                    ParserState::Payload
                };
            }
            ParserState::Payload => {
                self.payload[self.payload_pos] = byte;
                self.payload_pos += 1;
                self.checksum ^= byte;

                if self.payload_pos == self.payload_len {
                    self.state = ParserState::Checksum;
                }
            }
            ParserState::Checksum => {
                let calculated = self.checksum;
                if byte != calculated {
                    self.reset();
                    return Err(MspPacketError::Checksum {
                        expected: byte,
                        calculated,
                    });
                }

                let packet = MspPacket {
                    cmd: self.cmd,
                    direction: self.direction,
                    payload_len: self.payload_len,
                    payload: self.payload,
                };
                self.reset();
                return Ok(Some(packet));
            }
        }

        Ok(None)
    }

    fn reset(&mut self) {
        self.state = ParserState::HeaderStart;
        self.direction = MspDirection::ToFlightController;
        self.cmd = 0;
        self.payload_len = 0;
        self.payload_pos = 0;
        self.checksum = 0;
    }
}

impl Default for MspParser {
    fn default() -> Self {
        Self::new()
    }
}

pub fn encode(
    direction: MspDirection,
    cmd: u8,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, MspPacketError> {
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(MspPacketError::PayloadTooLong);
    }

    let frame_len = payload.len() + 6;
    if output.len() < frame_len {
        return Err(MspPacketError::OutputTooSmall);
    }

    output[0] = b'$';
    output[1] = b'M';
    output[2] = direction.as_byte();
    output[3] = payload.len() as u8;
    output[4] = cmd;
    output[5..5 + payload.len()].copy_from_slice(payload);

    let mut checksum = output[3] ^ output[4];
    for byte in payload {
        checksum ^= *byte;
    }
    output[frame_len - 1] = checksum;

    Ok(frame_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_and_parses_v1_frame() {
        let mut bytes = [0; MAX_FRAME_LEN];
        let len = encode(
            MspDirection::FromFlightController,
            182,
            &[3, 5, 14, 0, b'A'],
            &mut bytes,
        )
        .unwrap();

        let mut parser = MspParser::new();
        let mut parsed = None;
        for byte in &bytes[..len] {
            parsed = parser.parse(*byte).unwrap().or(parsed);
        }

        let parsed = parsed.unwrap();
        assert_eq!(parsed.direction, MspDirection::FromFlightController);
        assert_eq!(parsed.cmd, 182);
        assert_eq!(parsed.payload(), &[3, 5, 14, 0, b'A']);
    }

    #[test]
    fn parses_frame_split_across_uart_chunks() {
        let mut bytes = [0; MAX_FRAME_LEN];
        let len = encode(
            MspDirection::ToFlightController,
            101,
            &[1, 2, 3, 4],
            &mut bytes,
        )
        .unwrap();

        let mut parser = MspParser::new();
        let mut parsed = None;

        for chunk in bytes[..len].chunks(2) {
            for byte in chunk {
                parsed = parser.parse(*byte).unwrap().or(parsed);
            }
        }

        let parsed = parsed.unwrap();
        assert_eq!(parsed.direction, MspDirection::ToFlightController);
        assert_eq!(parsed.cmd, 101);
        assert_eq!(parsed.payload(), &[1, 2, 3, 4]);
    }

    #[test]
    fn parses_back_to_back_frames_from_one_uart_chunk() {
        let mut first = [0; MAX_FRAME_LEN];
        let mut second = [0; MAX_FRAME_LEN];
        let first_len = encode(MspDirection::ToFlightController, 105, &[10], &mut first).unwrap();
        let second_len = encode(MspDirection::ToFlightController, 110, &[20], &mut second).unwrap();

        let mut parser = MspParser::new();
        let mut parsed = [None, None];
        let mut parsed_len = 0;

        for byte in first[..first_len].iter().chain(second[..second_len].iter()) {
            if let Some(packet) = parser.parse(*byte).unwrap() {
                parsed[parsed_len] = Some(packet);
                parsed_len += 1;
            }
        }

        assert_eq!(parsed_len, 2);
        assert_eq!(parsed[0].unwrap().cmd, 105);
        assert_eq!(parsed[0].unwrap().payload(), &[10]);
        assert_eq!(parsed[1].unwrap().cmd, 110);
        assert_eq!(parsed[1].unwrap().payload(), &[20]);
    }

    #[test]
    fn recovers_after_checksum_error() {
        let mut bad = [0; MAX_FRAME_LEN];
        let mut good = [0; MAX_FRAME_LEN];
        let bad_len = encode(MspDirection::ToFlightController, 101, &[1], &mut bad).unwrap();
        let good_len = encode(MspDirection::ToFlightController, 105, &[2], &mut good).unwrap();
        bad[bad_len - 1] ^= 0xff;

        let mut parser = MspParser::new();
        let mut checksum_error_seen = false;
        let mut parsed = None;

        for byte in bad[..bad_len].iter().chain(good[..good_len].iter()) {
            match parser.parse(*byte) {
                Err(MspPacketError::Checksum { .. }) => checksum_error_seen = true,
                Ok(Some(packet)) => parsed = Some(packet),
                Ok(None) => {}
                Err(other) => panic!("unexpected parser error: {:?}", other),
            }
        }

        assert!(checksum_error_seen);
        let parsed = parsed.unwrap();
        assert_eq!(parsed.cmd, 105);
        assert_eq!(parsed.payload(), &[2]);
    }
}
