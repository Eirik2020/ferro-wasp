//! The adapter against a byte stream of `NAV-PVT` frames, driven through
//! the real `ubx` driver's reader. The frames are encoded by the driver's
//! own encoder, so a frame that decodes here would decode on a receiver's
//! wire; the values are the ones the driver's decoder tests pin.

use std::cell::Cell;
use std::rc::Rc;

use embedded_io::{ErrorKind, ErrorType, Read, ReadReady};
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_io_contracts::time::TimestampMicros;
use ferrowasp_sensor_traits::class::{Bringup, Clock, Sensor, SensorError, TimeoutStage};
use ferrowasp_sensor_traits::gnss::{
    AccuracyDegrees1e5, AccuracyMillimetres, AccuracyMillimetresPerSecond, Dop, FixKind, GnssFix,
    GnssSolution, Millimetres, MillimetresPerSecond, Position, ReportedFix, UtcTime, Velocity,
};

use ubx::encode;
use ubx::id::{ACK_ACK, CLASS_ACK, CLASS_NAV, NAV_PVT};

use super::{UbxNavPvt, UbxTiming};

const START_US: u64 = 1_000_000;

/// The model's clock, shared with the port so a byte costs time.
#[derive(Clone, Default)]
struct Shared {
    now: Cell<u64>,
    per_byte_us: Cell<u64>,
}

struct Ticker {
    shared: Rc<Shared>,
}

impl Ticker {
    fn new(shared: &Rc<Shared>) -> Self {
        Self {
            shared: Rc::clone(shared),
        }
    }

    fn advance(&self, micros: u64) {
        self.shared
            .now
            .set(self.shared.now.get().saturating_add(micros));
    }

    fn set_per_byte(&self, micros: u64) {
        self.shared.per_byte_us.set(micros);
    }
}

impl Clock for Ticker {
    fn now(&mut self) -> TimestampMicros {
        TimestampMicros(self.shared.now.get())
    }
}

/// A serial port over a fixed byte stream, failing on command, costing
/// time per byte.
struct Port {
    data: std::vec::Vec<u8>,
    next: usize,
    fail: Rc<Cell<bool>>,
    shared: Rc<Shared>,
}

impl Port {
    fn over(shared: &Rc<Shared>, stream: &[u8], fail: &Rc<Cell<bool>>) -> Self {
        Self {
            data: stream.to_vec(),
            next: 0,
            fail: Rc::clone(fail),
            shared: Rc::clone(shared),
        }
    }
}

impl ErrorType for Port {
    type Error = ErrorKind;
}

impl Read for Port {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if self.fail.get() {
            return Err(ErrorKind::Other);
        }
        let remaining = self.data.len() - self.next;
        let taken = buf.len().min(remaining);
        if taken == 0 {
            return Ok(0);
        }
        buf[..taken].copy_from_slice(&self.data[self.next..self.next + taken]);
        self.next += taken;
        self.shared.now.set(
            self.shared
                .now
                .get()
                .saturating_add(taken as u64 * self.shared.per_byte_us.get()),
        );
        Ok(taken)
    }
}

impl ReadReady for Port {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        if self.fail.get() {
            return Err(ErrorKind::Other);
        }
        Ok(self.next < self.data.len())
    }
}

/// One `NAV-PVT` payload with distinct values everywhere, so a wrong
/// offset reads a wrong value: the same layout the `ubx` decoder's own
/// tests pin, with the fix fields the caller asks for.
fn nav_pvt(fix_type: u8, flags: u8, valid: u8, lat_e7: i32, lon_e7: i32) -> [u8; 92] {
    fn put(p: &mut [u8; 92], at: usize, bytes: &[u8]) {
        p[at..at + bytes.len()].copy_from_slice(bytes);
    }
    let mut p = [0_u8; 92];
    put(&mut p, 4, &2026_u16.to_le_bytes());
    put(&mut p, 6, &[11, 4, 13, 37, 59]);
    p[11] = valid;
    put(&mut p, 12, &25_u32.to_le_bytes());
    put(&mut p, 16, &(-123_456_i32).to_le_bytes());
    p[20] = fix_type;
    p[21] = flags;
    p[23] = 10;
    put(&mut p, 24, &lon_e7.to_le_bytes());
    put(&mut p, 28, &lat_e7.to_le_bytes());
    put(&mut p, 32, &247_522_i32.to_le_bytes());
    put(&mut p, 36, &265_694_i32.to_le_bytes());
    put(&mut p, 40, &7_915_u32.to_le_bytes());
    put(&mut p, 44, &11_179_u32.to_le_bytes());
    put(&mut p, 48, &46_i32.to_le_bytes());
    put(&mut p, 52, &249_i32.to_le_bytes());
    put(&mut p, 56, &(-69_i32).to_le_bytes());
    put(&mut p, 60, &253_i32.to_le_bytes());
    put(&mut p, 64, &6_753_761_i32.to_le_bytes());
    put(&mut p, 68, &321_u32.to_le_bytes());
    put(&mut p, 72, &1_234_567_u32.to_le_bytes());
    put(&mut p, 76, &230_u16.to_le_bytes());
    put(&mut p, 84, &(-2_000_000_i32).to_le_bytes());
    put(&mut p, 88, &(-345_i16).to_le_bytes());
    put(&mut p, 90, &77_u16.to_le_bytes());
    p
}

/// The payload of a good 3D fix: fix type 3, `gnssFixOK` set, every UTC
/// validity bit set, at 46.4° N, 11.7° E.
fn good() -> [u8; 92] {
    nav_pvt(3, 0x01, 0x07, 464_045_956, -1_170_207_302)
}

/// A whole stream of `NAV-PVT` frames, encoded by the driver itself.
fn stream(frames: &[[u8; 92]]) -> std::vec::Vec<u8> {
    let mut out = std::vec::Vec::new();
    for frame in frames {
        let mut buffer = [0_u8; 128];
        let len = encode(CLASS_NAV, NAV_PVT, frame, &mut buffer).unwrap();
        out.extend_from_slice(&buffer[..len]);
    }
    out
}

/// An adapter over a port with the given stream, its clock handle, and
/// the port's failure knob.
fn adapter(stream: &[u8]) -> (UbxNavPvt<Port, 256, 32, Ticker>, Ticker, Rc<Cell<bool>>) {
    let shared = Rc::new(Shared {
        now: Cell::new(START_US),
        per_byte_us: Cell::new(0),
    });
    let fail = Rc::new(Cell::new(false));
    let port = Port::over(&shared, stream, &fail);
    let gnss = UbxNavPvt::new(port, Ticker::new(&shared), UbxTiming::default());
    (gnss, Ticker::new(&shared), fail)
}

/// A full solution as the class expects it, for comparing.
fn expected_solution() -> GnssSolution {
    GnssSolution {
        fix: GnssFix::Valid {
            kind: FixKind::ThreeD,
            position: Position::new(
                464_045_956,
                -1_170_207_302,
                Millimetres(247_522),
                Millimetres(265_694),
                AccuracyMillimetres(7_915),
                AccuracyMillimetres(11_179),
            )
            .unwrap(),
            velocity: Velocity::new(
                MillimetresPerSecond(46),
                MillimetresPerSecond(249),
                MillimetresPerSecond(-69),
                MillimetresPerSecond(253),
                6_753_761,
                AccuracyMillimetresPerSecond(321),
                AccuracyDegrees1e5(1_234_567),
            )
            .unwrap(),
        },
        satellites_used: 10,
        pdop: Dop(230),
        utc: Some(UtcTime::new(2026, 11, 4, 13, 37, 59, -123_456).unwrap()),
    }
}

#[test]
fn a_good_solution_is_converted_stamped_and_numbered() {
    let frames = [good(), good()];
    let bytes = stream(&frames);
    let (mut gnss, _clock, _fail) = adapter(&bytes);
    assert_eq!(gnss.read(), Err(SensorError::NotStarted));
    assert_eq!(gnss.start(), Ok(Bringup::Ready));

    let sample = gnss.read().unwrap();
    assert_eq!(
        sample,
        Stamped {
            seq: 1,
            at: TimestampMicros(START_US),
            value: expected_solution(),
        }
    );
    assert_eq!(
        sample.value.fix.position(),
        expected_solution().fix.position()
    );

    // The next frame in the stream numbers its solution next.
    let next = gnss.read().unwrap();
    assert_eq!(next.seq, 2);
    assert_eq!(next.value, sample.value);
    // Drained: nothing more until the receiver sends another solution.
    assert_eq!(gnss.read(), Err(SensorError::NoNewSample));
}

#[test]
fn a_solution_without_a_valid_fix_carries_no_position() {
    // No fix at all; the time is valid, the fix is not.
    let frames = [nav_pvt(0, 0x00, 0x07, 464_045_956, -1_170_207_302)];
    let bytes = stream(&frames);
    let (mut gnss, _clock, _fail) = adapter(&bytes);
    gnss.start().unwrap();
    let sample = gnss.read().unwrap();
    assert_eq!(
        sample.value.fix,
        GnssFix::None {
            reported: ReportedFix::NoFix
        }
    );
    assert_eq!(sample.value.fix.position(), None);
    assert_eq!(sample.value.utc, expected_solution().utc);

    // A 3D fix the receiver does not mark valid (`gnssFixOK` clear).
    let frames = [nav_pvt(3, 0x00, 0x00, 464_045_956, -1_170_207_302)];
    let bytes = stream(&frames);
    let (mut gnss, _clock, _fail) = adapter(&bytes);
    gnss.start().unwrap();
    let sample = gnss.read().unwrap();
    assert_eq!(
        sample.value.fix,
        GnssFix::None {
            reported: ReportedFix::ThreeD
        }
    );
    // An invalid time is not carried either.
    assert_eq!(sample.value.utc, None);
}

#[test]
fn without_a_new_solution_a_read_is_never_the_previous_sample() {
    let frames = [good()];
    let bytes = stream(&frames);
    let (mut gnss, clock, _fail) = adapter(&bytes);
    gnss.start().unwrap();
    assert_eq!(gnss.read().unwrap().seq, 1);

    // The receiver stops sending: every read refuses a value, never the
    // previous one, and past the stall limit it is a timeout.
    assert_eq!(gnss.read(), Err(SensorError::NoNewSample));
    clock.advance(u64::from(UbxTiming::DEFAULT_STALL_LIMIT_US) + 1);
    assert_eq!(
        gnss.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
    assert_eq!(
        gnss.read(),
        Err(SensorError::Timeout(TimeoutStage::Measurement))
    );
}

#[test]
fn a_solution_outside_the_physical_range_is_refused() {
    // Latitude beyond 90°: the class refuses the position.
    let frames = [nav_pvt(3, 0x01, 0x07, 950_000_000, -1_170_207_302)];
    let bytes = stream(&frames);
    let (mut gnss, _clock, _fail) = adapter(&bytes);
    gnss.start().unwrap();
    assert_eq!(gnss.read(), Err(SensorError::OutOfRange));
    assert_eq!(gnss.read(), Err(SensorError::NoNewSample));
}

#[test]
fn frames_of_other_messages_are_skipped_until_a_solution() {
    // An ACK frame ahead of the NAV-PVT: the read drains past it.
    let mut bytes = std::vec::Vec::new();
    let mut buffer = [0_u8; 32];
    let len = encode(CLASS_ACK, ACK_ACK, &[0x06, 0x8A], &mut buffer).unwrap();
    bytes.extend_from_slice(&buffer[..len]);
    bytes.extend_from_slice(&stream(&[good()]));

    let (mut gnss, _clock, _fail) = adapter(&bytes);
    gnss.start().unwrap();
    let sample = gnss.read().unwrap();
    assert_eq!(sample.value, expected_solution());
}

#[test]
fn a_read_that_overruns_its_budget_is_a_timeout_and_its_result_discarded() {
    let bytes = stream(&[good()]);
    let (mut gnss, clock, _fail) = adapter(&bytes);
    gnss.start().unwrap();

    // The stream is there, but reading it outlasts the read budget: what
    // was taken is discarded.
    clock.set_per_byte(200);
    assert_eq!(gnss.read(), Err(SensorError::Timeout(TimeoutStage::Read)));

    // The next read resumes the stream where it stopped, so the discarded
    // solution arrives only as a new, later read's result.
    clock.set_per_byte(0);
    let sample = gnss.read().unwrap();
    assert_eq!(sample.seq, 1);
    assert_eq!(sample.value, expected_solution());
}

#[test]
fn a_port_error_is_a_bus_error_and_never_a_sample() {
    let bytes = stream(&[good()]);
    let (mut gnss, _clock, fail) = adapter(&bytes);
    gnss.start().unwrap();
    fail.set(true);
    assert_eq!(gnss.read(), Err(SensorError::Bus));
    fail.set(false);
    // The port recovered before the reader took a byte.
    assert_eq!(gnss.read().unwrap().seq, 1);
}
