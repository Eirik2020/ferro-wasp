//! The GNSS class: one navigation solution per epoch, shaped for a source
//! that reports position, velocity and time together, such as u-blox
//! NAV-PVT, in integer units.
//!
//! A position and velocity exist only inside a valid fix, so "no fix" has no
//! coordinates to misread, and a UTC time exists only when its source marks
//! it valid. Every value is range-checked when it is built.
//!
//! Requirements: SENSOR-006, SENSOR-007, SENSOR-008
//! Safety-critical.

use crate::class::{Sensor, SensorError};

/// An angle in units of 1e-7 degree: latitude and longitude.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Degrees1e7(i32);

impl Degrees1e7 {
    pub const fn raw(self) -> i32 {
        self.0
    }
}

/// An angle in units of 1e-5 degree: heading of motion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Degrees1e5(i32);

impl Degrees1e5 {
    pub const fn raw(self) -> i32 {
        self.0
    }
}

/// A signed length in millimetres: heights.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Millimetres(pub i32);

/// A signed speed in millimetres per second: velocity components.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MillimetresPerSecond(pub i32);

/// An accuracy estimate in millimetres, one standard deviation as the
/// source reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AccuracyMillimetres(pub u32);

/// A speed accuracy estimate in millimetres per second.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AccuracyMillimetresPerSecond(pub u32);

/// A heading accuracy estimate in units of 1e-5 degree.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AccuracyDegrees1e5(pub u32);

/// A dilution of precision, in hundredths.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Dop(pub u16);

const LATITUDE_LIMIT: i32 = 900_000_000;
const LONGITUDE_LIMIT: i32 = 1_800_000_000;
const FULL_TURN_1E5: i32 = 36_000_000;

/// Where the receiver is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    latitude: Degrees1e7,
    longitude: Degrees1e7,
    height_ellipsoid: Millimetres,
    height_msl: Millimetres,
    horizontal_accuracy: AccuracyMillimetres,
    vertical_accuracy: AccuracyMillimetres,
}

impl Position {
    /// Latitude within ±90° and longitude within ±180°, in 1e-7 degree, or
    /// `SensorError::OutOfRange`.
    pub const fn new(
        latitude_1e7: i32,
        longitude_1e7: i32,
        height_ellipsoid: Millimetres,
        height_msl: Millimetres,
        horizontal_accuracy: AccuracyMillimetres,
        vertical_accuracy: AccuracyMillimetres,
    ) -> Result<Self, SensorError> {
        if latitude_1e7 < -LATITUDE_LIMIT
            || latitude_1e7 > LATITUDE_LIMIT
            || longitude_1e7 < -LONGITUDE_LIMIT
            || longitude_1e7 > LONGITUDE_LIMIT
        {
            return Err(SensorError::OutOfRange);
        }
        Ok(Self {
            latitude: Degrees1e7(latitude_1e7),
            longitude: Degrees1e7(longitude_1e7),
            height_ellipsoid,
            height_msl,
            horizontal_accuracy,
            vertical_accuracy,
        })
    }

    pub const fn latitude(&self) -> Degrees1e7 {
        self.latitude
    }
    pub const fn longitude(&self) -> Degrees1e7 {
        self.longitude
    }
    pub const fn height_ellipsoid(&self) -> Millimetres {
        self.height_ellipsoid
    }
    pub const fn height_msl(&self) -> Millimetres {
        self.height_msl
    }
    pub const fn horizontal_accuracy(&self) -> AccuracyMillimetres {
        self.horizontal_accuracy
    }
    pub const fn vertical_accuracy(&self) -> AccuracyMillimetres {
        self.vertical_accuracy
    }
}

/// How the receiver moves, north-east-down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Velocity {
    north: MillimetresPerSecond,
    east: MillimetresPerSecond,
    down: MillimetresPerSecond,
    ground_speed: MillimetresPerSecond,
    heading_of_motion: Degrees1e5,
    speed_accuracy: AccuracyMillimetresPerSecond,
    heading_accuracy: AccuracyDegrees1e5,
}

impl Velocity {
    /// Ground speed not negative and heading of motion in [0°, 360°), in
    /// 1e-5 degree, or `SensorError::OutOfRange`.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        north: MillimetresPerSecond,
        east: MillimetresPerSecond,
        down: MillimetresPerSecond,
        ground_speed: MillimetresPerSecond,
        heading_of_motion_1e5: i32,
        speed_accuracy: AccuracyMillimetresPerSecond,
        heading_accuracy: AccuracyDegrees1e5,
    ) -> Result<Self, SensorError> {
        if ground_speed.0 < 0 || heading_of_motion_1e5 < 0 || heading_of_motion_1e5 >= FULL_TURN_1E5
        {
            return Err(SensorError::OutOfRange);
        }
        Ok(Self {
            north,
            east,
            down,
            ground_speed,
            heading_of_motion: Degrees1e5(heading_of_motion_1e5),
            speed_accuracy,
            heading_accuracy,
        })
    }

    pub const fn north(&self) -> MillimetresPerSecond {
        self.north
    }
    pub const fn east(&self) -> MillimetresPerSecond {
        self.east
    }
    pub const fn down(&self) -> MillimetresPerSecond {
        self.down
    }
    pub const fn ground_speed(&self) -> MillimetresPerSecond {
        self.ground_speed
    }
    pub const fn heading_of_motion(&self) -> Degrees1e5 {
        self.heading_of_motion
    }
    pub const fn speed_accuracy(&self) -> AccuracyMillimetresPerSecond {
        self.speed_accuracy
    }
    pub const fn heading_accuracy(&self) -> AccuracyDegrees1e5 {
        self.heading_accuracy
    }
}

/// The fix type a source reports, whatever its validity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportedFix {
    NoFix,
    DeadReckoningOnly,
    TwoD,
    ThreeD,
    GnssDeadReckoning,
    TimeOnly,
}

/// The kinds of fix that carry a position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixKind {
    TwoD,
    ThreeD,
    GnssDeadReckoning,
}

/// A solution's fix: a position and velocity only inside a valid fix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GnssFix {
    /// No usable position: what the source reported is kept for display and
    /// diagnosis.
    None { reported: ReportedFix },
    Valid {
        kind: FixKind,
        position: Position,
        velocity: Velocity,
    },
}

impl GnssFix {
    /// A valid fix only for a two-dimensional, three-dimensional or
    /// combined GNSS and dead-reckoning fix that the source marks valid
    /// (NAV-PVT's `gnssFixOK`); anything else has no position. `build` makes
    /// the position and velocity, and runs only for a valid fix, so an
    /// invalid fix's fields are never read; its error is returned as is.
    pub fn from_report<F>(
        reported: ReportedFix,
        fix_valid: bool,
        build: F,
    ) -> Result<Self, SensorError>
    where
        F: FnOnce() -> Result<(Position, Velocity), SensorError>,
    {
        let kind = match reported {
            ReportedFix::TwoD => FixKind::TwoD,
            ReportedFix::ThreeD => FixKind::ThreeD,
            ReportedFix::GnssDeadReckoning => FixKind::GnssDeadReckoning,
            ReportedFix::NoFix | ReportedFix::DeadReckoningOnly | ReportedFix::TimeOnly => {
                return Ok(Self::None { reported });
            }
        };
        if !fix_valid {
            return Ok(Self::None { reported });
        }
        let (position, velocity) = build()?;
        Ok(Self::Valid {
            kind,
            position,
            velocity,
        })
    }

    pub const fn position(&self) -> Option<&Position> {
        match self {
            Self::Valid { position, .. } => Some(position),
            Self::None { .. } => None,
        }
    }

    pub const fn velocity(&self) -> Option<&Velocity> {
        match self {
            Self::Valid { velocity, .. } => Some(velocity),
            Self::None { .. } => None,
        }
    }
}

/// A UTC date and time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UtcTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    nanosecond: i32,
}

/// What a source says of its UTC time (NAV-PVT's `validDate`, `validTime`
/// and `fullyResolved`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UtcValidity {
    pub date: bool,
    pub time: bool,
    pub fully_resolved: bool,
}

impl UtcTime {
    /// A calendar-plausible time, or `SensorError::OutOfRange`: month 1-12,
    /// day 1-31, hour below 24, minute below 60, second up to 60 for a leap
    /// second, and a signed fraction under one second, as NAV-PVT's `nano`.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanosecond: i32,
    ) -> Result<Self, SensorError> {
        if month < 1
            || month > 12
            || day < 1
            || day > 31
            || hour > 23
            || minute > 59
            || second > 60
            || nanosecond <= -1_000_000_000
            || nanosecond >= 1_000_000_000
        {
            return Err(SensorError::OutOfRange);
        }
        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond,
        })
    }

    /// The time only when the source marks date and time valid and fully
    /// resolved; otherwise none, whatever the fields hold. A valid time
    /// outside the calendar is `SensorError::OutOfRange`.
    #[allow(clippy::too_many_arguments)]
    pub const fn from_report(
        validity: UtcValidity,
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanosecond: i32,
    ) -> Result<Option<Self>, SensorError> {
        if !(validity.date && validity.time && validity.fully_resolved) {
            return Ok(None);
        }
        match Self::new(year, month, day, hour, minute, second, nanosecond) {
            Ok(time) => Ok(Some(time)),
            Err(error) => Err(error),
        }
    }

    pub const fn year(&self) -> u16 {
        self.year
    }
    pub const fn month(&self) -> u8 {
        self.month
    }
    pub const fn day(&self) -> u8 {
        self.day
    }
    pub const fn hour(&self) -> u8 {
        self.hour
    }
    pub const fn minute(&self) -> u8 {
        self.minute
    }
    pub const fn second(&self) -> u8 {
        self.second
    }
    pub const fn nanosecond(&self) -> i32 {
        self.nanosecond
    }
}

/// One navigation solution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GnssSolution {
    pub fix: GnssFix,
    /// Satellites used in the solution (NAV-PVT's `numSV`).
    pub satellites_used: u8,
    /// Position dilution of precision.
    pub pdop: Dop,
    /// UTC time, only when the source marks it valid.
    pub utc: Option<UtcTime>,
}

/// A GNSS receiver: any polled sensor whose sample is a [`GnssSolution`].
pub trait Gnss: Sensor<Sample = GnssSolution> {}

impl<T: Sensor<Sample = GnssSolution>> Gnss for T {}

#[cfg(test)]
mod tests {
    use super::*;

    fn position() -> Position {
        Position::new(
            599_133_000,
            107_491_000,
            Millimetres(45_000),
            Millimetres(12_000),
            AccuracyMillimetres(1_500),
            AccuracyMillimetres(2_500),
        )
        .unwrap()
    }

    fn velocity() -> Velocity {
        Velocity::new(
            MillimetresPerSecond(1_000),
            MillimetresPerSecond(-500),
            MillimetresPerSecond(20),
            MillimetresPerSecond(1_118),
            15_343_000,
            AccuracyMillimetresPerSecond(300),
            AccuracyDegrees1e5(500_000),
        )
        .unwrap()
    }

    fn both() -> Result<(Position, Velocity), SensorError> {
        Ok((position(), velocity()))
    }

    /// What an invalid fix's garbage fields would build: never called.
    fn never() -> Result<(Position, Velocity), SensorError> {
        Err(SensorError::OutOfRange)
    }

    #[test]
    fn only_a_valid_position_fix_exposes_a_position() {
        for (reported, kind) in [
            (ReportedFix::TwoD, FixKind::TwoD),
            (ReportedFix::ThreeD, FixKind::ThreeD),
            (ReportedFix::GnssDeadReckoning, FixKind::GnssDeadReckoning),
        ] {
            let fix = GnssFix::from_report(reported, true, both).unwrap();
            assert_eq!(
                fix,
                GnssFix::Valid {
                    kind,
                    position: position(),
                    velocity: velocity()
                }
            );
            assert_eq!(fix.position(), Some(&position()));
            assert_eq!(fix.velocity(), Some(&velocity()));

            let unmarked = GnssFix::from_report(reported, false, never).unwrap();
            assert_eq!(unmarked, GnssFix::None { reported });
            assert_eq!(unmarked.position(), None);
            assert_eq!(unmarked.velocity(), None);
        }
        for reported in [
            ReportedFix::NoFix,
            ReportedFix::DeadReckoningOnly,
            ReportedFix::TimeOnly,
        ] {
            for valid in [false, true] {
                let fix = GnssFix::from_report(reported, valid, never).unwrap();
                assert_eq!(fix, GnssFix::None { reported });
                assert_eq!(fix.position(), None);
                assert_eq!(fix.velocity(), None);
            }
        }
        // A valid fix whose fields are out of range is refused, not dropped
        // to "no fix".
        assert_eq!(
            GnssFix::from_report(ReportedFix::ThreeD, true, never),
            Err(SensorError::OutOfRange)
        );
    }

    #[test]
    fn a_gnss_value_outside_its_physical_range_is_refused() {
        let height = Millimetres(0);
        let accuracy = AccuracyMillimetres(0);
        for (latitude, longitude) in [
            (900_000_000, 1_800_000_000),
            (-900_000_000, -1_800_000_000),
            (0, 0),
        ] {
            assert!(Position::new(latitude, longitude, height, height, accuracy, accuracy).is_ok());
        }
        for (latitude, longitude) in [
            (900_000_001, 0),
            (-900_000_001, 0),
            (0, 1_800_000_001),
            (0, -1_800_000_001),
            (i32::MIN, 0),
            (0, i32::MAX),
        ] {
            assert_eq!(
                Position::new(latitude, longitude, height, height, accuracy, accuracy),
                Err(SensorError::OutOfRange)
            );
        }

        let speed = MillimetresPerSecond(0);
        let speed_accuracy = AccuracyMillimetresPerSecond(0);
        let heading_accuracy = AccuracyDegrees1e5(0);
        for heading in [0, 35_999_999] {
            assert!(
                Velocity::new(
                    speed,
                    speed,
                    speed,
                    speed,
                    heading,
                    speed_accuracy,
                    heading_accuracy
                )
                .is_ok()
            );
        }
        for heading in [-1, 36_000_000, i32::MIN, i32::MAX] {
            assert_eq!(
                Velocity::new(
                    speed,
                    speed,
                    speed,
                    speed,
                    heading,
                    speed_accuracy,
                    heading_accuracy
                ),
                Err(SensorError::OutOfRange)
            );
        }
        assert_eq!(
            Velocity::new(
                speed,
                speed,
                speed,
                MillimetresPerSecond(-1),
                0,
                speed_accuracy,
                heading_accuracy
            ),
            Err(SensorError::OutOfRange)
        );

        assert!(UtcTime::new(2026, 12, 31, 23, 59, 60, 999_999_999).is_ok());
        assert!(UtcTime::new(2026, 1, 1, 0, 0, 0, -999_999_999).is_ok());
        for (month, day, hour, minute, second, nanosecond) in [
            (0, 1, 0, 0, 0, 0),
            (13, 1, 0, 0, 0, 0),
            (1, 0, 0, 0, 0, 0),
            (1, 32, 0, 0, 0, 0),
            (1, 1, 24, 0, 0, 0),
            (1, 1, 0, 60, 0, 0),
            (1, 1, 0, 0, 61, 0),
            (1, 1, 0, 0, 0, 1_000_000_000),
            (1, 1, 0, 0, 0, -1_000_000_000),
            (1, 1, 0, 0, 0, i32::MIN),
        ] {
            assert_eq!(
                UtcTime::new(2026, month, day, hour, minute, second, nanosecond),
                Err(SensorError::OutOfRange)
            );
        }
    }

    #[test]
    fn a_utc_time_exists_only_when_marked_valid_and_resolved() {
        let all = UtcValidity {
            date: true,
            time: true,
            fully_resolved: true,
        };
        assert_eq!(
            UtcTime::from_report(all, 2026, 11, 4, 12, 30, 15, 0),
            Ok(Some(UtcTime::new(2026, 11, 4, 12, 30, 15, 0).unwrap()))
        );
        for validity in [
            UtcValidity { date: false, ..all },
            UtcValidity { time: false, ..all },
            UtcValidity {
                fully_resolved: false,
                ..all
            },
            UtcValidity::default(),
        ] {
            assert_eq!(
                UtcTime::from_report(validity, 2026, 11, 4, 12, 30, 15, 0),
                Ok(None)
            );
            // An invalid time's fields are never read, garbage or not.
            assert_eq!(
                UtcTime::from_report(validity, 0, 0, 0, 99, 99, 99, i32::MAX),
                Ok(None)
            );
        }
        assert_eq!(
            UtcTime::from_report(all, 2026, 13, 4, 12, 30, 15, 0),
            Err(SensorError::OutOfRange)
        );

        let solution = GnssSolution {
            fix: GnssFix::from_report(ReportedFix::NoFix, false, never).unwrap(),
            satellites_used: 0,
            pdop: Dop(9_999),
            utc: UtcTime::from_report(UtcValidity::default(), 0, 0, 0, 0, 0, 0, 0).unwrap(),
        };
        assert_eq!(solution.fix.position(), None);
        assert_eq!(solution.utc, None);
    }
}
