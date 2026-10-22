//! Requirements: CTRL-009, CTRL-021, CTRL-022
//! Safety-critical.

pub mod accel_level;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GyroBiasUpdate {
    pub corrected_raw: [i32; 3],
    pub newly_calibrated: bool,
    pub bias_raw: [i32; 3],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GyroBiasCalibrator {
    sum: [i64; 3],
    samples: u32,
    bias_raw: [i32; 3],
    ready: bool,
    required_samples: u32,
    max_motion_raw: i32,
}

impl GyroBiasCalibrator {
    pub const fn new(required_samples: u32, max_motion_raw: i32) -> Self {
        Self {
            sum: [0; 3],
            samples: 0,
            bias_raw: [0; 3],
            ready: false,
            required_samples,
            max_motion_raw,
        }
    }

    pub const fn ready(self) -> bool {
        self.ready
    }

    pub const fn bias_raw(self) -> [i32; 3] {
        self.bias_raw
    }

    pub fn update(&mut self, armed: bool, raw: [i32; 3]) -> GyroBiasUpdate {
        let mut newly_calibrated = false;

        if !armed && !self.ready {
            let calibrating_motion = raw.iter().any(|value| value.abs() > self.max_motion_raw);

            if calibrating_motion {
                self.sum = [0; 3];
                self.samples = 0;
            } else {
                for (axis, value) in raw.iter().enumerate() {
                    self.sum[axis] += *value as i64;
                }

                self.samples = self.samples.saturating_add(1);
                if self.samples >= self.required_samples {
                    for axis in 0..3 {
                        self.bias_raw[axis] =
                            (self.sum[axis] / self.required_samples as i64) as i32;
                    }
                    self.ready = true;
                    newly_calibrated = true;
                }
            }
        }

        let mut corrected_raw = raw;
        if self.ready {
            for (axis, value) in corrected_raw.iter_mut().enumerate() {
                *value -= self.bias_raw[axis];
            }
        }

        GyroBiasUpdate {
            corrected_raw,
            newly_calibrated,
            bias_raw: self.bias_raw,
        }
    }

    /// Advances calibration only for a newly received IMU sample.
    ///
    /// Repeated samples may still be bias-corrected for observation, but they
    /// must never count toward the stationary startup calibration window.
    pub fn update_if_fresh(&mut self, armed: bool, fresh: bool, raw: [i32; 3]) -> GyroBiasUpdate {
        if fresh {
            return self.update(armed, raw);
        }

        let mut corrected_raw = raw;
        if self.ready {
            for (axis, value) in corrected_raw.iter_mut().enumerate() {
                *value -= self.bias_raw[axis];
            }
        }

        GyroBiasUpdate {
            corrected_raw,
            newly_calibrated: false,
            bias_raw: self.bias_raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gyro_bias_calibrator_resets_on_motion_and_subtracts_bias() {
        let mut calibrator = GyroBiasCalibrator::new(3, 1000);

        assert_eq!(
            calibrator.update(false, [10, -20, 30]).corrected_raw,
            [10, -20, 30]
        );
        calibrator.update(false, [2000, 0, 0]);
        assert!(!calibrator.ready());

        calibrator.update(false, [12, -18, 33]);
        calibrator.update(false, [14, -16, 36]);
        let update = calibrator.update(false, [16, -14, 39]);

        assert!(update.newly_calibrated);
        assert_eq!(update.bias_raw, [14, -16, 36]);
        assert_eq!(update.corrected_raw, [2, 2, 3]);
        assert_eq!(
            calibrator.update(true, [20, -10, 40]).corrected_raw,
            [6, 6, 4]
        );
    }

    #[test]
    fn stale_samples_do_not_advance_gyro_bias_calibration() {
        let mut calibrator = GyroBiasCalibrator::new(2, 1000);

        for _ in 0..10 {
            let update = calibrator.update_if_fresh(false, false, [10, -20, 30]);
            assert!(!update.newly_calibrated);
        }
        assert!(!calibrator.ready());

        assert!(
            !calibrator
                .update_if_fresh(false, true, [10, -20, 30])
                .newly_calibrated
        );
        assert!(
            calibrator
                .update_if_fresh(false, true, [12, -18, 32])
                .newly_calibrated
        );
        assert_eq!(calibrator.bias_raw(), [11, -19, 31]);
    }
}
