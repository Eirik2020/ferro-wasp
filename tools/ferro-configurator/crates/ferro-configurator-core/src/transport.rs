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
                    self.input.extend_from_slice(&chunk[..count]);
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
