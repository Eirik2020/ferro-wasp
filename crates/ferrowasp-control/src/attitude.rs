//! Requirements: CTRL-007, CTRL-020, CTRL-008
//! Safety-critical.

pub const IMU_COMPLEMENTARY_GYRO_WEIGHT: f32 = 0.98;
const RAD_TO_DEG: f32 = 57.295_78;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GyroAngleIntegrator {
    roll: f32,
    pitch: f32,
    yaw: f32,
}

impl GyroAngleIntegrator {
    pub const fn new() -> Self {
        Self {
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
        }
    }

    pub fn update(
        &mut self,
        roll_rate: f32,
        pitch_rate: f32,
        yaw_rate: f32,
        dt_seconds: f32,
    ) -> [f32; 3] {
        self.roll += roll_rate * dt_seconds;
        self.pitch += pitch_rate * dt_seconds;
        self.yaw += yaw_rate * dt_seconds;

        [self.roll, self.pitch, self.yaw]
    }

    pub fn update_with_accel(
        &mut self,
        rates_dps: [f32; 3],
        accel_g: [f32; 3],
        dt_seconds: f32,
    ) -> [f32; 3] {
        let (acc_roll, acc_pitch) = accel_roll_pitch_degrees(accel_g[0], accel_g[1], accel_g[2]);

        self.roll += rates_dps[0] * dt_seconds;
        self.pitch += rates_dps[1] * dt_seconds;
        self.yaw += rates_dps[2] * dt_seconds;

        self.roll = self.roll * IMU_COMPLEMENTARY_GYRO_WEIGHT
            + acc_roll * (1.0 - IMU_COMPLEMENTARY_GYRO_WEIGHT);
        self.pitch = self.pitch * IMU_COMPLEMENTARY_GYRO_WEIGHT
            + acc_pitch * (1.0 - IMU_COMPLEMENTARY_GYRO_WEIGHT);

        [self.roll, self.pitch, self.yaw]
    }
}

impl Default for GyroAngleIntegrator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn accel_roll_pitch_degrees(acc_x: f32, acc_y: f32, acc_z: f32) -> (f32, f32) {
    let roll = fast_atan2(acc_y, acc_z) * RAD_TO_DEG;
    let pitch = fast_atan2(-acc_x, fast_sqrt(acc_y * acc_y + acc_z * acc_z)) * RAD_TO_DEG;

    (roll, pitch)
}

fn fast_sqrt(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }

    let mut x = value;
    let mut i = 0;
    while i < 5 {
        x = 0.5 * (x + value / x);
        i += 1;
    }

    x
}

fn fast_atan2(y: f32, x: f32) -> f32 {
    const PI: f32 = core::f32::consts::PI;
    const FRAC_PI_2: f32 = core::f32::consts::FRAC_PI_2;
    const FRAC_PI_4: f32 = core::f32::consts::FRAC_PI_4;

    if x == 0.0 {
        if y > 0.0 {
            return FRAC_PI_2;
        }
        if y < 0.0 {
            return -FRAC_PI_2;
        }
        return 0.0;
    }

    let abs_y = if y < 0.0 { -y } else { y };
    let (angle, r) = if x >= 0.0 {
        (FRAC_PI_4, (x - abs_y) / (x + abs_y))
    } else {
        (3.0 * FRAC_PI_4, (x + abs_y) / (abs_y - x))
    };
    let angle = angle - FRAC_PI_4 * r;

    if y < 0.0 { -angle } else { angle.min(PI) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "actual {actual} != expected {expected}"
        );
    }

    #[test]
    fn gyro_integrator_accumulates_rates_and_uses_accel_correction() {
        let mut integrator = GyroAngleIntegrator::new();

        assert_eq!(
            integrator.update(100.0, -50.0, 25.0, 0.01),
            [1.0, -0.5, 0.25]
        );

        let corrected = integrator.update_with_accel([0.0; 3], [0.0, 1.0, 0.0], 0.01);

        assert_close(corrected[0], 2.78);
        assert_close(corrected[1], -0.49);
        assert_close(corrected[2], 0.25);
    }

    #[test]
    fn accelerometer_roll_pitch_have_expected_static_orientations() {
        let (roll, pitch) = accel_roll_pitch_degrees(0.0, 0.0, 1.0);
        assert_close(roll, 0.0);
        assert_close(pitch, 0.0);

        let (roll, pitch) = accel_roll_pitch_degrees(0.0, 1.0, 0.0);
        assert_close(roll, 90.0);
        assert_close(pitch, 0.0);
    }
}
