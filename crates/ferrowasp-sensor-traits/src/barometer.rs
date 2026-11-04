//! The barometer class: pressure and temperature from one measurement, in
//! integer units, and the limits a part declares its values must lie in.
//!
//! Integers at the class boundary, though drivers often compute in `f32`: a
//! sample then compares exactly, holds no NaN or infinity, and carries
//! straight into integer telemetry and logs. Centipascals resolve about 1 mm
//! of altitude and centidegrees Celsius are finer than any barometer's
//! accuracy, so the units lose nothing a part measures.
//!
//! Requirements: SENSOR-003, SENSOR-004
//! Safety-critical.

use crate::class::{Sensor, SensorError};

/// 2^32, the first value a `u32` cannot hold, exactly representable in f32.
const U32_LIMIT: f32 = 4_294_967_296.0;
/// 2^31, the first value an `i32` cannot hold, exactly representable in f32.
const I32_LIMIT: f32 = 2_147_483_648.0;

/// Absolute pressure, in centipascals (0.01 Pa).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Pressure(u32);

impl Pressure {
    pub const fn from_centipascals(centipascals: u32) -> Self {
        Self(centipascals)
    }

    /// Whole pascals; `None` where the centipascal value would not fit.
    pub const fn from_pascals(pascals: u32) -> Option<Self> {
        match pascals.checked_mul(100) {
            Some(centipascals) => Some(Self(centipascals)),
            None => None,
        }
    }

    /// The nearest centipascal value to `pascals`, or `None` for a
    /// non-finite or negative value or one too large to hold. Never
    /// saturates, so an overflowed computation cannot pass as a pressure.
    pub fn from_pascals_f32(pascals: f32) -> Option<Self> {
        if !pascals.is_finite() || pascals < 0.0 {
            return None;
        }
        let rounded = pascals * 100.0 + 0.5;
        if !rounded.is_finite() || rounded >= U32_LIMIT {
            return None;
        }
        Some(Self(rounded as u32))
    }

    pub const fn centipascals(self) -> u32 {
        self.0
    }
}

/// Temperature, in centidegrees Celsius (0.01 °C).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Temperature(i32);

impl Temperature {
    pub const fn from_centidegrees_celsius(centidegrees: i32) -> Self {
        Self(centidegrees)
    }

    /// The nearest centidegree value to `celsius`, or `None` for a
    /// non-finite value or one too large to hold. Never saturates.
    pub fn from_celsius_f32(celsius: f32) -> Option<Self> {
        if !celsius.is_finite() {
            return None;
        }
        let scaled = celsius * 100.0;
        let rounded = if scaled >= 0.0 {
            scaled + 0.5
        } else {
            scaled - 0.5
        };
        // Symmetric limits: at this magnitude f32 cannot tell i32::MIN from
        // values beyond it, so it is refused too.
        if !rounded.is_finite() || rounded >= I32_LIMIT || rounded <= -I32_LIMIT {
            return None;
        }
        Some(Self(rounded as i32))
    }

    pub const fn centidegrees_celsius(self) -> i32 {
        self.0
    }
}

/// One barometer measurement: pressure and the temperature measured with it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaroSample {
    pub pressure: Pressure,
    pub temperature: Temperature,
}

/// The range, inclusive, a part's values must lie in to be believed: its
/// datasheet's operating range. A value outside it is refused, not clamped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaroLimits {
    pub pressure_min: Pressure,
    pub pressure_max: Pressure,
    pub temperature_min: Temperature,
    pub temperature_max: Temperature,
}

impl BaroLimits {
    /// The sample unchanged, or `SensorError::OutOfRange` if either value
    /// lies outside its limits.
    pub fn check(&self, sample: BaroSample) -> Result<BaroSample, SensorError> {
        let pressure_ok =
            sample.pressure >= self.pressure_min && sample.pressure <= self.pressure_max;
        let temperature_ok = sample.temperature >= self.temperature_min
            && sample.temperature <= self.temperature_max;
        if pressure_ok && temperature_ok {
            Ok(sample)
        } else {
            Err(SensorError::OutOfRange)
        }
    }

    /// Build a sample from a driver's floating-point values: refused as
    /// `SensorError::OutOfRange` if either cannot be held or lies outside
    /// the limits.
    pub fn sample_from_f32(&self, pascals: f32, celsius: f32) -> Result<BaroSample, SensorError> {
        let pressure = Pressure::from_pascals_f32(pascals).ok_or(SensorError::OutOfRange)?;
        let temperature = Temperature::from_celsius_f32(celsius).ok_or(SensorError::OutOfRange)?;
        self.check(BaroSample {
            pressure,
            temperature,
        })
    }
}

/// A barometer: any polled sensor whose sample is a [`BaroSample`].
pub trait Barometer: Sensor<Sample = BaroSample> {}

impl<T: Sensor<Sample = BaroSample>> Barometer for T {}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: BaroLimits = BaroLimits {
        pressure_min: Pressure::from_centipascals(3_000_000),
        pressure_max: Pressure::from_centipascals(12_000_000),
        temperature_min: Temperature::from_centidegrees_celsius(-4_000),
        temperature_max: Temperature::from_centidegrees_celsius(8_500),
    };

    fn sample(centipascals: u32, centidegrees: i32) -> BaroSample {
        BaroSample {
            pressure: Pressure::from_centipascals(centipascals),
            temperature: Temperature::from_centidegrees_celsius(centidegrees),
        }
    }

    #[test]
    fn a_barometer_sample_outside_its_limits_is_refused() {
        assert!(LIMITS.check(sample(10_132_500, 2_500)).is_ok());
        assert!(LIMITS.check(sample(3_000_000, -4_000)).is_ok());
        assert!(LIMITS.check(sample(12_000_000, 8_500)).is_ok());

        assert_eq!(
            LIMITS.check(sample(2_999_999, 2_500)),
            Err(SensorError::OutOfRange)
        );
        assert_eq!(
            LIMITS.check(sample(12_000_001, 2_500)),
            Err(SensorError::OutOfRange)
        );
        assert_eq!(
            LIMITS.check(sample(10_132_500, -4_001)),
            Err(SensorError::OutOfRange)
        );
        assert_eq!(
            LIMITS.check(sample(10_132_500, 8_501)),
            Err(SensorError::OutOfRange)
        );
        assert_eq!(LIMITS.check(sample(0, 2_500)), Err(SensorError::OutOfRange));
        assert_eq!(
            LIMITS.check(sample(u32::MAX, i32::MIN)),
            Err(SensorError::OutOfRange)
        );

        assert!(LIMITS.sample_from_f32(101_325.0, 25.0).is_ok());
        assert_eq!(
            LIMITS.sample_from_f32(29_999.9, 25.0),
            Err(SensorError::OutOfRange)
        );
        assert_eq!(
            LIMITS.sample_from_f32(101_325.0, 85.01),
            Err(SensorError::OutOfRange)
        );
    }

    #[test]
    fn floating_point_pressure_rounds_and_never_saturates() {
        assert_eq!(
            Pressure::from_pascals_f32(101_325.0),
            Some(Pressure::from_centipascals(10_132_500))
        );
        assert_eq!(
            Pressure::from_pascals_f32(0.0),
            Some(Pressure::from_centipascals(0))
        );
        assert_eq!(
            Pressure::from_pascals_f32(-0.0),
            Some(Pressure::from_centipascals(0))
        );
        assert_eq!(
            Pressure::from_pascals_f32(0.004),
            Some(Pressure::from_centipascals(0))
        );
        assert_eq!(
            Pressure::from_pascals_f32(0.006),
            Some(Pressure::from_centipascals(1))
        );

        assert_eq!(Pressure::from_pascals_f32(f32::NAN), None);
        assert_eq!(Pressure::from_pascals_f32(f32::INFINITY), None);
        assert_eq!(Pressure::from_pascals_f32(f32::NEG_INFINITY), None);
        assert_eq!(Pressure::from_pascals_f32(f32::MAX), None);
        assert_eq!(Pressure::from_pascals_f32(-0.01), None);
        assert_eq!(Pressure::from_pascals_f32(f32::MIN), None);
        // 2^32 centipascals is the first value a u32 cannot hold; the largest
        // f32 below it still fits.
        assert_eq!(Pressure::from_pascals_f32(42_949_672.96), None);
        let largest = Pressure::from_pascals_f32(42_949_660.0).map(Pressure::centipascals);
        assert!(largest.is_some_and(|centipascals| centipascals > 4_294_960_000));

        assert_eq!(
            Pressure::from_pascals(1_013),
            Some(Pressure::from_centipascals(101_300))
        );
        assert_eq!(
            Pressure::from_pascals(u32::MAX / 100),
            Some(Pressure::from_centipascals(u32::MAX / 100 * 100))
        );
        assert_eq!(Pressure::from_pascals(u32::MAX / 100 + 1), None);
    }

    #[test]
    fn floating_point_temperature_rounds_both_ways_and_never_saturates() {
        assert_eq!(
            Temperature::from_celsius_f32(25.0),
            Some(Temperature::from_centidegrees_celsius(2_500))
        );
        assert_eq!(
            Temperature::from_celsius_f32(-40.0),
            Some(Temperature::from_centidegrees_celsius(-4_000))
        );
        assert_eq!(
            Temperature::from_celsius_f32(0.004),
            Some(Temperature::from_centidegrees_celsius(0))
        );
        assert_eq!(
            Temperature::from_celsius_f32(-0.006),
            Some(Temperature::from_centidegrees_celsius(-1))
        );

        assert_eq!(Temperature::from_celsius_f32(f32::NAN), None);
        assert_eq!(Temperature::from_celsius_f32(f32::INFINITY), None);
        assert_eq!(Temperature::from_celsius_f32(f32::NEG_INFINITY), None);
        assert_eq!(Temperature::from_celsius_f32(f32::MAX), None);
        assert_eq!(Temperature::from_celsius_f32(f32::MIN), None);
        assert_eq!(Temperature::from_celsius_f32(21_474_836.48), None);
        assert_eq!(Temperature::from_celsius_f32(-21_474_836.50), None);
        let lowest =
            Temperature::from_celsius_f32(-21_474_830.0).map(Temperature::centidegrees_celsius);
        assert!(lowest.is_some_and(|centidegrees| centidegrees < -2_147_480_000));
    }
}
