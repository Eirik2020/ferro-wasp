//! The compass class: the magnetic field a magnetometer measures, in integer
//! units, and the limits a part declares its values must lie in.
//!
//! Integers at the class boundary, though drivers often compute in `f32`: a
//! sample then compares exactly, holds no NaN or infinity, and carries
//! straight into integer telemetry and logs. Nanotesla resolve a thousandth
//! of a microtesla, finer than any compass chip's accuracy, so the unit
//! loses nothing a part measures; Earth's field is about 20 000 to 65 000 nT,
//! far from an i32's ends.
//!
//! The sample is the field in the sensor's own frame, x, y and z as the part
//! numbers them; rotating it to the body frame is the consumer's job, as it
//! is on the IMU path.
//!
//! Requirements: SENSOR-015
//! Safety-critical.

use crate::class::{Sensor, SensorError};

/// 2^31, the first value an `i32` cannot hold, exactly representable in f32.
const I32_LIMIT: f32 = 2_147_483_648.0;

/// One magnetic-field component, in nanotesla (1e-9 tesla).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Nanotesla(i32);

impl Nanotesla {
    pub const fn from_nanotesla(nanotesla: i32) -> Self {
        Self(nanotesla)
    }

    /// The nearest nanotesla value to `microtesla`, or `None` for a
    /// non-finite value or one too large to hold. Never saturates, so an
    /// overflowed computation cannot pass as a field component.
    pub fn from_microtesla_f32(microtesla: f32) -> Option<Self> {
        if !microtesla.is_finite() {
            return None;
        }
        let scaled = microtesla * 1000.0;
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

    pub const fn nanotesla(self) -> i32 {
        self.0
    }
}

/// One compass measurement: the magnetic field in the sensor's frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompassSample {
    pub x: Nanotesla,
    pub y: Nanotesla,
    pub z: Nanotesla,
}

/// The range, inclusive, a part's field components must lie in to be
/// believed: its datasheet's measuring range. A component outside it is
/// refused, not clamped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompassLimits {
    pub min: Nanotesla,
    pub max: Nanotesla,
}

impl CompassLimits {
    /// The sample unchanged, or `SensorError::OutOfRange` if any component
    /// lies outside its limits.
    pub fn check(&self, sample: CompassSample) -> Result<CompassSample, SensorError> {
        let ok = [sample.x, sample.y, sample.z]
            .into_iter()
            .all(|component| component >= self.min && component <= self.max);
        if ok {
            Ok(sample)
        } else {
            Err(SensorError::OutOfRange)
        }
    }

    /// Build a sample from a driver's floating-point components: refused as
    /// `SensorError::OutOfRange` if any cannot be held or lies outside the
    /// limits.
    pub fn sample_from_f32(&self, microtesla: [f32; 3]) -> Result<CompassSample, SensorError> {
        let [x, y, z] = microtesla;
        let sample = CompassSample {
            x: Nanotesla::from_microtesla_f32(x).ok_or(SensorError::OutOfRange)?,
            y: Nanotesla::from_microtesla_f32(y).ok_or(SensorError::OutOfRange)?,
            z: Nanotesla::from_microtesla_f32(z).ok_or(SensorError::OutOfRange)?,
        };
        self.check(sample)
    }
}

/// A compass: any polled sensor whose sample is a [`CompassSample`].
pub trait Compass: Sensor<Sample = CompassSample> {}

impl<T: Sensor<Sample = CompassSample>> Compass for T {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The QMC5883L's ±8 gauss range, the widest a common part carries.
    const LIMITS: CompassLimits = CompassLimits {
        min: Nanotesla::from_nanotesla(-800_000),
        max: Nanotesla::from_nanotesla(800_000),
    };

    fn sample(x: i32, y: i32, z: i32) -> CompassSample {
        CompassSample {
            x: Nanotesla::from_nanotesla(x),
            y: Nanotesla::from_nanotesla(y),
            z: Nanotesla::from_nanotesla(z),
        }
    }

    #[test]
    fn a_compass_sample_outside_its_limits_is_refused() {
        assert_eq!(
            LIMITS.check(sample(45_000, -52_000, 30_000)),
            Ok(sample(45_000, -52_000, 30_000))
        );
        for (x, y, z) in [
            (800_001, 0, 0),
            (-800_001, 0, 0),
            (0, 800_001, 0),
            (0, 0, -800_001),
        ] {
            assert_eq!(LIMITS.check(sample(x, y, z)), Err(SensorError::OutOfRange));
        }
        // Both ends inclusive.
        assert!(LIMITS.check(sample(800_000, -800_000, 800_000)).is_ok());

        assert_eq!(
            LIMITS.sample_from_f32([45.0, -52.0, 30.0]),
            Ok(sample(45_000, -52_000, 30_000))
        );
        assert_eq!(
            LIMITS.sample_from_f32([45.0, 800.01, 30.0]),
            Err(SensorError::OutOfRange)
        );
        assert_eq!(
            LIMITS.sample_from_f32([f32::NAN, 0.0, 0.0]),
            Err(SensorError::OutOfRange)
        );
        assert_eq!(
            LIMITS.sample_from_f32([0.0, f32::INFINITY, 0.0]),
            Err(SensorError::OutOfRange)
        );
    }

    #[test]
    fn floating_point_components_round_both_ways_and_never_saturate() {
        assert_eq!(
            Nanotesla::from_microtesla_f32(52.4),
            Some(Nanotesla::from_nanotesla(52_400))
        );
        assert_eq!(
            Nanotesla::from_microtesla_f32(-52.4),
            Some(Nanotesla::from_nanotesla(-52_400))
        );
        assert_eq!(
            Nanotesla::from_microtesla_f32(0.0004),
            Some(Nanotesla::from_nanotesla(0))
        );
        assert_eq!(
            Nanotesla::from_microtesla_f32(-0.0006),
            Some(Nanotesla::from_nanotesla(-1))
        );
        assert_eq!(Nanotesla::from_microtesla_f32(f32::NAN), None);
        assert_eq!(Nanotesla::from_microtesla_f32(f32::INFINITY), None);
        assert_eq!(Nanotesla::from_microtesla_f32(f32::NEG_INFINITY), None);
        assert_eq!(Nanotesla::from_microtesla_f32(f32::MAX), None);
        assert_eq!(Nanotesla::from_microtesla_f32(f32::MIN), None);
        // 2^31 nanotesla is the first value an i32 cannot hold.
        assert_eq!(Nanotesla::from_microtesla_f32(2_147_483.7), None);
        let lowest = Nanotesla::from_microtesla_f32(-2_147_480.0).map(Nanotesla::nanotesla);
        assert!(lowest.is_some_and(|nanotesla| nanotesla < -2_147_400_000));
        let highest = Nanotesla::from_microtesla_f32(2_147_480.0).map(Nanotesla::nanotesla);
        assert!(highest.is_some_and(|nanotesla| nanotesla > 2_147_400_000));
    }
}
