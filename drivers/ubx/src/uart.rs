//! UBX over a serial port, through `embedded-io`.

use embedded_io::{Read, ReadReady, Write, WriteReady};

use crate::frame::{Deframer, Event, Step};

/// Why [`UartReader::poll`] stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadError<E> {
    Port(E),
    /// The port reported data ready, then returned none: an end of stream.
    EndOfStream,
}

/// Reads frames from a serial port without blocking: a [`Self::poll`] reads
/// only when the port says a read will not block, takes at most `CHUNK`
/// bytes from it in one read, and pushes at most `CHUNK` bytes into a
/// [`Deframer`] of `BUF` bytes.
#[derive(Debug)]
pub struct UartReader<R, const BUF: usize, const CHUNK: usize = 32> {
    port: R,
    deframer: Deframer<BUF>,
    chunk: [u8; CHUNK],
    next: usize,
    len: usize,
}

impl<R, const BUF: usize, const CHUNK: usize> UartReader<R, BUF, CHUNK> {
    pub const fn new(port: R) -> Self {
        Self {
            port,
            deframer: Deframer::new(),
            chunk: [0; CHUNK],
            next: 0,
            len: 0,
        }
    }

    pub const fn deframer(&self) -> &Deframer<BUF> {
        &self.deframer
    }

    pub fn into_inner(self) -> R {
        self.port
    }
}

impl<R: Read + ReadReady, const BUF: usize, const CHUNK: usize> UartReader<R, BUF, CHUNK> {
    /// The next frame or deframing error, or `None` once the bytes read so
    /// far are used up and the port has no more ready. Call again until
    /// `None` to drain a burst.
    pub fn poll(&mut self) -> Result<Option<Event<'_>>, ReadError<R::Error>> {
        let step = self.step()?;
        Ok(self.deframer.event(step))
    }

    fn step(&mut self) -> Result<Step, ReadError<R::Error>> {
        let step = self.deframer.poll_step();
        if step != Step::Pending {
            return Ok(step);
        }
        if self.next >= self.len {
            self.next = 0;
            self.len = 0;
            if !self.port.read_ready().map_err(ReadError::Port)? {
                return Ok(Step::Pending);
            }
            let read = self.port.read(&mut self.chunk).map_err(ReadError::Port)?;
            if read == 0 {
                return Err(ReadError::EndOfStream);
            }
            self.len = read.min(CHUNK);
        }
        while let Some(byte) = self.chunk.get(self.next).copied() {
            if self.next >= self.len {
                break;
            }
            self.next = self.next.saturating_add(1);
            let step = self.deframer.push_step(byte);
            if step != Step::Pending {
                return Ok(step);
            }
        }
        Ok(Step::Pending)
    }
}

/// Writes complete frames to a serial port.
#[derive(Debug)]
pub struct UartWriter<W> {
    port: W,
}

impl<W> UartWriter<W> {
    pub const fn new(port: W) -> Self {
        Self { port }
    }

    pub fn into_inner(self) -> W {
        self.port
    }
}

impl<W: Write> UartWriter<W> {
    /// Write all of `frame` and flush. This blocks until the port has taken
    /// every byte: for initialisation, not for a real-time task.
    pub fn send(&mut self, frame: &[u8]) -> Result<(), W::Error> {
        self.port.write_all(frame)?;
        self.port.flush()
    }
}

impl<W: Write + WriteReady> UartWriter<W> {
    /// Write what the port takes now without blocking, and return how many
    /// bytes of `rest` it took; the caller resends the remainder later. Zero
    /// when the port is not ready.
    pub fn send_some(&mut self, rest: &[u8]) -> Result<usize, W::Error> {
        if rest.is_empty() || !self.port.write_ready()? {
            return Ok(0);
        }
        self.port.write(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, encode};
    use core::convert::Infallible;

    struct Port<'a> {
        data: &'a [u8],
        per_read: usize,
        reads: usize,
    }

    impl embedded_io::ErrorType for Port<'_> {
        type Error = Infallible;
    }

    impl Read for Port<'_> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Infallible> {
            self.reads += 1;
            let n = buf.len().min(self.per_read).min(self.data.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    impl ReadReady for Port<'_> {
        fn read_ready(&mut self) -> Result<bool, Infallible> {
            Ok(!self.data.is_empty())
        }
    }

    #[test]
    fn frames_in_one_read_all_come_out_and_idle_reads_nothing() {
        let mut stream = [0_u8; 64];
        let mut at = 0;
        for id in 0..3 {
            at += encode(0x05, id, &[0x06, 0x8A], &mut stream[at..]).unwrap();
        }
        let mut reader = UartReader::<_, 64, 64>::new(Port {
            data: &stream[..at],
            per_read: 64,
            reads: 0,
        });
        let mut ids = [0_u8; 3];
        for slot in &mut ids {
            match reader.poll() {
                Ok(Some(Event::Frame(Frame { id, .. }))) => *slot = id,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(ids, [0, 1, 2]);
        assert_eq!(reader.poll(), Ok(None));
        assert_eq!(reader.into_inner().reads, 1);
    }

    #[test]
    fn a_frame_across_small_reads_comes_out_once() {
        let mut stream = [0_u8; 32];
        let len = encode(0x05, 0x01, &[0x06, 0x8A], &mut stream).unwrap();
        let mut reader = UartReader::<_, 32, 3>::new(Port {
            data: &stream[..len],
            per_read: 3,
            reads: 0,
        });
        let mut frames = 0;
        for _ in 0..10 {
            if let Ok(Some(Event::Frame(_))) = reader.poll() {
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    struct Sink {
        taken: [u8; 16],
        len: usize,
        ready: bool,
    }

    impl embedded_io::ErrorType for Sink {
        type Error = Infallible;
    }

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> Result<usize, Infallible> {
            let n = buf.len().min(4).min(self.taken.len() - self.len);
            self.taken[self.len..self.len + n].copy_from_slice(&buf[..n]);
            self.len += n;
            Ok(n)
        }

        fn flush(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }

    impl WriteReady for Sink {
        fn write_ready(&mut self) -> Result<bool, Infallible> {
            Ok(self.ready)
        }
    }

    #[test]
    fn send_writes_the_whole_frame_and_send_some_only_when_ready() {
        let frame = [0xB5, 0x62, 0x05, 0x01, 0x02, 0x00, 0x06, 0x01, 0x0F, 0x38];
        let mut writer = UartWriter::new(Sink {
            taken: [0; 16],
            len: 0,
            ready: false,
        });
        writer.send(&frame).unwrap();
        assert_eq!(writer.send_some(&frame), Ok(0));
        let mut sink = writer.into_inner();
        assert_eq!(&sink.taken[..sink.len], &frame);
        sink.ready = true;
        let mut writer = UartWriter::new(sink);
        assert_eq!(writer.send_some(&frame), Ok(4));
    }
}
