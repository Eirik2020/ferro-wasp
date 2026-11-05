//! Telemetry frames a flight controller sends its receiver to relay to the
//! radio: battery, GPS, vario, barometric altitude, attitude and flight mode.
//!
//! Every frame starts with the flight controller's sync byte and carries its
//! fields big-endian, in the units the CRSF specification defines for them.
//! The encoders use integer arithmetic only. Each quantity enters as a typed
//! unit, and each field is bounded explicitly:
//!
//! - an angle wraps into one turn, because 370 degrees is the same heading
//!   as 10;
//! - every other number saturates at the nearest value its field carries,
//!   because a value wrapped by truncation reads as plausible and wrong;
//! - a value no field could carry honestly, a latitude past a pole or a
//!   flight-mode name too long for a frame, cannot be constructed at all.
//!
//! The encoders keep the caller's sign conventions: an attitude angle goes
//! out with the sign it came in with.
//!
//! Requirements: CRSF-006, CRSF-007
//! Safety-critical.

use crate::frame::{
    FRAME_TYPE_ATTITUDE, FRAME_TYPE_BARO_ALTITUDE, FRAME_TYPE_BATTERY_SENSOR,
    FRAME_TYPE_FLIGHT_MODE, FRAME_TYPE_GPS, FRAME_TYPE_VARIO, MAX_FRAME_LEN, MAX_PAYLOAD_LEN, seal,
};

/// Address, length, type and CRC around a payload.
const FRAMING_LEN: usize = 4;

/// A voltage in tenths of a volt.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Decivolts(pub u16);

/// A current in tenths of an amp.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Deciamps(pub u16);

/// A charge in milliamp-hours.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MilliampHours(pub u32);

/// A share in percent.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Percent(pub u8);

/// A speed in centimetres per second; positive is up for a vertical speed.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct CentimetresPerSecond(pub i32);

/// A length or altitude in decimetres.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Decimetres(pub i32);

/// A length or altitude in metres.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Metres(pub i32);

/// An angle in hundredths of a degree.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Centidegrees(pub i32);

/// An angle in units of 100 microradians, radians times 10 000.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RadiansE4(pub i32);

/// A latitude in degrees times 10^7, from the south pole to the north.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Latitude(i32);

impl Latitude {
    pub const MAX_DEGREES_E7: i32 = 900_000_000;

    /// `None` past either pole.
    pub const fn from_degrees_e7(value: i32) -> Option<Self> {
        if value >= -Self::MAX_DEGREES_E7 && value <= Self::MAX_DEGREES_E7 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn degrees_e7(self) -> i32 {
        self.0
    }
}

/// A longitude in degrees times 10^7, from -180 to 180 degrees.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Longitude(i32);

impl Longitude {
    pub const MAX_DEGREES_E7: i32 = 1_800_000_000;

    /// `None` past the antimeridian on either side.
    pub const fn from_degrees_e7(value: i32) -> Option<Self> {
        if value >= -Self::MAX_DEGREES_E7 && value <= Self::MAX_DEGREES_E7 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn degrees_e7(self) -> i32 {
        self.0
    }
}

fn saturate_u16(value: i64) -> u16 {
    value.clamp(0, i64::from(u16::MAX)) as u16
}

fn saturate_i16(value: i64) -> i16 {
    value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

// --- Battery (0x08) ---

const BATTERY_PAYLOAD_LEN: usize = 8;
/// Address, length, type, payload and CRC.
pub const BATTERY_FRAME_LEN: usize = BATTERY_PAYLOAD_LEN + FRAMING_LEN;
/// The largest voltage and current sent: some readers take these two fields
/// as signed, so nothing above `i16::MAX` goes out, and every reader agrees.
pub const BATTERY_FIELD_MAX: u16 = i16::MAX as u16;
/// CRSF carries the charge drawn in 24 bits.
pub const BATTERY_USED_MAX: MilliampHours = MilliampHours(0x00FF_FFFF);

/// A battery sensor report.
///
/// A value the flight controller does not measure is sent as zero, which
/// radios show as zero rather than as missing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatteryTelemetry {
    /// Pack voltage; saturates at [`BATTERY_FIELD_MAX`] tenths of a volt.
    pub voltage: Decivolts,
    /// Pack current; saturates at [`BATTERY_FIELD_MAX`] tenths of an amp.
    pub current: Deciamps,
    /// Charge drawn so far; saturates at [`BATTERY_USED_MAX`].
    pub used: MilliampHours,
    /// Estimated charge remaining; saturates at 100.
    pub remaining: Percent,
}

/// Encode one battery sensor frame.
pub fn encode_battery(telemetry: BatteryTelemetry) -> [u8; BATTERY_FRAME_LEN] {
    let voltage = telemetry.voltage.0.min(BATTERY_FIELD_MAX).to_be_bytes();
    let current = telemetry.current.0.min(BATTERY_FIELD_MAX).to_be_bytes();
    let used = telemetry.used.0.min(BATTERY_USED_MAX.0).to_be_bytes();
    let mut frame = [0_u8; BATTERY_FRAME_LEN];
    frame[3..5].copy_from_slice(&voltage);
    frame[5..7].copy_from_slice(&current);
    frame[7..10].copy_from_slice(&used[1..]);
    frame[10] = telemetry.remaining.0.min(100);
    seal(&mut frame, FRAME_TYPE_BATTERY_SENSOR);
    frame
}

// --- GPS (0x02) ---

const GPS_PAYLOAD_LEN: usize = 15;
/// Address, length, type, payload and CRC.
pub const GPS_FRAME_LEN: usize = GPS_PAYLOAD_LEN + FRAMING_LEN;
/// The GPS altitude field is metres plus this offset, as an unsigned 16-bit
/// value, so it carries -1000 to 64 535 m.
pub const GPS_ALTITUDE_OFFSET: Metres = Metres(1000);

/// A GPS fix.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpsTelemetry {
    pub latitude: Latitude,
    pub longitude: Longitude,
    /// Speed over ground, sent in tenths of a km/h, rounded to nearest; a
    /// negative speed saturates at zero and a fast one at 6553.5 km/h.
    pub ground_speed: CentimetresPerSecond,
    /// Course over ground; wraps into 0 to 359.99 degrees.
    pub heading: Centidegrees,
    /// Altitude; saturates at -1000 m and 64 535 m.
    pub altitude: Metres,
    /// Satellites in the fix.
    pub satellites: u8,
}

/// Tenths of a km/h from centimetres per second: one cm/s is 0.36 tenths.
fn ground_speed_field(speed: CentimetresPerSecond) -> u16 {
    let speed = i64::from(speed.0.max(0));
    saturate_u16((speed * 36 + 50) / 100)
}

fn heading_field(heading: Centidegrees) -> u16 {
    heading.0.rem_euclid(36_000) as u16
}

fn gps_altitude_field(altitude: Metres) -> u16 {
    saturate_u16(i64::from(altitude.0) + i64::from(GPS_ALTITUDE_OFFSET.0))
}

/// Encode one GPS frame.
pub fn encode_gps(telemetry: GpsTelemetry) -> [u8; GPS_FRAME_LEN] {
    let mut frame = [0_u8; GPS_FRAME_LEN];
    frame[3..7].copy_from_slice(&telemetry.latitude.0.to_be_bytes());
    frame[7..11].copy_from_slice(&telemetry.longitude.0.to_be_bytes());
    frame[11..13].copy_from_slice(&ground_speed_field(telemetry.ground_speed).to_be_bytes());
    frame[13..15].copy_from_slice(&heading_field(telemetry.heading).to_be_bytes());
    frame[15..17].copy_from_slice(&gps_altitude_field(telemetry.altitude).to_be_bytes());
    frame[17] = telemetry.satellites;
    seal(&mut frame, FRAME_TYPE_GPS);
    frame
}

// --- Vario (0x07) ---

const VARIO_PAYLOAD_LEN: usize = 2;
/// Address, length, type, payload and CRC.
pub const VARIO_FRAME_LEN: usize = VARIO_PAYLOAD_LEN + FRAMING_LEN;

/// Encode one vario frame: the vertical speed in cm/s, saturating at what a
/// signed 16-bit field carries, about ±327 m/s.
pub fn encode_vario(vertical_speed: CentimetresPerSecond) -> [u8; VARIO_FRAME_LEN] {
    let mut frame = [0_u8; VARIO_FRAME_LEN];
    frame[3..5].copy_from_slice(&saturate_i16(i64::from(vertical_speed.0)).to_be_bytes());
    seal(&mut frame, FRAME_TYPE_VARIO);
    frame
}

// --- Barometric altitude (0x09) ---

const BARO_ALTITUDE_PAYLOAD_LEN: usize = 3;
/// Address, length, type, payload and CRC.
pub const BARO_ALTITUDE_FRAME_LEN: usize = BARO_ALTITUDE_PAYLOAD_LEN + FRAMING_LEN;

/// The packed altitude's decimetre form carries the altitude plus this.
const ALTITUDE_OFFSET_DM: i32 = 10_000;
/// Below this the decimetre form is used; at and above it, whole metres.
const ALTITUDE_METRE_FORM_DM: i32 = 0x8000 - ALTITUDE_OFFSET_DM;
/// The highest altitude the metre form carries, rounding to 32 766 m.
const ALTITUDE_MAX_DM: i32 = 0x7FFE * 10 - 5;
/// The packed altitude's flag for its metre form.
const ALTITUDE_METRE_FLAG: u16 = 0x8000;

/// The smallest vertical speed, in cm/s, at which each packed step from 1 to
/// 128 begins: step `n` covers speeds from `100 * (exp(0.026 * n) - 1)`, the
/// specification's logarithmic packing with its constants of 100 and 0.026.
/// A climb uses steps up to 127 and a descent up to 128, what a signed byte
/// carries.
const VERTICAL_SPEED_STEPS: [u16; 128] = [
    3, 6, 9, 11, 14, 17, 20, 24, 27, 30, 34, 37, 41, 44, 48, 52, 56, 60, 64, 69, 73, 78, 82, 87,
    92, 97, 102, 108, 113, 119, 124, 130, 136, 143, 149, 155, 162, 169, 176, 183, 191, 199, 206,
    214, 223, 231, 240, 249, 258, 267, 277, 287, 297, 308, 318, 329, 341, 352, 364, 376, 389, 402,
    415, 429, 442, 457, 471, 486, 502, 518, 534, 551, 568, 585, 603, 622, 641, 660, 680, 701, 722,
    744, 766, 789, 812, 836, 861, 886, 912, 939, 966, 994, 1023, 1052, 1083, 1114, 1146, 1179,
    1212, 1247, 1282, 1319, 1356, 1394, 1434, 1474, 1516, 1558, 1602, 1647, 1693, 1740, 1788, 1838,
    1889, 1941, 1995, 2050, 2107, 2165, 2225, 2286, 2349, 2413, 2480, 2547, 2617, 2689,
];

/// A barometric altitude report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BaroAltitudeTelemetry {
    /// Altitude above the start point; see [`pack_baro_altitude`].
    pub altitude: Decimetres,
    /// Vertical speed; see [`pack_vertical_speed`].
    pub vertical_speed: CentimetresPerSecond,
}

/// Pack an altitude as the specification's 16-bit field: decimetres plus
/// 10 000 below 2276.8 m, whole metres with the top bit set from there,
/// rounded to nearest. Below -1000 m it saturates at 0, and above 32 765.5 m
/// at `0xFFFE`.
pub fn pack_baro_altitude(altitude: Decimetres) -> u16 {
    let dm = altitude.0;
    if dm < -ALTITUDE_OFFSET_DM {
        0
    } else if dm > ALTITUDE_MAX_DM {
        ALTITUDE_METRE_FLAG | 0x7FFE
    } else if dm < ALTITUDE_METRE_FORM_DM {
        (dm + ALTITUDE_OFFSET_DM) as u16
    } else {
        ALTITUDE_METRE_FLAG | ((dm + 5) / 10) as u16
    }
}

/// Pack a vertical speed as the specification's signed 8-bit logarithmic
/// field: about 3 cm/s per step near zero, truncated towards zero. A climb
/// saturates at 127 from 26.17 m/s, and a descent at -128 from 26.89 m/s.
pub fn pack_vertical_speed(vertical_speed: CentimetresPerSecond) -> i8 {
    let magnitude = vertical_speed.0.unsigned_abs();
    let steps = VERTICAL_SPEED_STEPS
        .iter()
        .take_while(|step| u32::from(**step) <= magnitude)
        .count() as i16;
    if vertical_speed.0 < 0 {
        (-steps).max(i16::from(i8::MIN)) as i8
    } else {
        steps.min(i16::from(i8::MAX)) as i8
    }
}

/// Encode one barometric altitude frame.
pub fn encode_baro_altitude(telemetry: BaroAltitudeTelemetry) -> [u8; BARO_ALTITUDE_FRAME_LEN] {
    let mut frame = [0_u8; BARO_ALTITUDE_FRAME_LEN];
    frame[3..5].copy_from_slice(&pack_baro_altitude(telemetry.altitude).to_be_bytes());
    frame[5] = pack_vertical_speed(telemetry.vertical_speed) as u8;
    seal(&mut frame, FRAME_TYPE_BARO_ALTITUDE);
    frame
}

// --- Attitude (0x1E) ---

const ATTITUDE_PAYLOAD_LEN: usize = 6;
/// Address, length, type, payload and CRC.
pub const ATTITUDE_FRAME_LEN: usize = ATTITUDE_PAYLOAD_LEN + FRAMING_LEN;
/// Half a turn, π, in units of 100 µrad.
const HALF_TURN_E4: i64 = 31_416;

/// An attitude report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttitudeTelemetry {
    pub pitch: RadiansE4,
    pub roll: RadiansE4,
    pub yaw: RadiansE4,
}

/// Wrap an angle into -π to just under π, which a signed 16-bit field in
/// units of 100 µrad carries. One turn is taken as 62 832 units, 15 µrad
/// more than 2π, which no radio can show.
pub fn wrap_angle(angle: RadiansE4) -> i16 {
    ((i64::from(angle.0) + HALF_TURN_E4).rem_euclid(2 * HALF_TURN_E4) - HALF_TURN_E4) as i16
}

/// Encode one attitude frame: pitch, roll and yaw, each wrapped by
/// [`wrap_angle`].
pub fn encode_attitude(telemetry: AttitudeTelemetry) -> [u8; ATTITUDE_FRAME_LEN] {
    let mut frame = [0_u8; ATTITUDE_FRAME_LEN];
    frame[3..5].copy_from_slice(&wrap_angle(telemetry.pitch).to_be_bytes());
    frame[5..7].copy_from_slice(&wrap_angle(telemetry.roll).to_be_bytes());
    frame[7..9].copy_from_slice(&wrap_angle(telemetry.yaw).to_be_bytes());
    seal(&mut frame, FRAME_TYPE_ATTITUDE);
    frame
}

// --- Flight mode (0x21) ---

/// The longest flight-mode name: a frame's payload less its terminator.
pub const MAX_FLIGHT_MODE_LEN: usize = MAX_PAYLOAD_LEN - 1;

/// Why a flight-mode name was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlightModeError {
    /// Longer than [`MAX_FLIGHT_MODE_LEN`] bytes.
    TooLong,
    /// A byte outside printable ASCII, a terminator included, which a
    /// radio would cut the name at or show as garbage.
    NotPrintable,
}

/// A flight-mode name: printable ASCII, at most [`MAX_FLIGHT_MODE_LEN`]
/// bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlightMode {
    name: [u8; MAX_FLIGHT_MODE_LEN],
    len: u8,
}

impl FlightMode {
    pub fn new(name: &str) -> Result<Self, FlightModeError> {
        let bytes = name.as_bytes();
        if bytes.len() > MAX_FLIGHT_MODE_LEN {
            return Err(FlightModeError::TooLong);
        }
        if !bytes.iter().all(|byte| (0x20..=0x7E).contains(byte)) {
            return Err(FlightModeError::NotPrintable);
        }
        let mut stored = [0_u8; MAX_FLIGHT_MODE_LEN];
        stored[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            name: stored,
            len: bytes.len() as u8,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.name.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

/// A frame whose length depends on its content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodedFrame {
    bytes: [u8; MAX_FRAME_LEN],
    len: u8,
}

impl EncodedFrame {
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

impl AsRef<[u8]> for EncodedFrame {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// Encode one flight-mode frame: the name and its null terminator.
pub fn encode_flight_mode(mode: &FlightMode) -> EncodedFrame {
    let name = mode.as_bytes();
    let len = name.len() + 1 + FRAMING_LEN;
    let mut bytes = [0_u8; MAX_FRAME_LEN];
    bytes[3..3 + name.len()].copy_from_slice(name);
    seal(&mut bytes[..len], FRAME_TYPE_FLIGHT_MODE);
    EncodedFrame {
        bytes,
        len: len as u8,
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::frame::{ADDRESS_FLIGHT_CONTROLLER, is_well_formed};
    use crate::parser::{Frame, StreamingParser};

    fn parse_one(bytes: &[u8]) -> Option<Result<Frame, crate::parser::DecodeError>> {
        let mut parser = StreamingParser::new();
        let mut decoded = None;
        for byte in bytes {
            if let Some(result) = parser.push_byte(*byte) {
                decoded = Some(result);
            }
        }
        decoded
    }

    #[test]
    fn a_battery_frame_is_big_endian_and_round_trips_through_the_parser() {
        let frame = encode_battery(BatteryTelemetry {
            voltage: Decivolts(168),
            current: Deciamps(0x0102),
            used: MilliampHours(0x0003_0405),
            remaining: Percent(87),
        });
        assert_eq!(
            frame[..BATTERY_FRAME_LEN - 1],
            [0xC8, 10, 0x08, 0, 168, 0x01, 0x02, 0x03, 0x04, 0x05, 87]
        );
        assert_eq!(
            parse_one(&frame),
            Some(Ok(Frame::Other {
                frame_type: FRAME_TYPE_BATTERY_SENSOR
            }))
        );
    }

    #[test]
    fn battery_fields_saturate_at_what_crsf_carries() {
        let frame = encode_battery(BatteryTelemetry {
            voltage: Decivolts(u16::MAX),
            current: Deciamps(0x8000),
            used: MilliampHours(u32::MAX),
            remaining: Percent(250),
        });
        assert_eq!(
            frame[3..11],
            [0x7F, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 100]
        );
        assert!(is_well_formed(&frame));
    }

    #[test]
    fn a_gps_frame_carries_each_field_big_endian_in_its_unit() {
        let frame = encode_gps(GpsTelemetry {
            latitude: Latitude::from_degrees_e7(-338_688_000).unwrap(),
            longitude: Longitude::from_degrees_e7(1_512_093_000).unwrap(),
            // 10 m/s is 36 km/h.
            ground_speed: CentimetresPerSecond(1_000),
            heading: Centidegrees(9_000),
            altitude: Metres(120),
            satellites: 14,
        });
        let mut expected = [0_u8; GPS_FRAME_LEN - 1];
        expected[..3].copy_from_slice(&[ADDRESS_FLIGHT_CONTROLLER, 17, FRAME_TYPE_GPS]);
        expected[3..7].copy_from_slice(&(-338_688_000_i32).to_be_bytes());
        expected[7..11].copy_from_slice(&1_512_093_000_i32.to_be_bytes());
        expected[11..13].copy_from_slice(&360_u16.to_be_bytes());
        expected[13..15].copy_from_slice(&9_000_u16.to_be_bytes());
        expected[15..17].copy_from_slice(&1_120_u16.to_be_bytes());
        expected[17] = 14;
        assert_eq!(frame[..GPS_FRAME_LEN - 1], expected);
        assert_eq!(
            parse_one(&frame),
            Some(Ok(Frame::Other {
                frame_type: FRAME_TYPE_GPS
            }))
        );
    }

    #[test]
    fn gps_fields_saturate_or_wrap_and_a_position_past_a_pole_is_refused() {
        assert_eq!(ground_speed_field(CentimetresPerSecond(-5)), 0);
        assert_eq!(ground_speed_field(CentimetresPerSecond(1)), 0);
        assert_eq!(ground_speed_field(CentimetresPerSecond(2)), 1);
        assert_eq!(ground_speed_field(CentimetresPerSecond(i32::MAX)), u16::MAX);
        assert_eq!(heading_field(Centidegrees(36_000)), 0);
        assert_eq!(heading_field(Centidegrees(-100)), 35_900);
        assert_eq!(
            heading_field(Centidegrees(i32::MIN)),
            (i32::MIN.rem_euclid(36_000)) as u16
        );
        assert_eq!(gps_altitude_field(Metres(-5_000)), 0);
        assert_eq!(gps_altitude_field(Metres(-1_000)), 0);
        assert_eq!(gps_altitude_field(Metres(64_535)), u16::MAX);
        assert_eq!(gps_altitude_field(Metres(i32::MAX)), u16::MAX);

        assert!(Latitude::from_degrees_e7(900_000_001).is_none());
        assert!(Latitude::from_degrees_e7(-900_000_001).is_none());
        assert!(Latitude::from_degrees_e7(i32::MIN).is_none());
        assert!(Longitude::from_degrees_e7(1_800_000_001).is_none());
        assert!(Longitude::from_degrees_e7(i32::MAX).is_none());
        assert_eq!(
            Longitude::from_degrees_e7(-1_800_000_000).map(Longitude::degrees_e7),
            Some(-1_800_000_000)
        );
    }

    #[test]
    fn a_vario_frame_carries_signed_centimetres_per_second() {
        let frame = encode_vario(CentimetresPerSecond(-250));
        assert_eq!(frame[..5], [0xC8, 4, FRAME_TYPE_VARIO, 0xFF, 0x06]);
        assert!(is_well_formed(&frame));
        assert_eq!(
            encode_vario(CentimetresPerSecond(i32::MAX))[3..5],
            [0x7F, 0xFF]
        );
        assert_eq!(
            encode_vario(CentimetresPerSecond(i32::MIN))[3..5],
            [0x80, 0x00]
        );
    }

    #[test]
    fn baro_altitude_packs_decimetres_then_metres_and_saturates() {
        assert_eq!(pack_baro_altitude(Decimetres(i32::MIN)), 0);
        assert_eq!(pack_baro_altitude(Decimetres(-10_001)), 0);
        assert_eq!(pack_baro_altitude(Decimetres(-10_000)), 0);
        assert_eq!(pack_baro_altitude(Decimetres(0)), 10_000);
        assert_eq!(pack_baro_altitude(Decimetres(1_234)), 11_234);
        assert_eq!(pack_baro_altitude(Decimetres(22_767)), 0x7FFF);
        // 2276.8 m is the first altitude in whole metres, rounded.
        assert_eq!(pack_baro_altitude(Decimetres(22_768)), 0x8000 | 2_277);
        assert_eq!(pack_baro_altitude(Decimetres(22_774)), 0x8000 | 2_277);
        assert_eq!(pack_baro_altitude(Decimetres(22_775)), 0x8000 | 2_278);
        assert_eq!(pack_baro_altitude(Decimetres(327_655)), 0xFFFE);
        assert_eq!(pack_baro_altitude(Decimetres(327_656)), 0xFFFE);
        assert_eq!(pack_baro_altitude(Decimetres(i32::MAX)), 0xFFFE);

        let frame = encode_baro_altitude(BaroAltitudeTelemetry {
            altitude: Decimetres(1_234),
            vertical_speed: CentimetresPerSecond(-100),
        });
        assert_eq!(
            frame[..6],
            [
                0xC8,
                5,
                FRAME_TYPE_BARO_ALTITUDE,
                0x2B,
                0xE2,
                (-26_i8) as u8
            ]
        );
        assert!(is_well_formed(&frame));
    }

    #[test]
    fn vertical_speed_packing_matches_the_specification_formula_everywhere() {
        let formula = |speed: i32| -> i8 {
            let magnitude = f64::from(speed.unsigned_abs());
            let steps = ((magnitude / 100.0 + 1.0).ln() / 0.026).floor();
            if speed < 0 {
                -(steps.min(128.0)) as i8
            } else {
                steps.min(127.0) as i8
            }
        };
        for speed in -40_000..=40_000 {
            assert_eq!(
                pack_vertical_speed(CentimetresPerSecond(speed)),
                formula(speed),
                "{speed} cm/s"
            );
        }
        assert_eq!(pack_vertical_speed(CentimetresPerSecond(i32::MAX)), 127);
        assert_eq!(pack_vertical_speed(CentimetresPerSecond(i32::MIN)), -128);
        assert_eq!(pack_vertical_speed(CentimetresPerSecond(-2_688)), -127);
        assert_eq!(pack_vertical_speed(CentimetresPerSecond(-2_689)), -128);
        assert_eq!(pack_vertical_speed(CentimetresPerSecond(2_689)), 127);
    }

    #[test]
    fn attitude_angles_wrap_into_one_turn() {
        assert_eq!(wrap_angle(RadiansE4(0)), 0);
        assert_eq!(wrap_angle(RadiansE4(31_415)), 31_415);
        assert_eq!(wrap_angle(RadiansE4(31_416)), -31_416);
        assert_eq!(wrap_angle(RadiansE4(-31_416)), -31_416);
        assert_eq!(wrap_angle(RadiansE4(-31_417)), 31_415);
        // 359 degrees of yaw is a degree short of north.
        assert_eq!(wrap_angle(RadiansE4(62_657)), -175);
        assert_eq!(
            wrap_angle(RadiansE4(i32::MAX)),
            ((i64::from(i32::MAX) + 31_416).rem_euclid(62_832) - 31_416) as i16
        );
        assert_eq!(
            wrap_angle(RadiansE4(i32::MIN)),
            ((i64::from(i32::MIN) + 31_416).rem_euclid(62_832) - 31_416) as i16
        );

        let frame = encode_attitude(AttitudeTelemetry {
            pitch: RadiansE4(-1_000),
            roll: RadiansE4(2_000),
            yaw: RadiansE4(62_832 + 3_000),
        });
        assert_eq!(
            frame[..9],
            [
                0xC8,
                8,
                FRAME_TYPE_ATTITUDE,
                0xFC,
                0x18,
                0x07,
                0xD0,
                0x0B,
                0xB8
            ]
        );
        assert!(is_well_formed(&frame));
    }

    #[test]
    fn a_flight_mode_frame_is_its_name_and_a_terminator() {
        let frame = encode_flight_mode(&FlightMode::new("ACRO").unwrap());
        assert_eq!(
            frame.as_bytes()[..8],
            [0xC8, 7, FRAME_TYPE_FLIGHT_MODE, b'A', b'C', b'R', b'O', 0]
        );
        assert_eq!(frame.as_bytes().len(), 9);
        assert!(is_well_formed(frame.as_bytes()));

        let empty = encode_flight_mode(&FlightMode::new("").unwrap());
        assert_eq!(empty.as_bytes().len(), 5);
        assert!(is_well_formed(empty.as_bytes()));

        let longest = "M".repeat(MAX_FLIGHT_MODE_LEN);
        let frame = encode_flight_mode(&FlightMode::new(&longest).unwrap());
        assert_eq!(frame.as_bytes().len(), MAX_FRAME_LEN);
        assert!(is_well_formed(frame.as_bytes()));
        assert_eq!(
            parse_one(frame.as_bytes()),
            Some(Ok(Frame::Other {
                frame_type: FRAME_TYPE_FLIGHT_MODE
            }))
        );
    }

    #[test]
    fn a_flight_mode_name_a_frame_cannot_carry_is_refused() {
        let too_long = "M".repeat(MAX_FLIGHT_MODE_LEN + 1);
        assert_eq!(FlightMode::new(&too_long), Err(FlightModeError::TooLong));
        assert_eq!(
            FlightMode::new("AC\0RO"),
            Err(FlightModeError::NotPrintable)
        );
        assert_eq!(
            FlightMode::new("ANGLE\n"),
            Err(FlightModeError::NotPrintable)
        );
        assert_eq!(FlightMode::new("HÖHE"), Err(FlightModeError::NotPrintable));
        assert_eq!(
            FlightMode::new("!FS!").map(|mode| mode.as_bytes().len()),
            Ok(4)
        );
    }
}
