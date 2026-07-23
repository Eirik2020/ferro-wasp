pub const MAX_PAYLOAD_LEN: usize = 1024;
pub const MAX_FRAME_LEN: usize = MAX_PAYLOAD_LEN + 9;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MspPacketError {
    OutputTooSmall,
    PayloadTooLong,
    Checksum { received: u8, calculated: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MspPacket {
    pub direction: MspDirection,
    pub flags: u8,
    pub function: u16,
    payload_len: u16,
    payload: [u8; MAX_PAYLOAD_LEN],
}

impl MspPacket {
    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.payload_len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParserState {
    HeaderStart,
    HeaderVersion,
    Direction,
    Flags,
    FunctionLow,
    FunctionHigh,
    LengthLow,
    LengthHigh,
    Payload,
    Checksum,
}

pub struct MspParser {
    state: ParserState,
    direction: MspDirection,
    flags: u8,
    function: u16,
    payload_len: u16,
    payload_pos: u16,
    payload: [u8; MAX_PAYLOAD_LEN],
    checksum: u8,
}

impl MspParser {
    pub const fn new() -> Self {
        Self {
            state: ParserState::HeaderStart,
            direction: MspDirection::ToFlightController,
            flags: 0,
            function: 0,
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
                if byte == b'X' {
                    self.state = ParserState::Direction;
                } else if byte != b'$' {
                    self.reset();
                }
            }
            ParserState::Direction => {
                if let Some(direction) = MspDirection::from_byte(byte) {
                    self.direction = direction;
                    self.state = ParserState::Flags;
                } else {
                    self.reset_with_possible_prefix(byte);
                }
            }
            ParserState::Flags => {
                self.flags = byte;
                self.checksum = crc8_dvb_s2_update(0, byte);
                self.state = ParserState::FunctionLow;
            }
            ParserState::FunctionLow => {
                self.function = u16::from(byte);
                self.checksum = crc8_dvb_s2_update(self.checksum, byte);
                self.state = ParserState::FunctionHigh;
            }
            ParserState::FunctionHigh => {
                self.function |= u16::from(byte) << 8;
                self.checksum = crc8_dvb_s2_update(self.checksum, byte);
                self.state = ParserState::LengthLow;
            }
            ParserState::LengthLow => {
                self.payload_len = u16::from(byte);
                self.checksum = crc8_dvb_s2_update(self.checksum, byte);
                self.state = ParserState::LengthHigh;
            }
            ParserState::LengthHigh => {
                self.payload_len |= u16::from(byte) << 8;
                self.checksum = crc8_dvb_s2_update(self.checksum, byte);
                if usize::from(self.payload_len) > MAX_PAYLOAD_LEN {
                    self.reset();
                    return Err(MspPacketError::PayloadTooLong);
                }
                self.payload_pos = 0;
                self.state = if self.payload_len == 0 {
                    ParserState::Checksum
                } else {
                    ParserState::Payload
                };
            }
            ParserState::Payload => {
                self.payload[usize::from(self.payload_pos)] = byte;
                self.payload_pos += 1;
                self.checksum = crc8_dvb_s2_update(self.checksum, byte);
                if self.payload_pos == self.payload_len {
                    self.state = ParserState::Checksum;
                }
            }
            ParserState::Checksum => {
                let calculated = self.checksum;
                if byte != calculated {
                    self.reset_with_possible_prefix(byte);
                    return Err(MspPacketError::Checksum {
                        received: byte,
                        calculated,
                    });
                }
                let packet = MspPacket {
                    direction: self.direction,
                    flags: self.flags,
                    function: self.function,
                    payload_len: self.payload_len,
                    payload: self.payload,
                };
                self.reset();
                return Ok(Some(packet));
            }
        }
        Ok(None)
    }

    pub fn clear(&mut self) {
        self.reset();
    }

    fn reset_with_possible_prefix(&mut self, byte: u8) {
        self.reset();
        if byte == b'$' {
            self.state = ParserState::HeaderVersion;
        }
    }

    fn reset(&mut self) {
        self.state = ParserState::HeaderStart;
        self.direction = MspDirection::ToFlightController;
        self.flags = 0;
        self.function = 0;
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

pub const fn crc8_dvb_s2_update(mut crc: u8, byte: u8) -> u8 {
    crc ^= byte;
    let mut bit = 0;
    while bit < 8 {
        crc = if crc & 0x80 != 0 {
            (crc << 1) ^ 0xd5
        } else {
            crc << 1
        };
        bit += 1;
    }
    crc
}

pub fn crc8_dvb_s2(bytes: &[u8]) -> u8 {
    bytes
        .iter()
        .fold(0, |crc, byte| crc8_dvb_s2_update(crc, *byte))
}

pub fn encode(
    direction: MspDirection,
    flags: u8,
    function: u16,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, MspPacketError> {
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(MspPacketError::PayloadTooLong);
    }
    let frame_len = payload.len() + 9;
    if output.len() < frame_len {
        return Err(MspPacketError::OutputTooSmall);
    }
    output[..3].copy_from_slice(&[b'$', b'X', direction.as_byte()]);
    output[3] = flags;
    output[4..6].copy_from_slice(&function.to_le_bytes());
    output[6..8].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    output[8..8 + payload.len()].copy_from_slice(payload);
    output[frame_len - 1] = crc8_dvb_s2(&output[3..frame_len - 1]);
    Ok(frame_len)
}

pub struct EncodedFrame {
    bytes: [u8; MAX_FRAME_LEN],
    len: u16,
    offset: u16,
}

impl EncodedFrame {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_FRAME_LEN],
            len: 0,
            offset: 0,
        }
    }

    pub fn set(
        &mut self,
        direction: MspDirection,
        flags: u8,
        function: u16,
        payload: &[u8],
    ) -> Result<(), MspPacketError> {
        self.len = encode(direction, flags, function, payload, &mut self.bytes)? as u16;
        self.offset = 0;
        Ok(())
    }

    pub fn is_pending(&self) -> bool {
        self.offset < self.len
    }

    pub fn remaining(&self) -> &[u8] {
        &self.bytes[usize::from(self.offset)..usize::from(self.len)]
    }

    pub fn advance(&mut self, count: usize) {
        self.offset = self
            .offset
            .saturating_add(count.min(self.remaining().len()) as u16);
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.offset = 0;
    }
}

impl Default for EncodedFrame {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_example(payload: &[u8]) -> ([u8; MAX_FRAME_LEN], usize) {
        let mut output = [0; MAX_FRAME_LEN];
        let len = encode(
            MspDirection::ToFlightController,
            0,
            0x7a00,
            payload,
            &mut output,
        )
        .unwrap();
        (output, len)
    }

    #[test]
    fn crc_matches_the_standard_check_vector() {
        assert_eq!(crc8_dvb_s2(b"123456789"), 0xbc);
    }

    #[test]
    fn parser_accepts_split_and_back_to_back_frames() {
        let (first, first_len) = encode_example(&[1, 2, 3]);
        let (second, second_len) = encode_example(&[4, 5]);
        let mut parser = MspParser::new();
        let mut packets = heapless::Vec::<MspPacket, 2>::new();
        for byte in b"noise"
            .iter()
            .chain(first[..first_len].iter())
            .chain(second[..second_len].iter())
        {
            if let Some(packet) = parser.parse(*byte).unwrap() {
                packets.push(packet).unwrap();
            }
        }
        assert_eq!(packets[0].payload(), &[1, 2, 3]);
        assert_eq!(packets[1].payload(), &[4, 5]);
    }

    #[test]
    fn parser_rejects_crc_and_recovers() {
        let (mut bad, bad_len) = encode_example(&[1]);
        bad[bad_len - 1] ^= 1;
        let (good, good_len) = encode_example(&[2]);
        let mut parser = MspParser::new();
        let mut saw_error = false;
        let mut parsed = None;
        for byte in bad[..bad_len].iter().chain(good[..good_len].iter()) {
            match parser.parse(*byte) {
                Err(MspPacketError::Checksum { .. }) => saw_error = true,
                Ok(Some(packet)) => parsed = Some(packet),
                Ok(None) => {}
                Err(other) => panic!("unexpected parser error: {other:?}"),
            }
        }
        assert!(saw_error);
        assert_eq!(parsed.unwrap().payload(), &[2]);
    }

    #[test]
    fn oversized_declared_payload_is_rejected_before_body_buffering() {
        let mut parser = MspParser::new();
        let header = [b'$', b'X', b'<', 0, 0, 0, 1, 4];
        let mut result = Ok(None);
        for byte in header {
            result = parser.parse(byte);
        }
        assert_eq!(result, Err(MspPacketError::PayloadTooLong));
    }

    #[test]
    fn pending_frame_supports_partial_transport_writes() {
        let mut frame = EncodedFrame::new();
        frame
            .set(MspDirection::FromFlightController, 0, 1, &[1, 2, 3])
            .unwrap();
        assert_eq!(frame.remaining().len(), 12);
        frame.advance(5);
        assert_eq!(frame.remaining().len(), 7);
        frame.advance(99);
        assert!(!frame.is_pending());
    }
}
