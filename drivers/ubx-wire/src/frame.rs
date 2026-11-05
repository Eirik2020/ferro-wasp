//! UBX framing: `B5 62`, class, identifier, little-endian payload length,
//! payload, and an 8-bit Fletcher checksum over class to payload end.
//!
//! The layout and checksum agree in ublox `ubx_packets.rs:21` and
//! `parser/checksum.rs:30-31`, and kinavis-ubx `frame.rs:1-35`.

use core::fmt;

/// First synchronisation byte: ublox `constants.rs:2`, kinavis-ubx `frame.rs:18`.
pub const SYNC_1: u8 = 0xB5;
/// Second synchronisation byte: ublox `constants.rs:4`, kinavis-ubx `frame.rs:20`.
pub const SYNC_2: u8 = 0x62;
/// Sync, class, identifier and length.
pub const HEADER_LEN: usize = 6;
/// What a frame adds to its payload: the header and the two checksum bytes.
pub const OVERHEAD: usize = HEADER_LEN + 2;

/// The 8-bit Fletcher checksum `[CK_A, CK_B]` of `covered`, which is a
/// frame's class, identifier, length and payload.
pub fn checksum(covered: &[u8]) -> [u8; 2] {
    let (a, b) = covered.iter().fold((0_u8, 0_u8), |(a, b), byte| {
        let a = a.wrapping_add(*byte);
        (a, b.wrapping_add(a))
    });
    [a, b]
}

/// A frame whose checksum matched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame<'a> {
    pub class: u8,
    pub id: u8,
    pub payload: &'a [u8],
}

/// Why [`encode`] wrote nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    /// The output slice is shorter than the frame.
    BufferTooSmall { needed: usize },
    /// The payload does not fit the 16-bit length field.
    PayloadTooLong { len: usize },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall { needed } => write!(f, "the frame needs {needed} bytes"),
            Self::PayloadTooLong { len } => write!(f, "a {len}-byte payload exceeds 65535"),
        }
    }
}

impl core::error::Error for EncodeError {}

/// Write a complete frame carrying `payload` into `out`, returning its length.
/// On an error nothing in `out` is meaningful.
pub fn encode(class: u8, id: u8, payload: &[u8], out: &mut [u8]) -> Result<usize, EncodeError> {
    encode_with(class, id, payload.len(), out, |slot| {
        slot.copy_from_slice(payload);
    })
}

/// Write a frame whose `payload_len`-byte payload `fill` writes in place.
/// `fill` is given exactly `payload_len` bytes.
pub(crate) fn encode_with(
    class: u8,
    id: u8,
    payload_len: usize,
    out: &mut [u8],
    fill: impl FnOnce(&mut [u8]),
) -> Result<usize, EncodeError> {
    let length =
        u16::try_from(payload_len).map_err(|_| EncodeError::PayloadTooLong { len: payload_len })?;
    // At most 65 535 + 8, which a usize holds on every target.
    let needed = usize::from(length).saturating_add(OVERHEAD);
    let frame = out
        .get_mut(..needed)
        .ok_or(EncodeError::BufferTooSmall { needed })?;
    let [low, high] = length.to_le_bytes();
    let (header, rest) = frame.split_at_mut(HEADER_LEN);
    header.copy_from_slice(&[SYNC_1, SYNC_2, class, id, low, high]);
    let (payload, _) = rest.split_at_mut(usize::from(length));
    fill(payload);
    let check_at = needed.saturating_sub(2);
    let sums = frame.get(2..check_at).map(checksum).unwrap_or_default();
    if let Some(check) = frame.get_mut(check_at..needed) {
        check.copy_from_slice(&sums);
    }
    Ok(needed)
}

/// Why a candidate frame was dropped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeframeError {
    /// The checksum did not match.
    BadChecksum { class: u8, id: u8 },
    /// The announced payload is longer than the deframer can hold; refused
    /// from the header, before any payload was buffered.
    TooLong { class: u8, id: u8, length: u16 },
}

impl fmt::Display for DeframeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChecksum { class, id } => {
                write!(f, "UBX {class:02X}-{id:02X} failed its checksum")
            }
            Self::TooLong { class, id, length } => write!(
                f,
                "UBX {class:02X}-{id:02X} announces a {length}-byte payload, longer than the buffer"
            ),
        }
    }
}

impl core::error::Error for DeframeError {}

/// What the deframer produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event<'a> {
    Frame(Frame<'a>),
    Error(DeframeError),
}

/// Counters since construction or [`Deframer::reset`]; each wraps.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    /// Frames returned.
    pub frames: u32,
    pub bad_checksum: u32,
    pub too_long: u32,
    /// Bytes dropped outside any returned frame: NMEA, noise, and the bytes
    /// of dropped frames that held no frame of their own. Each byte counts
    /// once however often it is scanned.
    pub skipped: u32,
    /// Bytes dropped because the buffer was full of bytes not yet examined.
    /// Unreachable through the public interface; counted rather than assumed.
    pub overrun: u32,
}

/// The result of one step, without a borrow of the buffer, so a caller
/// looping over bytes can return the frame afterwards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    Pending,
    Frame,
    Error(DeframeError),
}

/// Byte-at-a-time UBX deframer over a buffer of `BUF` bytes, which holds a
/// whole frame: the largest payload it accepts is [`Self::MAX_PAYLOAD`],
/// `BUF - 8`. Stable Rust cannot size an array `N + 8`, so the const
/// parameter is the frame size rather than the payload size. `BUF` below 8
/// fails to compile.
///
/// A candidate frame stays buffered until it completes. When it fails, the
/// deframer drops only its first byte and scans the rest again, so a frame
/// that started inside a corrupt one is still returned. Running Fletcher
/// sums are kept per buffered byte, so checking any candidate costs the same
/// whatever its length: one call examines at most `BUF` bytes and computes
/// one checksum in constant time. Memory is three `BUF`-byte arrays.
///
/// One call returns at most one event. After an event, bytes already
/// buffered may hold more: [`Self::poll`] returns them without new input,
/// and the next [`Self::push`] would too.
#[derive(Clone)]
pub struct Deframer<const BUF: usize> {
    bytes: [u8; BUF],
    /// Fletcher `A` over every buffered byte up to and including this one.
    sum_a: [u8; BUF],
    /// Fletcher `B` likewise.
    sum_b: [u8; BUF],
    /// The first byte not yet consumed.
    start: usize,
    /// Bytes held.
    fill: usize,
    /// Length of the frame last returned, consumed on the next call so its
    /// payload stays readable until then.
    release: usize,
    stats: Stats,
}

impl<const BUF: usize> Default for Deframer<BUF> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const BUF: usize> fmt::Debug for Deframer<BUF> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Deframer")
            .field("buffer", &BUF)
            .field("held", &self.held())
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl<const BUF: usize> Deframer<BUF> {
    /// The largest payload accepted.
    pub const MAX_PAYLOAD: usize = BUF.saturating_sub(OVERHEAD);
    const BUFFER_HOLDS_A_FRAME: () = assert!(BUF >= OVERHEAD, "a UBX frame is at least 8 bytes");

    pub const fn new() -> Self {
        let () = Self::BUFFER_HOLDS_A_FRAME;
        Self {
            bytes: [0; BUF],
            sum_a: [0; BUF],
            sum_b: [0; BUF],
            start: 0,
            fill: 0,
            release: 0,
            stats: Stats {
                frames: 0,
                bad_checksum: 0,
                too_long: 0,
                skipped: 0,
                overrun: 0,
            },
        }
    }

    /// Drop everything buffered and zero the counters.
    pub fn reset(&mut self) {
        self.start = 0;
        self.fill = 0;
        self.release = 0;
        self.stats = Stats::default();
    }

    pub const fn stats(&self) -> Stats {
        self.stats
    }

    /// Bytes buffered and not yet consumed, at most `BUF`.
    pub const fn held(&self) -> usize {
        self.fill.saturating_sub(self.start)
    }

    /// Take one byte.
    pub fn push(&mut self, byte: u8) -> Option<Event<'_>> {
        let step = self.push_step(byte);
        self.event(step)
    }

    /// Return the next event the bytes already buffered hold, if any.
    pub fn poll(&mut self) -> Option<Event<'_>> {
        let step = self.poll_step();
        self.event(step)
    }

    pub(crate) fn push_step(&mut self, byte: u8) -> Step {
        self.retire();
        self.append(byte);
        self.advance()
    }

    pub(crate) fn poll_step(&mut self) -> Step {
        self.retire();
        self.advance()
    }

    /// The event for `step`; for [`Step::Frame`], the frame just found.
    pub(crate) fn event(&self, step: Step) -> Option<Event<'_>> {
        match step {
            Step::Pending => None,
            Step::Error(error) => Some(Event::Error(error)),
            Step::Frame => self.current_frame().map(Event::Frame),
        }
    }

    fn current_frame(&self) -> Option<Frame<'_>> {
        let header = self
            .bytes
            .get(self.start..self.start.checked_add(HEADER_LEN)?)?;
        let [_, _, class, id, _, _] = <[u8; HEADER_LEN]>::try_from(header).ok()?;
        let payload_start = self.start.checked_add(HEADER_LEN)?;
        let payload_end = self.start.checked_add(self.release)?.checked_sub(2)?;
        let payload = self.bytes.get(payload_start..payload_end)?;
        Some(Frame { class, id, payload })
    }

    fn retire(&mut self) {
        self.start = self.start.saturating_add(self.release).min(self.fill);
        self.release = 0;
        if self.start == self.fill {
            self.start = 0;
            self.fill = 0;
        }
    }

    /// Fletcher sums `(A, B)` over `bytes[..index]`, relative to an origin
    /// that only the differences in [`Self::covered_checksum`] cancel.
    fn sums_before(&self, index: usize) -> (u8, u8) {
        match index.checked_sub(1) {
            Some(last) => (
                self.sum_a.get(last).copied().unwrap_or(0),
                self.sum_b.get(last).copied().unwrap_or(0),
            ),
            None => (0, 0),
        }
    }

    /// The checksum of `bytes[from..to]` from the running sums: with `A(i)`
    /// and `B(i)` the sums over `bytes[..i]`, the range's `A` is
    /// `A(to) - A(from)` and its `B` is `B(to) - B(from) - (to - from) A(from)`,
    /// all modulo 256.
    fn covered_checksum(&self, from: usize, to: usize) -> [u8; 2] {
        let (a_from, b_from) = self.sums_before(from);
        let (a_to, b_to) = self.sums_before(to);
        // Only the length modulo 256 matters.
        let span = to.wrapping_sub(from).to_le_bytes()[0];
        [
            a_to.wrapping_sub(a_from),
            b_to.wrapping_sub(b_from)
                .wrapping_sub(span.wrapping_mul(a_from)),
        ]
    }

    /// Move the unconsumed bytes and their sums to the front. The sums move
    /// unchanged: a checksum only ever uses differences of them.
    fn compact(&mut self) {
        if self.start == 0 || self.start > self.fill || self.fill > BUF {
            return;
        }
        let range = self.start..self.fill;
        self.bytes.copy_within(range.clone(), 0);
        self.sum_a.copy_within(range.clone(), 0);
        self.sum_b.copy_within(range, 0);
        self.fill = self.fill.saturating_sub(self.start);
        self.start = 0;
    }

    fn append(&mut self, byte: u8) {
        if self.fill >= BUF {
            self.compact();
        }
        if self.fill >= BUF {
            // Every candidate in a full buffer was decided before this call,
            // so this needs a caller that ignored the buffer's invariants;
            // drop the oldest byte rather than trust that.
            self.start = 1;
            self.compact();
            self.stats.overrun = self.stats.overrun.wrapping_add(1);
        }
        let (a, b) = self.sums_before(self.fill);
        let a = a.wrapping_add(byte);
        let b = b.wrapping_add(a);
        let index = self.fill;
        if let (Some(slot), Some(sum_a), Some(sum_b)) = (
            self.bytes.get_mut(index),
            self.sum_a.get_mut(index),
            self.sum_b.get_mut(index),
        ) {
            *slot = byte;
            *sum_a = a;
            *sum_b = b;
            self.fill = index.saturating_add(1);
        }
    }

    fn byte_at(&self, offset: usize) -> Option<u8> {
        let index = self.start.checked_add(offset)?;
        if index >= self.fill {
            return None;
        }
        self.bytes.get(index).copied()
    }

    fn skip_one(&mut self) {
        self.start = self.start.saturating_add(1).min(self.fill);
        self.stats.skipped = self.stats.skipped.wrapping_add(1);
    }

    /// Examine buffered bytes until a frame or an error, or until the next
    /// decision needs a byte not yet received.
    fn advance(&mut self) -> Step {
        // Each pass consumes a byte or returns, so this ends within BUF passes.
        while self.start < self.fill {
            let Some(first) = self.byte_at(0) else {
                return Step::Pending;
            };
            if first != SYNC_1 {
                self.skip_one();
                continue;
            }
            let Some(second) = self.byte_at(1) else {
                return Step::Pending;
            };
            if second != SYNC_2 {
                self.skip_one();
                continue;
            }
            let (Some(class), Some(id), Some(low), Some(high)) = (
                self.byte_at(2),
                self.byte_at(3),
                self.byte_at(4),
                self.byte_at(5),
            ) else {
                return Step::Pending;
            };
            let length = u16::from_le_bytes([low, high]);
            let payload_len = usize::from(length);
            if payload_len > Self::MAX_PAYLOAD {
                self.skip_one();
                self.stats.too_long = self.stats.too_long.wrapping_add(1);
                return Step::Error(DeframeError::TooLong { class, id, length });
            }
            // payload_len <= BUF - 8, so these sums stay below BUF.
            let check_at = HEADER_LEN.saturating_add(payload_len);
            let (Some(ck_a), Some(ck_b)) = (
                self.byte_at(check_at),
                self.byte_at(check_at.saturating_add(1)),
            ) else {
                return Step::Pending;
            };
            let covered_from = self.start.saturating_add(2);
            let covered_to = self.start.saturating_add(check_at);
            if self.covered_checksum(covered_from, covered_to) != [ck_a, ck_b] {
                self.skip_one();
                self.stats.bad_checksum = self.stats.bad_checksum.wrapping_add(1);
                return Step::Error(DeframeError::BadChecksum { class, id });
            }
            self.release = check_at.saturating_add(2);
            self.stats.frames = self.stats.frames.wrapping_add(1);
            return Step::Frame;
        }
        Step::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(class: u8, id: u8, payload: &[u8]) -> ([u8; 300], usize) {
        let mut out = [0_u8; 300];
        let len = encode(class, id, payload, &mut out).unwrap();
        (out, len)
    }

    /// Up to 16 frames as (class, id, payload length, first payload byte),
    /// how many, up to 16 errors, and how many.
    type Seen = ([(u8, u8, usize, u8); 16], usize, [DeframeError; 16], usize);

    /// Every frame and error a byte sequence yields, draining with poll.
    fn run<const BUF: usize>(deframer: &mut Deframer<BUF>, bytes: &[u8]) -> Seen {
        let mut frames = [(0, 0, 0, 0); 16];
        let mut nframes = 0;
        let mut errors = [DeframeError::BadChecksum { class: 0, id: 0 }; 16];
        let mut nerrors = 0;
        let mut record = |event: Event<'_>| match event {
            Event::Frame(f) => {
                frames[nframes] = (
                    f.class,
                    f.id,
                    f.payload.len(),
                    *f.payload.first().unwrap_or(&0),
                );
                nframes += 1;
            }
            Event::Error(e) => {
                errors[nerrors] = e;
                nerrors += 1;
            }
        };
        for byte in bytes {
            if let Some(event) = deframer.push(*byte) {
                record(event);
                while let Some(event) = deframer.poll() {
                    record(event);
                }
            }
        }
        (frames, nframes, errors, nerrors)
    }

    // Captured frames from ublox's tests, `tests/parser_tests.rs:102`, `:852`
    // and `:1208`, and `tests/generator_test.rs:6` and `:11`.
    const KNOWN_FRAMES: [&[u8]; 5] = [
        &[0xB5, 0x62, 0x05, 0x01, 0x02, 0x00, 0x06, 0x01, 0x0F, 0x38],
        &[0xB5, 0x62, 0x05, 0x01, 0x00, 0x00, 0x06, 0x17],
        &[0xB5, 0x62, 0x05, 0x01, 0x02, 0x00, 0x04, 0x05, 0x11, 0x38],
        &[
            0xB5, 0x62, 0x06, 0x01, 0x03, 0x00, 0x01, 0x02, 0x01, 0x0E, 0x47,
        ],
        &[
            0xB5, 0x62, 0x06, 0x01, 0x03, 0x00, 0x01, 0x03, 0x01, 0x0F, 0x49,
        ],
    ];

    #[test]
    fn checksum_matches_frames_captured_by_another_implementation() {
        for known in KNOWN_FRAMES {
            let end = known.len() - 2;
            assert_eq!(checksum(&known[2..end]), [known[end], known[end + 1]]);
        }
    }

    #[test]
    fn encoding_reproduces_the_captured_frames() {
        for known in KNOWN_FRAMES {
            let (out, len) = frame(known[2], known[3], &known[6..known.len() - 2]);
            assert_eq!(&out[..len], known);
        }
    }

    #[test]
    fn encoding_refuses_a_short_buffer_and_an_oversize_payload() {
        let mut out = [0_u8; 9];
        assert_eq!(
            encode(1, 7, &[0; 2], &mut out),
            Err(EncodeError::BufferTooSmall { needed: 10 })
        );
        assert_eq!(encode(1, 7, &[0; 1], &mut out), Ok(9));
        let big = [0_u8; 65_536];
        assert_eq!(
            encode(1, 7, &big, &mut [0_u8; 8]),
            Err(EncodeError::PayloadTooLong { len: 65_536 })
        );
    }

    #[test]
    fn known_frames_deframe_after_garbage_and_nmea() {
        let mut deframer = Deframer::<64>::new();
        let mut stream = [0_u8; 128];
        let nmea = b"$GNGGA,,,,,,0,00,99.99,,,,,,*56\r\n\xB5\x00";
        stream[..nmea.len()].copy_from_slice(nmea);
        let mut at = nmea.len();
        for known in KNOWN_FRAMES {
            stream[at..at + known.len()].copy_from_slice(known);
            at += known.len();
        }
        let (frames, n, _, nerrors) = run(&mut deframer, &stream[..at]);
        assert_eq!((n, nerrors), (5, 0));
        assert_eq!(frames[3], (0x06, 0x01, 3, 0x01));
        assert_eq!(deframer.stats().skipped as usize, nmea.len());
    }

    #[test]
    fn a_frame_split_across_any_byte_boundary_deframes() {
        let (bytes, len) = frame(0x01, 0x07, &[9; 92]);
        for split in 0..=len {
            let mut deframer = Deframer::<128>::new();
            let (_, n1, _, _) = run(&mut deframer, &bytes[..split]);
            let (_, n2, _, e) = run(&mut deframer, &bytes[split..len]);
            assert_eq!((n1 + n2, e), (1, 0), "split at {split}");
        }
    }

    #[test]
    fn a_corrupt_frame_hiding_a_valid_one_yields_the_valid_one() {
        // The outer header claims 40 bytes; inside them sits a complete ACK.
        let (inner, inner_len) = frame(0x05, 0x01, &[0x06, 0x8A]);
        let mut stream = [0_u8; 80];
        stream[..6].copy_from_slice(&[SYNC_1, SYNC_2, 0x01, 0x07, 40, 0]);
        stream[8..8 + inner_len].copy_from_slice(&inner[..inner_len]);
        // Bytes 6.. to the checksum are the claimed payload; the checksum is wrong.
        let outer_len = 8 + 40;
        stream[outer_len - 2] = 0x00;
        stream[outer_len - 1] = 0x00;
        let mut deframer = Deframer::<64>::new();
        let (frames, n, errors, nerrors) = run(&mut deframer, &stream[..outer_len]);
        assert_eq!(nerrors, 1);
        assert_eq!(errors[0], DeframeError::BadChecksum { class: 1, id: 7 });
        assert_eq!(n, 1);
        assert_eq!(frames[0], (0x05, 0x01, 2, 0x06));
    }

    #[test]
    fn two_valid_frames_hidden_in_a_corrupt_one_both_come_out_in_order() {
        let (a, a_len) = frame(0x05, 0x01, &[0x06, 0x8A]);
        let (b, b_len) = frame(0x05, 0x00, &[0x06, 0x8B]);
        let mut stream = [0_u8; 80];
        stream[..6].copy_from_slice(&[SYNC_1, SYNC_2, 0x01, 0x07, 30, 0]);
        stream[6..6 + a_len].copy_from_slice(&a[..a_len]);
        stream[6 + a_len..6 + a_len + b_len].copy_from_slice(&b[..b_len]);
        let total = 8 + 30;
        let mut deframer = Deframer::<64>::new();
        let (frames, n, _, nerrors) = run(&mut deframer, &stream[..total]);
        assert_eq!((n, nerrors), (2, 1));
        assert_eq!(frames[0].1, 0x01);
        assert_eq!(frames[1].1, 0x00);
    }

    #[test]
    fn an_oversize_length_is_refused_from_the_header_and_a_later_frame_survives() {
        let mut deframer = Deframer::<32>::new();
        assert_eq!(Deframer::<32>::MAX_PAYLOAD, 24);
        let mut stream = [0_u8; 40];
        stream[..6].copy_from_slice(&[SYNC_1, SYNC_2, 0x01, 0x07, 25, 0]);
        let (ack, ack_len) = frame(0x05, 0x01, &[0x06, 0x8A]);
        stream[6..6 + ack_len].copy_from_slice(&ack[..ack_len]);
        let mut events = 0;
        let mut first_error = None;
        for (i, byte) in stream[..6 + ack_len].iter().enumerate() {
            if let Some(event) = deframer.push(*byte) {
                events += 1;
                if first_error.is_none() {
                    let error = match event {
                        Event::Error(error) => Some(error),
                        Event::Frame(_) => None,
                    };
                    first_error = Some((i, error));
                }
            }
        }
        assert_eq!(
            first_error,
            Some((
                5,
                Some(DeframeError::TooLong {
                    class: 1,
                    id: 7,
                    length: 25
                })
            ))
        );
        assert_eq!(events, 2);
        assert_eq!(deframer.stats().frames, 1);
    }

    #[test]
    fn a_maximum_size_frame_fits_exactly() {
        let payload = [0x5A; 56];
        let (bytes, len) = frame(0x01, 0x07, &payload);
        let mut deframer = Deframer::<64>::new();
        let (frames, n, _, e) = run(&mut deframer, &bytes[..len]);
        assert_eq!((n, e), (1, 0));
        assert_eq!(frames[0].2, 56);
        let (bytes, len) = frame(0x01, 0x07, &[0x5A; 57]);
        let (_, n, errors, e) = run(&mut deframer, &bytes[..len]);
        assert_eq!((n, e), (0, 1));
        assert!(matches!(
            errors[0],
            DeframeError::TooLong { length: 57, .. }
        ));
    }

    #[test]
    fn a_frame_survives_a_buffer_that_wrapped_many_times() {
        let mut deframer = Deframer::<16>::new();
        for round in 0..200_u8 {
            let (bytes, len) = frame(0x05, 0x01, &[0x06, round]);
            let mut got = None;
            for byte in &bytes[..len] {
                if let Some(Event::Frame(f)) = deframer.push(*byte) {
                    got = Some(f.payload[1]);
                }
            }
            assert_eq!(got, Some(round));
            assert!(deframer.push(0x24).is_none());
        }
        assert_eq!(deframer.stats().overrun, 0);
    }

    #[test]
    fn running_sums_agree_with_the_direct_checksum_after_compaction() {
        let mut deframer = Deframer::<24>::new();
        let mut seed = 0x1234_5678_u32;
        for _ in 0..5_000 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let _ = deframer.push(seed.to_le_bytes()[0]);
            let (start, fill) = (deframer.start, deframer.fill);
            if fill >= start + 2 {
                let direct = checksum(&deframer.bytes[start + 1..fill]);
                assert_eq!(deframer.covered_checksum(start + 1, fill), direct);
            }
        }
    }
}
