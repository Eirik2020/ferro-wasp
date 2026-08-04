//! Bounded consumers for boot-assigned receive-only serial ports.

use ferrowasp_io_core::serial::{
    RAW_LINE_MAX_LEN, RcInputSnapshot, RcProtocol, SerialPortAssignment,
};
use heapless::Vec;
use sbus_rs::StreamingParser;

/// One output produced while consuming a received UART chunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SerialConsumerEvent {
    /// A complete RC snapshot replaced the previous value.
    RcSnapshot(RcInputSnapshot),

    /// A complete COMPORT line, without its CR or LF terminator.
    ComPortLine(Vec<u8, RAW_LINE_MAX_LEN>),

    /// The current COMPORT line exceeded its fixed buffer and is being discarded.
    ComPortOverflow,
}

/// Boot-selected, allocation-free parser for one serial endpoint.
#[derive(Debug)]
pub enum SerialConsumer {
    /// No protocol consumer is active.
    Disabled,

    /// Decode a selected radio-control protocol.
    Rc(SbusConsumer),

    /// Collect CR/LF-terminated raw input lines.
    ComPort(LineConsumer),
}

impl SerialConsumer {
    /// Constructs the consumer selected by a fixed startup assignment.
    pub const fn new(assignment: SerialPortAssignment) -> Self {
        match assignment {
            SerialPortAssignment::Disabled => Self::Disabled,
            SerialPortAssignment::Rc(RcProtocol::Sbus) => Self::Rc(SbusConsumer::new()),
            SerialPortAssignment::ComPort => Self::ComPort(LineConsumer::new()),
        }
    }

    /// Consumes one bounded UART chunk and reports every completed output.
    pub fn consume(&mut self, bytes: &[u8], emit: impl FnMut(SerialConsumerEvent)) {
        match self {
            Self::Disabled => {}
            Self::Rc(consumer) => consumer.consume(bytes, emit),
            Self::ComPort(consumer) => consumer.consume(bytes, emit),
        }
    }
}

/// Stateful SBUS parser with a complete latest-value snapshot.
#[derive(Debug)]
pub struct SbusConsumer {
    parser: StreamingParser,
    snapshot: RcInputSnapshot,
}

impl SbusConsumer {
    /// Creates an empty SBUS parser and RC snapshot.
    pub const fn new() -> Self {
        Self {
            parser: StreamingParser::new(),
            snapshot: RcInputSnapshot::new(),
        }
    }

    fn consume(&mut self, bytes: &[u8], mut emit: impl FnMut(SerialConsumerEvent)) {
        for result in self.parser.push_bytes(bytes) {
            match result {
                Ok(packet) => {
                    self.snapshot.record_frame(
                        packet.channels,
                        packet.flags.d1,
                        packet.flags.d2,
                        packet.flags.frame_lost,
                        packet.flags.failsafe,
                    );
                    emit(SerialConsumerEvent::RcSnapshot(self.snapshot));
                }
                Err(_) => {
                    self.snapshot.record_parse_error();
                    emit(SerialConsumerEvent::RcSnapshot(self.snapshot));
                }
            }
        }
    }
}

impl Default for SbusConsumer {
    fn default() -> Self {
        Self::new()
    }
}

/// Fixed-capacity CR/LF line consumer for a raw COM port.
#[derive(Debug)]
pub struct LineConsumer {
    line: Vec<u8, RAW_LINE_MAX_LEN>,
    discarding: bool,
    previous_was_cr: bool,
}

impl LineConsumer {
    /// Creates an empty line consumer.
    pub const fn new() -> Self {
        Self {
            line: Vec::new(),
            discarding: false,
            previous_was_cr: false,
        }
    }

    fn consume(&mut self, bytes: &[u8], mut emit: impl FnMut(SerialConsumerEvent)) {
        for &byte in bytes {
            match byte {
                b'\r' => {
                    self.finish_line(&mut emit);
                    self.previous_was_cr = true;
                }
                b'\n' if self.previous_was_cr => {
                    self.previous_was_cr = false;
                }
                b'\n' => self.finish_line(&mut emit),
                _ => {
                    self.previous_was_cr = false;
                    if self.discarding {
                        continue;
                    }
                    if self.line.push(byte).is_err() {
                        self.line.clear();
                        self.discarding = true;
                        emit(SerialConsumerEvent::ComPortOverflow);
                    }
                }
            }
        }
    }

    fn finish_line(&mut self, emit: &mut impl FnMut(SerialConsumerEvent)) {
        if self.discarding {
            self.discarding = false;
            self.line.clear();
            return;
        }
        emit(SerialConsumerEvent::ComPortLine(core::mem::take(
            &mut self.line,
        )));
    }
}

impl Default for LineConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbus_rs::{CHANNEL_COUNT, SBUS_FOOTER, SBUS_FRAME_LENGTH, SBUS_HEADER, pack_channels};
    use std::vec::Vec as StdVec;

    fn sbus_frame(value: u16, flags: u8) -> [u8; SBUS_FRAME_LENGTH] {
        let mut frame = [0; SBUS_FRAME_LENGTH];
        frame[0] = SBUS_HEADER;
        frame[SBUS_FRAME_LENGTH - 1] = SBUS_FOOTER;
        pack_channels(&mut frame, &[value; CHANNEL_COUNT]);
        frame[23] = flags;
        frame
    }

    #[test]
    fn sbus_handles_complete_and_fragmented_frames() {
        let frame = sbus_frame(900, 0b1100);
        let mut consumer = SerialConsumer::new(SerialPortAssignment::Rc(RcProtocol::Sbus));
        let mut events = StdVec::new();
        consumer.consume(&frame[..9], |event| events.push(event));
        consumer.consume(&frame[9..], |event| events.push(event));

        let SerialConsumerEvent::RcSnapshot(snapshot) = &events[0] else {
            panic!("expected RC snapshot")
        };
        assert_eq!(snapshot.channels, [900; 16]);
        assert!(snapshot.frame_lost);
        assert!(snapshot.failsafe);
        assert_eq!(snapshot.valid_frames, 1);
    }

    #[test]
    fn sbus_parse_errors_preserve_the_last_valid_sample() {
        let valid = sbus_frame(500, 0);
        let mut invalid = sbus_frame(1000, 0);
        invalid[SBUS_FRAME_LENGTH - 1] = 0xff;
        let mut consumer = SerialConsumer::new(SerialPortAssignment::Rc(RcProtocol::Sbus));
        let mut snapshots = StdVec::new();
        consumer.consume(&valid, |event| {
            if let SerialConsumerEvent::RcSnapshot(snapshot) = event {
                snapshots.push(snapshot);
            }
        });
        consumer.consume(&invalid, |event| {
            if let SerialConsumerEvent::RcSnapshot(snapshot) = event {
                snapshots.push(snapshot);
            }
        });

        assert_eq!(snapshots.last().unwrap().channels, [500; 16]);
        assert_eq!(snapshots.last().unwrap().valid_frames, 1);
        assert_eq!(snapshots.last().unwrap().parse_errors, 1);
    }

    #[test]
    fn comport_handles_crlf_split_chunks_and_empty_lines() {
        let mut consumer = SerialConsumer::new(SerialPortAssignment::ComPort);
        let mut lines = StdVec::new();
        consumer.consume(b"first\r", |event| lines.push(event));
        consumer.consume(b"\nsec", |event| lines.push(event));
        consumer.consume(b"ond\n\n", |event| lines.push(event));

        assert_eq!(
            lines,
            [
                SerialConsumerEvent::ComPortLine(Vec::from_slice(b"first").unwrap()),
                SerialConsumerEvent::ComPortLine(Vec::from_slice(b"second").unwrap()),
                SerialConsumerEvent::ComPortLine(Vec::new()),
            ]
        );
    }

    #[test]
    fn comport_preserves_invalid_utf8_as_bytes() {
        let mut consumer = SerialConsumer::new(SerialPortAssignment::ComPort);
        let mut events = StdVec::new();
        consumer.consume(&[0xff, b'\n'], |event| events.push(event));
        let SerialConsumerEvent::ComPortLine(line) = &events[0] else {
            panic!("expected line")
        };
        assert!(core::str::from_utf8(line.as_slice()).is_err());
    }

    #[test]
    fn comport_warns_once_and_discards_an_overflowed_line() {
        let mut consumer = SerialConsumer::new(SerialPortAssignment::ComPort);
        let mut events = StdVec::new();
        consumer.consume(&[b'x'; RAW_LINE_MAX_LEN + 8], |event| events.push(event));
        consumer.consume(b"\nokay\n", |event| events.push(event));

        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, SerialConsumerEvent::ComPortOverflow))
                .count(),
            1
        );
        assert_eq!(
            events.last(),
            Some(&SerialConsumerEvent::ComPortLine(
                Vec::from_slice(b"okay").unwrap()
            ))
        );
    }
}
