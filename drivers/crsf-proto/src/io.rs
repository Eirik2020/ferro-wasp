//! A frame reader over `embedded_io::Read` and a telemetry writer over
//! `embedded_io::Write`.
//!
//! Both bound each call: the reader makes at most one read, and the writer
//! at most one write for each byte of the frame. Neither uses
//! `embedded_io::Write::write_all`, which panics when a sink accepts nothing.
//!
//! Requirements: CRSF-008, CRSF-009, CRSF-010
//! Safety-critical.

use embedded_io::{Read, ReadReady, Write};

use crate::frame::{MAX_FRAME_LEN, is_well_formed};
use crate::parser::{ParseResult, StreamingParser};

/// Bytes the reader asks its source for at once: one longest frame.
pub const READ_CHUNK_LEN: usize = MAX_FRAME_LEN;

/// Why the reader got no bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadError<E> {
    /// The source reported an error.
    Source(E),
    /// The source returned no bytes, which embedded-io defines as the end
    /// of the stream.
    EndOfStream,
}

/// Pulls bytes from an embedded-io source into the streaming parser.
///
/// The bytes after a completed frame stay buffered for the next call, so no
/// byte is lost or reordered between reads. A source error or the end of
/// the stream also resets the parser: the bytes around a fault may be lost,
/// so a partial frame is not continued across it.
#[derive(Debug)]
pub struct FrameReader<R> {
    source: R,
    parser: StreamingParser,
    chunk: [u8; READ_CHUNK_LEN],
    start: usize,
    end: usize,
}

impl<R> FrameReader<R> {
    pub const fn new(source: R) -> Self {
        Self {
            source,
            parser: StreamingParser::new(),
            chunk: [0; READ_CHUNK_LEN],
            start: 0,
            end: 0,
        }
    }

    /// Drop buffered bytes and any partial frame.
    pub fn reset(&mut self) {
        self.parser.reset();
        self.start = 0;
        self.end = 0;
    }

    /// Bytes read from the source and not yet given to the parser.
    pub const fn buffered(&self) -> usize {
        self.end - self.start
    }

    pub fn source_mut(&mut self) -> &mut R {
        &mut self.source
    }

    pub fn into_source(self) -> R {
        self.source
    }

    /// Feed buffered bytes until one completes or rejects a frame.
    fn drain(&mut self) -> Option<ParseResult> {
        while self.start < self.end {
            let byte = self.chunk.get(self.start).copied().unwrap_or_default();
            self.start += 1;
            if let Some(result) = self.parser.push_byte(byte) {
                return Some(result);
            }
        }
        None
    }
}

impl<R: Read> FrameReader<R> {
    /// Return the next frame result from buffered bytes, or, with none
    /// buffered, after exactly one read. `Ok(None)` means the bytes read so
    /// far completed no frame.
    ///
    /// The one read blocks as the source's `read` does, until at least one
    /// byte arrives; [`Self::poll_ready`] does not wait.
    pub fn poll(&mut self) -> Result<Option<ParseResult>, ReadError<R::Error>> {
        if self.start == self.end {
            self.fill()?;
        }
        Ok(self.drain())
    }

    fn fill(&mut self) -> Result<(), ReadError<R::Error>> {
        match self.source.read(&mut self.chunk) {
            Ok(0) => {
                self.reset();
                Err(ReadError::EndOfStream)
            }
            Ok(count) => {
                // A source that claims more than the buffer holds has broken
                // embedded-io's contract; only the buffer is trusted.
                self.start = 0;
                self.end = count.min(READ_CHUNK_LEN);
                Ok(())
            }
            Err(error) => {
                self.reset();
                Err(ReadError::Source(error))
            }
        }
    }
}

impl<R: Read + ReadReady> FrameReader<R> {
    /// As [`Self::poll`], but reads only when the source says a read will
    /// not block, so the call never waits.
    pub fn poll_ready(&mut self) -> Result<Option<ParseResult>, ReadError<R::Error>> {
        if self.start == self.end {
            match self.source.read_ready() {
                Ok(true) => self.fill()?,
                Ok(false) => return Ok(None),
                Err(error) => {
                    self.reset();
                    return Err(ReadError::Source(error));
                }
            }
        }
        Ok(self.drain())
    }
}

/// Why a frame was not sent whole.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteError<E> {
    /// The bytes are not one well-formed frame; nothing was written.
    Malformed,
    /// The sink reported an error after `written` bytes.
    Sink { error: E, written: usize },
    /// The sink accepted no bytes after `written` bytes.
    Stalled { written: usize },
}

/// Writes telemetry frames to an embedded-io sink.
#[derive(Debug)]
pub struct TelemetryWriter<W> {
    sink: W,
}

impl<W> TelemetryWriter<W> {
    pub const fn new(sink: W) -> Self {
        Self { sink }
    }

    pub fn sink_mut(&mut self) -> &mut W {
        &mut self.sink
    }

    pub fn into_sink(self) -> W {
        self.sink
    }
}

impl<W: Write> TelemetryWriter<W> {
    /// Write one frame whole. The frame is checked first, address, length
    /// and CRC, so only a well-formed frame reaches the sink. A write that
    /// ends early reports how many bytes went out; the receiver drops that
    /// partial frame on its CRC.
    pub fn send(&mut self, frame: &[u8]) -> Result<(), WriteError<W::Error>> {
        if !is_well_formed(frame) {
            return Err(WriteError::Malformed);
        }
        let mut written = 0;
        while let Some(rest) = frame.get(written..).filter(|rest| !rest.is_empty()) {
            match self.sink.write(rest) {
                Ok(0) => return Err(WriteError::Stalled { written }),
                Ok(count) => written += count.min(rest.len()),
                Err(error) => return Err(WriteError::Sink { error, written }),
            }
        }
        Ok(())
    }

    /// Flush the sink.
    pub fn flush(&mut self) -> Result<(), W::Error> {
        self.sink.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{CHANNEL_COUNT, pack_channels};
    use crate::frame::{ADDRESS_FLIGHT_CONTROLLER, FRAME_TYPE_RC_CHANNELS_PACKED, crc8_dvb_s2};
    use crate::parser::{DecodeError, Frame};
    use crate::telemetry::{BatteryTelemetry, Decivolts, encode_battery};
    use embedded_io::{ErrorKind, ErrorType};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct MockError;

    impl embedded_io::Error for MockError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    impl core::fmt::Display for MockError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("mock error")
        }
    }

    impl core::error::Error for MockError {}

    /// A source that hands out its bytes in chunks of a fixed size, then
    /// an error or the end of the stream, and counts its reads.
    struct ChunkedSource<'a> {
        bytes: &'a [u8],
        chunk: usize,
        fail_at_end: bool,
        claim_extra: bool,
        reads: usize,
        ready: bool,
    }

    impl<'a> ChunkedSource<'a> {
        fn new(bytes: &'a [u8], chunk: usize) -> Self {
            Self {
                bytes,
                chunk,
                fail_at_end: false,
                claim_extra: false,
                reads: 0,
                ready: true,
            }
        }
    }

    impl ErrorType for ChunkedSource<'_> {
        type Error = MockError;
    }

    impl Read for ChunkedSource<'_> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, MockError> {
            self.reads += 1;
            if self.bytes.is_empty() {
                return if self.fail_at_end {
                    Err(MockError)
                } else {
                    Ok(0)
                };
            }
            let count = self.chunk.min(buf.len()).min(self.bytes.len());
            buf[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes = &self.bytes[count..];
            Ok(if self.claim_extra { usize::MAX } else { count })
        }
    }

    impl ReadReady for ChunkedSource<'_> {
        fn read_ready(&mut self) -> Result<bool, MockError> {
            Ok(self.ready)
        }
    }

    /// A sink that accepts at most `per_write` bytes a call, and after
    /// `budget` bytes either stalls or fails.
    struct LimitedSink {
        written: [u8; 256],
        len: usize,
        per_write: usize,
        budget: usize,
        fail: bool,
        writes: usize,
    }

    impl LimitedSink {
        fn new(per_write: usize, budget: usize, fail: bool) -> Self {
            Self {
                written: [0; 256],
                len: 0,
                per_write,
                budget,
                fail,
                writes: 0,
            }
        }

        fn bytes(&self) -> &[u8] {
            &self.written[..self.len]
        }
    }

    impl ErrorType for LimitedSink {
        type Error = MockError;
    }

    impl Write for LimitedSink {
        fn write(&mut self, buf: &[u8]) -> Result<usize, MockError> {
            self.writes += 1;
            let room = self.budget - self.len;
            if room == 0 {
                return if self.fail { Err(MockError) } else { Ok(0) };
            }
            let count = buf.len().min(self.per_write).min(room);
            self.written[self.len..self.len + count].copy_from_slice(&buf[..count]);
            self.len += count;
            Ok(count)
        }

        fn flush(&mut self) -> Result<(), MockError> {
            Ok(())
        }
    }

    fn rc_frame(channels: &[u16; CHANNEL_COUNT]) -> [u8; 26] {
        let mut frame = [0_u8; 26];
        frame[0] = ADDRESS_FLIGHT_CONTROLLER;
        frame[1] = 24;
        frame[2] = FRAME_TYPE_RC_CHANNELS_PACKED;
        frame[3..25].copy_from_slice(&pack_channels(channels));
        frame[25] = crc8_dvb_s2(&frame[2..25]);
        frame
    }

    fn stream_of_three() -> ([u8; 26 * 3 + 2], [[u16; CHANNEL_COUNT]; 3]) {
        let sets = [
            [992; CHANNEL_COUNT],
            [172; CHANNEL_COUNT],
            [1811; CHANNEL_COUNT],
        ];
        let mut stream = [0_u8; 26 * 3 + 2];
        stream[0] = 0x00;
        stream[1] = 0x55;
        for (index, channels) in sets.iter().enumerate() {
            stream[2 + index * 26..2 + (index + 1) * 26].copy_from_slice(&rc_frame(channels));
        }
        (stream, sets)
    }

    #[test]
    fn the_reader_delivers_every_frame_in_order_whatever_the_chunking() {
        let (stream, sets) = stream_of_three();
        for chunk in 1..=READ_CHUNK_LEN {
            let mut reader = FrameReader::new(ChunkedSource::new(&stream, chunk));
            let mut seen = 0;
            loop {
                let reads_before = reader.source_mut().reads;
                match reader.poll() {
                    Ok(Some(result)) => {
                        assert_eq!(result, Ok(Frame::RcChannels(sets[seen])), "chunk {chunk}");
                        seen += 1;
                    }
                    Ok(None) => {}
                    Err(ReadError::EndOfStream) => break,
                    Err(error) => panic!("chunk {chunk}: {error:?}"),
                }
                assert!(reader.source_mut().reads - reads_before <= 1);
            }
            assert_eq!(seen, 3, "chunk {chunk}");
        }
    }

    #[test]
    fn bytes_after_a_frame_stay_buffered_for_the_next_call() {
        let (stream, sets) = stream_of_three();
        let mut reader = FrameReader::new(ChunkedSource::new(&stream, READ_CHUNK_LEN));
        assert_eq!(reader.poll(), Ok(Some(Ok(Frame::RcChannels(sets[0])))));
        assert_eq!(reader.buffered(), READ_CHUNK_LEN - 28);
        assert_eq!(reader.poll(), Ok(Some(Ok(Frame::RcChannels(sets[1])))));
        assert_eq!(reader.source_mut().reads, 1);
    }

    #[test]
    fn a_source_error_or_end_of_stream_is_an_error_and_resets_the_parser() {
        let (stream, sets) = stream_of_three();
        // The stream ends ten bytes into the first frame.
        let mut source = ChunkedSource::new(&stream[..12], 64);
        source.fail_at_end = true;
        let mut reader = FrameReader::new(source);
        assert_eq!(reader.poll(), Ok(None));
        assert_eq!(reader.poll(), Err(ReadError::Source(MockError)));
        assert_eq!(reader.buffered(), 0);

        // After the error the next frame starts clean, so the remainder of
        // the cut frame is not glued onto it.
        let mut reader = FrameReader::new(ChunkedSource::new(&stream[..12], 64));
        assert_eq!(reader.poll(), Ok(None));
        assert_eq!(reader.poll(), Err(ReadError::EndOfStream));
        *reader.source_mut() = ChunkedSource::new(&stream[28..54], 64);
        assert_eq!(reader.poll(), Ok(Some(Ok(Frame::RcChannels(sets[1])))));
    }

    #[test]
    fn a_source_claiming_more_than_the_buffer_is_held_to_the_buffer() {
        let (stream, sets) = stream_of_three();
        let mut source = ChunkedSource::new(&stream, 64);
        source.claim_extra = true;
        let mut reader = FrameReader::new(source);
        assert_eq!(reader.poll(), Ok(Some(Ok(Frame::RcChannels(sets[0])))));
        assert!(reader.buffered() <= READ_CHUNK_LEN);
    }

    #[test]
    fn poll_ready_does_not_read_a_source_that_is_not_ready() {
        let (stream, sets) = stream_of_three();
        let mut source = ChunkedSource::new(&stream, 64);
        source.ready = false;
        let mut reader = FrameReader::new(source);
        assert_eq!(reader.poll_ready(), Ok(None));
        assert_eq!(reader.source_mut().reads, 0);
        reader.source_mut().ready = true;
        assert_eq!(
            reader.poll_ready(),
            Ok(Some(Ok(Frame::RcChannels(sets[0]))))
        );
        assert_eq!(reader.source_mut().reads, 1);
    }

    #[test]
    fn the_reader_reports_corrupt_frames_as_the_parser_does() {
        let mut frame = rc_frame(&[992; CHANNEL_COUNT]);
        frame[7] ^= 0x10;
        let mut reader = FrameReader::new(ChunkedSource::new(&frame, 64));
        assert_eq!(reader.poll(), Ok(Some(Err(DecodeError::Crc))));
    }

    #[test]
    fn the_writer_sends_a_frame_whole_across_short_writes() {
        let frame = encode_battery(BatteryTelemetry {
            voltage: Decivolts(168),
            ..BatteryTelemetry::default()
        });
        for per_write in 1..=frame.len() {
            let mut writer = TelemetryWriter::new(LimitedSink::new(per_write, 256, false));
            assert_eq!(writer.send(&frame), Ok(()));
            assert_eq!(writer.flush(), Ok(()));
            let sink = writer.into_sink();
            assert_eq!(sink.bytes(), frame);
            assert_eq!(sink.writes, frame.len().div_ceil(per_write));
        }
    }

    #[test]
    fn a_stalled_or_failing_sink_is_an_error_not_a_sent_frame() {
        let frame = encode_battery(BatteryTelemetry::default());
        let mut writer = TelemetryWriter::new(LimitedSink::new(4, 5, false));
        assert_eq!(writer.send(&frame), Err(WriteError::Stalled { written: 5 }));
        let mut writer = TelemetryWriter::new(LimitedSink::new(4, 5, true));
        assert_eq!(
            writer.send(&frame),
            Err(WriteError::Sink {
                error: MockError,
                written: 5
            })
        );
        let mut writer = TelemetryWriter::new(LimitedSink::new(4, 0, false));
        assert_eq!(writer.send(&frame), Err(WriteError::Stalled { written: 0 }));
    }

    #[test]
    fn a_malformed_frame_never_reaches_the_sink() {
        let frame = encode_battery(BatteryTelemetry::default());
        let mut corrupt = frame;
        corrupt[4] ^= 1;
        let mut writer = TelemetryWriter::new(LimitedSink::new(64, 256, false));
        assert_eq!(writer.send(&corrupt), Err(WriteError::Malformed));
        assert_eq!(
            writer.send(&frame[..frame.len() - 1]),
            Err(WriteError::Malformed)
        );
        assert_eq!(writer.send(&[]), Err(WriteError::Malformed));
        let mut readdressed = frame;
        readdressed[0] = 0x00;
        assert_eq!(writer.send(&readdressed), Err(WriteError::Malformed));
        assert_eq!(writer.sink_mut().writes, 0);
    }
}
