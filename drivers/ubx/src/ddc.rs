//! UBX over the receiver's I2C interface, which u-blox calls DDC.
//!
//! *Inferred*: neither reference crate implements the I2C transport, so the
//! facts below have no second implementation behind them. The receiver
//! answers at 7-bit address 0x42 by default (`CFG-I2C-ADDRESS`, ublox
//! `ubx_packets/cfg_val.rs:555`, sets it; its default value is inferred).
//! Registers 0xFD and 0xFE hold the number of bytes it has ready, high byte
//! first; register 0xFF is the stream those bytes are read from. A write
//! carries UBX bytes to the receiver as a plain I2C write. A count of 0xFFFF
//! is what a bus with no receiver on it reads back.

use embedded_hal::i2c::I2c;

use crate::frame::{Deframer, Event, Step};

const REGISTER_COUNT: u8 = 0xFD;
const REGISTER_STREAM: u8 = 0xFF;

/// The largest ready count accepted by default. *Inferred*: a ceiling
/// against a count read from a misbehaving bus, not a receiver fact; a
/// larger real backlog is read anyway, `CHUNK` bytes per call, once
/// [`Ddc::with_max_pending`] allows it.
pub const DEFAULT_MAX_PENDING: u16 = 4096;

/// A 7-bit I2C address.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DdcAddress(u8);

impl DdcAddress {
    /// The receiver's default address, 0x42 (inferred; see the module).
    pub const DEFAULT: Self = Self(0x42);

    /// `None` for an address above 0x7F.
    pub const fn new(address: u8) -> Option<Self> {
        if address <= 0x7F {
            Some(Self(address))
        } else {
            None
        }
    }

    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// Why a DDC call failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DdcError<E> {
    Bus(E),
    /// A ready count of 0xFFFF, or above the configured ceiling: not a
    /// receiver's answer.
    ImplausibleCount(u16),
}

/// Reads frames from, and writes requests to, a receiver on an I2C bus.
/// Each [`Self::poll`] makes at most two bus transactions, a count read and
/// a stream read of at most `CHUNK` bytes, and pushes at most `CHUNK` bytes
/// into a [`Deframer`] of `BUF` bytes.
#[derive(Debug)]
pub struct Ddc<I, const BUF: usize, const CHUNK: usize = 32> {
    i2c: I,
    address: DdcAddress,
    max_pending: u16,
    deframer: Deframer<BUF>,
    chunk: [u8; CHUNK],
    next: usize,
    len: usize,
}

impl<I, const BUF: usize, const CHUNK: usize> Ddc<I, BUF, CHUNK> {
    pub const fn new(i2c: I, address: DdcAddress) -> Self {
        Self {
            i2c,
            address,
            max_pending: DEFAULT_MAX_PENDING,
            deframer: Deframer::new(),
            chunk: [0; CHUNK],
            next: 0,
            len: 0,
        }
    }

    /// Accept ready counts up to `max_pending`; 0xFFFF is always refused.
    pub const fn with_max_pending(mut self, max_pending: u16) -> Self {
        self.max_pending = max_pending;
        self
    }

    pub const fn deframer(&self) -> &Deframer<BUF> {
        &self.deframer
    }

    pub fn into_inner(self) -> I {
        self.i2c
    }
}

impl<I: I2c, const BUF: usize, const CHUNK: usize> Ddc<I, BUF, CHUNK> {
    /// Bytes the receiver has ready.
    pub fn pending(&mut self) -> Result<u16, DdcError<I::Error>> {
        let mut count = [0_u8; 2];
        self.i2c
            .write_read(self.address.0, &[REGISTER_COUNT], &mut count)
            .map_err(DdcError::Bus)?;
        let count = u16::from_be_bytes(count);
        if count == u16::MAX || count > self.max_pending {
            return Err(DdcError::ImplausibleCount(count));
        }
        Ok(count)
    }

    /// Read up to `buf.len()` of the bytes the receiver has ready into `buf`,
    /// returning how many. Zero, with no stream read, when none are ready.
    pub fn read_stream(&mut self, buf: &mut [u8]) -> Result<usize, DdcError<I::Error>> {
        let pending = usize::from(self.pending()?);
        let n = pending.min(buf.len());
        let Some(target) = buf.get_mut(..n) else {
            return Ok(0);
        };
        if target.is_empty() {
            return Ok(0);
        }
        self.i2c
            .write_read(self.address.0, &[REGISTER_STREAM], target)
            .map_err(DdcError::Bus)?;
        Ok(n)
    }

    /// Send `bytes`, normally one encoded frame, as one I2C write.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), DdcError<I::Error>> {
        self.i2c.write(self.address.0, bytes).map_err(DdcError::Bus)
    }

    /// The next frame or deframing error, or `None` once the bytes read so
    /// far are used up and the receiver has none ready.
    pub fn poll(&mut self) -> Result<Option<Event<'_>>, DdcError<I::Error>> {
        let step = self.step()?;
        Ok(self.deframer.event(step))
    }

    fn step(&mut self) -> Result<Step, DdcError<I::Error>> {
        let step = self.deframer.poll_step();
        if step != Step::Pending {
            return Ok(step);
        }
        if self.next >= self.len {
            self.next = 0;
            self.len = 0;
            let mut chunk = [0_u8; CHUNK];
            let read = self.read_stream(&mut chunk)?;
            self.chunk = chunk;
            self.len = read.min(CHUNK);
        }
        while self.next < self.len {
            let Some(byte) = self.chunk.get(self.next).copied() else {
                break;
            };
            self.next = self.next.saturating_add(1);
            let step = self.deframer.push_step(byte);
            if step != Step::Pending {
                return Ok(step);
            }
        }
        Ok(Step::Pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, encode};
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};

    /// A receiver with `data` ready, recording each transaction.
    struct Receiver {
        data: [u8; 64],
        len: usize,
        count_override: Option<u16>,
        log: [(u8, u8, usize); 16],
        transactions: usize,
        written: usize,
        fail: bool,
    }

    impl Receiver {
        fn new(bytes: &[u8]) -> Self {
            let mut data = [0; 64];
            data[..bytes.len()].copy_from_slice(bytes);
            Self {
                data,
                len: bytes.len(),
                count_override: None,
                log: [(0, 0, 0); 16],
                transactions: 0,
                written: 0,
                fail: false,
            }
        }
    }

    impl ErrorType for Receiver {
        type Error = ErrorKind;
    }

    impl I2c for Receiver {
        fn transaction(
            &mut self,
            address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), ErrorKind> {
            if self.fail {
                return Err(ErrorKind::Other);
            }
            let mut register = None;
            for operation in operations.iter_mut() {
                match operation {
                    Operation::Write(bytes) if bytes.len() == 1 => register = Some(bytes[0]),
                    Operation::Write(bytes) => self.written += bytes.len(),
                    Operation::Read(buf) => {
                        let reg = register.unwrap_or(REGISTER_STREAM);
                        self.log[self.transactions] = (address, reg, buf.len());
                        self.transactions += 1;
                        if reg == REGISTER_COUNT {
                            let count = self.count_override.unwrap_or(self.len as u16);
                            buf.copy_from_slice(&count.to_be_bytes());
                        } else {
                            let n = buf.len().min(self.len);
                            buf[..n].copy_from_slice(&self.data[..n]);
                            self.data.copy_within(n..self.len, 0);
                            self.len -= n;
                        }
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn a_frame_is_read_through_the_count_and_stream_registers_in_bounded_reads() {
        let mut frame = [0_u8; 16];
        let len = encode(0x05, 0x01, &[0x06, 0x8A], &mut frame).unwrap();
        let mut ddc = Ddc::<_, 32, 4>::new(Receiver::new(&frame[..len]), DdcAddress::DEFAULT);
        let mut got = None;
        for _ in 0..8 {
            if let Ok(Some(Event::Frame(Frame { class, id, .. }))) = ddc.poll() {
                got = Some((class, id));
            }
        }
        assert_eq!(got, Some((0x05, 0x01)));
        let receiver = ddc.into_inner();
        // Count, then a stream read of at most four bytes, until the frame is
        // in; then counts of zero with no stream read.
        assert_eq!(receiver.log[0], (0x42, REGISTER_COUNT, 2));
        assert_eq!(receiver.log[1], (0x42, REGISTER_STREAM, 4));
        assert_eq!(receiver.log[5], (0x42, REGISTER_STREAM, 2));
        assert_eq!(receiver.log[6], (0x42, REGISTER_COUNT, 2));
        assert_eq!(receiver.log[7], (0x42, REGISTER_COUNT, 2));
    }

    #[test]
    fn a_floating_bus_or_an_implausible_count_is_an_error() {
        let mut receiver = Receiver::new(&[]);
        receiver.count_override = Some(0xFFFF);
        let mut ddc = Ddc::<_, 32>::new(receiver, DdcAddress::DEFAULT);
        assert_eq!(ddc.poll(), Err(DdcError::ImplausibleCount(0xFFFF)));
        let mut receiver = ddc.into_inner();
        receiver.count_override = Some(5000);
        let mut ddc = Ddc::<_, 32>::new(receiver, DdcAddress::DEFAULT);
        assert_eq!(ddc.pending(), Err(DdcError::ImplausibleCount(5000)));
        let mut ddc = ddc.with_max_pending(6000);
        assert_eq!(ddc.pending(), Ok(5000));
    }

    #[test]
    fn bus_errors_and_writes_pass_through() {
        let mut receiver = Receiver::new(&[]);
        receiver.fail = true;
        let mut ddc = Ddc::<_, 32>::new(receiver, DdcAddress::new(0x43).unwrap());
        assert_eq!(ddc.poll(), Err(DdcError::Bus(ErrorKind::Other)));
        let mut receiver = ddc.into_inner();
        receiver.fail = false;
        let mut ddc = Ddc::<_, 32>::new(receiver, DdcAddress::new(0x43).unwrap());
        ddc.write(&[0xB5, 0x62, 0x06, 0x8A]).unwrap();
        assert_eq!(ddc.into_inner().written, 4);
        assert_eq!(DdcAddress::new(0x80), None);
    }
}
