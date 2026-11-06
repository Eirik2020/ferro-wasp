use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    time::{Duration, Instant},
};

use crate::error::{FerroError, Result};

const MAX_LINE_BUFFER: usize = 4096;

pub trait LineTransport {
    fn write_line(&mut self, line: &str) -> Result<()>;
    fn read_line(&mut self, timeout: Duration) -> Result<Option<String>>;
    fn description(&self) -> &str;
}

pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
    name: String,
    input: Vec<u8>,
}

impl SerialTransport {
    pub fn open(name: &str, timeout: Duration) -> Result<Self> {
        let port = serialport::new(name, 115_200)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(timeout.min(Duration::from_millis(50)))
            .open()
            .map_err(|error| FerroError::OpenPort {
                port: name.to_owned(),
                reason: error.to_string(),
            })?;
        Ok(Self {
            port,
            name: name.to_owned(),
            input: Vec::with_capacity(512),
        })
    }

    fn take_complete_line(&mut self) -> Option<String> {
        let newline = self.input.iter().position(|byte| *byte == b'\n')?;
        let bytes: Vec<_> = self.input.drain(..=newline).collect();
        Some(
            String::from_utf8_lossy(&bytes)
                .trim_matches(['\r', '\n'])
                .to_owned(),
        )
    }
}

impl LineTransport for SerialTransport {
    fn write_line(&mut self, line: &str) -> Result<()> {
        self.port
            .write_all(line.as_bytes())
            .and_then(|_| self.port.write_all(b"\r\n"))
            .and_then(|_| self.port.flush())
            .map_err(|error| FerroError::Transport {
                port: self.name.clone(),
                reason: error.to_string(),
            })
    }

    fn read_line(&mut self, timeout: Duration) -> Result<Option<String>> {
        if let Some(line) = self.take_complete_line() {
            return Ok(Some(line));
        }
        let deadline = Instant::now() + timeout;
        let mut chunk = [0u8; 256];
        while Instant::now() < deadline {
            match self.port.read(&mut chunk) {
                Ok(0) => {}
                Ok(count) => {
                    // A UART link pads its last transmit chunk with NUL
                    // bytes; no answer contains one.
                    self.input
                        .extend(chunk[..count].iter().copied().filter(|byte| *byte != 0));
                    if self.input.len() > MAX_LINE_BUFFER {
                        self.input.clear();
                        return Err(FerroError::Transport {
                            port: self.name.clone(),
                            reason: format!("input line exceeded {MAX_LINE_BUFFER} bytes"),
                        });
                    }
                    if let Some(line) = self.take_complete_line() {
                        return Ok(Some(line));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
                Err(error) => {
                    return Err(FerroError::Transport {
                        port: self.name.clone(),
                        reason: error.to_string(),
                    });
                }
            }
        }
        Ok(None)
    }

    fn description(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Default)]
pub struct MockTransport {
    pub writes: Vec<String>,
    responses: VecDeque<Result<Option<String>>>,
    description: String,
}

impl MockTransport {
    pub fn with_lines(lines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            writes: Vec::new(),
            responses: lines
                .into_iter()
                .map(|line| Ok(Some(line.into())))
                .collect(),
            description: "mock FerroWasp".to_owned(),
        }
    }

    pub fn push_timeout(&mut self) {
        self.responses.push_back(Ok(None));
    }

    pub fn push_line(&mut self, line: impl Into<String>) {
        self.responses.push_back(Ok(Some(line.into())));
    }

    /// The port failing, as a serial port does when its cable is pulled.
    pub fn push_transport_error(&mut self, reason: impl Into<String>) {
        self.responses.push_back(Err(FerroError::Transport {
            port: self.description.clone(),
            reason: reason.into(),
        }));
    }
}

impl LineTransport for MockTransport {
    fn write_line(&mut self, line: &str) -> Result<()> {
        self.writes.push(line.to_owned());
        Ok(())
    }

    fn read_line(&mut self, _timeout: Duration) -> Result<Option<String>> {
        self.responses.pop_front().unwrap_or(Ok(None))
    }

    fn description(&self) -> &str {
        &self.description
    }
}

/// A transport is object safe, so one front-end can hold any of them behind
/// this type and pick USB or Bluetooth at run time.
impl LineTransport for Box<dyn LineTransport> {
    fn write_line(&mut self, line: &str) -> Result<()> {
        (**self).write_line(line)
    }

    fn read_line(&mut self, timeout: Duration) -> Result<Option<String>> {
        (**self).read_line(timeout)
    }

    fn description(&self) -> &str {
        (**self).description()
    }
}

/// Accumulates bytes and hands back complete `\n`-terminated lines, bounded,
/// for transports whose bytes arrive from something other than a serial
/// port — a Bluetooth bridge's notifications, for example. The line
/// convention matches [`SerialTransport`]: `\r\n` terminated, trimmed, lossy
/// UTF-8.
///
/// [`SerialTransport`] keeps its own buffer rather than using this type: it
/// also strips the NUL bytes a UART link pads its last transmit chunk with,
/// a quirk specific to that transport.
pub struct LineReader {
    input: Vec<u8>,
}

impl LineReader {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            input: Vec::with_capacity(capacity),
        }
    }

    /// Adds one chunk of received bytes, refusing a line that grows past
    /// the bound rather than parsing a line that was never whole.
    pub fn push(&mut self, chunk: &[u8], port: &str) -> Result<()> {
        self.input.extend_from_slice(chunk);
        if self.input.len() > MAX_LINE_BUFFER {
            self.input.clear();
            return Err(FerroError::Transport {
                port: port.to_owned(),
                reason: format!("input line exceeded {MAX_LINE_BUFFER} bytes"),
            });
        }
        Ok(())
    }

    /// The next complete line, if the buffer holds one.
    pub fn next_line(&mut self) -> Option<String> {
        let newline = self.input.iter().position(|byte| *byte == b'\n')?;
        let bytes: Vec<_> = self.input.drain(..=newline).collect();
        Some(
            String::from_utf8_lossy(&bytes)
                .trim_matches(['\r', '\n'])
                .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_reassembles_across_chunks() {
        let mut reader = LineReader::with_capacity(64);
        assert!(reader.push(b"OK live arm", "test").is_ok());
        assert_eq!(reader.next_line(), None);
        assert!(reader.push(b"ed=0\r\n", "test").is_ok());
        assert_eq!(reader.next_line().as_deref(), Some("OK live armed=0"));
        assert_eq!(reader.next_line(), None);
    }

    #[test]
    fn two_lines_in_one_chunk_come_out_in_order() {
        let mut reader = LineReader::with_capacity(64);
        reader.push(b"one\ntwo\n", "test").unwrap();
        assert_eq!(reader.next_line().as_deref(), Some("one"));
        assert_eq!(reader.next_line().as_deref(), Some("two"));
    }

    #[test]
    fn a_runaway_line_is_refused_whole_and_the_reader_recovers() {
        let mut reader = LineReader::with_capacity(64);
        let error = reader
            .push(&[b'x'; MAX_LINE_BUFFER + 1], "ble")
            .unwrap_err();
        assert!(matches!(error, FerroError::Transport { port, .. } if port == "ble"));
        reader.push(b"OK\n", "ble").unwrap();
        assert_eq!(reader.next_line().as_deref(), Some("OK"));
    }

    #[test]
    fn a_boxed_transport_still_speaks_lines() {
        let boxed: Box<dyn LineTransport> = Box::new(MockTransport::with_lines(["OK status fine"]));
        let mut boxed = boxed;
        boxed.write_line("status").unwrap();
        assert_eq!(
            boxed
                .read_line(Duration::from_millis(1))
                .unwrap()
                .as_deref(),
            Some("OK status fine")
        );
        assert_eq!(boxed.description(), "mock FerroWasp");
    }
}
