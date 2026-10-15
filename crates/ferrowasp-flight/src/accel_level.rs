//! Accelerometer level calibration.
//!
//! A pilot sets the craft still on level ground and asks for a calibration.
//! The mean gravity it reads then becomes "level": its offset from straight
//! down is stored and subtracted before the attitude estimate, so a board
//! mounted a few degrees off reads level when the frame is. The rate loop
//! never reads the accelerometer, so this changes the estimated attitude, and
//! nothing that flies the craft today.
//!
//! Offsets are in the drone frame, in milli-g, so they mean the same whatever
//! IMU the board fits.

/// Gravity as the drone frame sees it on level ground, in g.
const LEVEL_GRAVITY: [f32; 3] = [0.0, 0.0, 1.0];

/// Samples averaged into one calibration: about half a second at 1 kHz.
pub const CALIBRATION_SAMPLES: u16 = 500;

/// The most any gravity axis may wander during the window, in g, before the
/// craft counts as moving.
pub const MAX_ACCEL_SPREAD_G: f32 = 0.05;

/// The most any gyro axis may wander during the window, in degrees per
/// second. A spread rather than a level, so the gyro's bias does not count.
pub const MAX_GYRO_SPREAD_DPS: f32 = 10.0;

/// Cosine of the most the board may lean from level, 10 degrees. More than
/// that is a craft not on level ground, not a board mounted slightly off.
const MIN_LEVEL_COSINE: f32 = 0.984_807_8;

/// How far the gravity magnitude may be from 1 g. Further means the sensor
/// or its scale is wrong, which an offset must not paper over.
const MAX_GRAVITY_ERROR_G: f32 = 0.15;

/// The largest stored offset on any axis, in milli-g; anything larger was
/// not produced by a calibration this firmware accepts.
pub const MAX_OFFSET_MG: i16 = 400;

/// Where the accelerometer reads level: gravity minus this is the level
/// gravity the attitude estimate uses.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AccelLevel {
    pub offset_mg: [i16; 3],
}

impl AccelLevel {
    /// No correction: the state of a board never calibrated.
    pub const ZERO: Self = Self { offset_mg: [0; 3] };

    /// `None` when an offset is outside what a calibration can produce.
    pub const fn new(offset_mg: [i16; 3]) -> Option<Self> {
        let mut axis = 0;
        while axis < 3 {
            if offset_mg[axis] < -MAX_OFFSET_MG || offset_mg[axis] > MAX_OFFSET_MG {
                return None;
            }
            axis += 1;
        }
        Some(Self { offset_mg })
    }

    /// Drone-frame gravity in g, corrected to read level when the craft is.
    pub fn apply(self, gravity_g: [f32; 3]) -> [f32; 3] {
        [
            gravity_g[0] - f32::from(self.offset_mg[0]) / 1000.0,
            gravity_g[1] - f32::from(self.offset_mg[1]) / 1000.0,
            gravity_g[2] - f32::from(self.offset_mg[2]) / 1000.0,
        ]
    }
}

/// Why a calibration produced nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationError {
    /// The craft moved during the window.
    Moving,
    /// The craft leans more than 10 degrees.
    NotLevel,
    /// Gravity's magnitude is too far from 1 g to trust.
    BadGravity,
}

/// One calibration in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccelLevelCalibration {
    samples: u16,
    sum: [f32; 3],
    accel_min: [f32; 3],
    accel_max: [f32; 3],
    gyro_min: [f32; 3],
    gyro_max: [f32; 3],
}

impl Default for AccelLevelCalibration {
    fn default() -> Self {
        Self::new()
    }
}

impl AccelLevelCalibration {
    pub const fn new() -> Self {
        Self {
            samples: 0,
            sum: [0.0; 3],
            accel_min: [f32::MAX; 3],
            accel_max: [f32::MIN; 3],
            gyro_min: [f32::MAX; 3],
            gyro_max: [f32::MIN; 3],
        }
    }

    /// Adds one IMU sample: drone-frame gravity in g and the gyro in degrees
    /// per second, any axis order. `Some` once the window is complete.
    pub fn push(
        &mut self,
        gravity_g: [f32; 3],
        gyro_dps: [f32; 3],
    ) -> Option<Result<AccelLevel, CalibrationError>> {
        for axis in 0..3 {
            self.sum[axis] += gravity_g[axis];
            self.accel_min[axis] = self.accel_min[axis].min(gravity_g[axis]);
            self.accel_max[axis] = self.accel_max[axis].max(gravity_g[axis]);
            self.gyro_min[axis] = self.gyro_min[axis].min(gyro_dps[axis]);
            self.gyro_max[axis] = self.gyro_max[axis].max(gyro_dps[axis]);
        }
        self.samples += 1;
        (self.samples >= CALIBRATION_SAMPLES).then(|| self.finish())
    }

    fn finish(&self) -> Result<AccelLevel, CalibrationError> {
        for axis in 0..3 {
            let accel_spread = self.accel_max[axis] - self.accel_min[axis];
            let gyro_spread = self.gyro_max[axis] - self.gyro_min[axis];
            if accel_spread > MAX_ACCEL_SPREAD_G || gyro_spread > MAX_GYRO_SPREAD_DPS {
                return Err(CalibrationError::Moving);
            }
        }
        let count = f32::from(self.samples);
        let mean = [
            self.sum[0] / count,
            self.sum[1] / count,
            self.sum[2] / count,
        ];
        let magnitude = libm::sqrtf(mean[0] * mean[0] + mean[1] * mean[1] + mean[2] * mean[2]);
        // A NaN magnitude fails here too.
        if magnitude.is_nan() || libm::fabsf(magnitude - 1.0) > MAX_GRAVITY_ERROR_G {
            return Err(CalibrationError::BadGravity);
        }
        if mean[2] / magnitude < MIN_LEVEL_COSINE {
            return Err(CalibrationError::NotLevel);
        }
        let offset = |axis: usize| libm::roundf((mean[axis] - LEVEL_GRAVITY[axis]) * 1000.0) as i16;
        AccelLevel::new([offset(0), offset(1), offset(2)]).ok_or(CalibrationError::BadGravity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        gravity: impl Fn(u16) -> [f32; 3],
        gyro: impl Fn(u16) -> [f32; 3],
    ) -> Result<AccelLevel, CalibrationError> {
        let mut calibration = AccelLevelCalibration::new();
        for sample in 0..CALIBRATION_SAMPLES - 1 {
            assert_eq!(calibration.push(gravity(sample), gyro(sample)), None);
        }
        calibration
            .push(
                gravity(CALIBRATION_SAMPLES - 1),
                gyro(CALIBRATION_SAMPLES - 1),
            )
            .expect("the last sample completes the window")
    }

    #[test]
    fn a_still_board_mounted_slightly_off_is_corrected_to_level() {
        // About 3 degrees of roll, with a 1 dps gyro bias.
        let level = run(|_| [0.0, 0.052, 0.9986], |_| [1.0, -1.0, 0.5]).unwrap();
        assert_eq!(level.offset_mg, [0, 52, -1]);
        let corrected = level.apply([0.0, 0.052, 0.9986]);
        assert!(corrected[0].abs() < 1e-3 && corrected[1].abs() < 1e-3);
        assert!((corrected[2] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn movement_tilt_and_a_wrong_scale_are_refused() {
        let wobble = |sample: u16| [0.0, if sample.is_multiple_of(2) { 0.0 } else { 0.08 }, 1.0];
        assert_eq!(run(wobble, |_| [0.0; 3]), Err(CalibrationError::Moving));
        let turning = |sample: u16| [0.0, 0.0, f32::from(sample) * 0.05];
        assert_eq!(
            run(|_| [0.0, 0.0, 1.0], turning),
            Err(CalibrationError::Moving)
        );
        // 20 degrees of pitch.
        assert_eq!(
            run(|_| [0.342, 0.0, 0.94], |_| [0.0; 3]),
            Err(CalibrationError::NotLevel)
        );
        assert_eq!(
            run(|_| [0.0, 0.0, 0.7], |_| [0.0; 3]),
            Err(CalibrationError::BadGravity)
        );
        // A NaN sample poisons the mean, which no check passes.
        assert_eq!(
            run(|_| [0.0, 0.0, f32::NAN], |_| [0.0; 3]),
            Err(CalibrationError::BadGravity)
        );
    }

    #[test]
    fn stored_offsets_are_bounded() {
        assert!(AccelLevel::new([400, -400, 0]).is_some());
        assert!(AccelLevel::new([401, 0, 0]).is_none());
        assert_eq!(AccelLevel::ZERO.apply([0.1, 0.2, 0.9]), [0.1, 0.2, 0.9]);
    }
}
