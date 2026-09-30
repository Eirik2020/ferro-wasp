use ferrowasp_core::frames::{DroneBodyFrame, FrameRotation, ImuControlAxisProfile};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdcObservationProfile {
    pub vbat_divider_ratio: f32,
    pub current_betaflight_scale: u32,
    pub current_offset_ma: i32,
    pub battery_max_cell_mv: u16,
    pub battery_detect_cell_mv: u16,
    pub battery_max_cells: u8,
    pub documented_baseline_verified: bool,
}

// The upstream TBS_LUCID_H7 Betaflight target uses the default VBAT scale
// (110, represented here as an 11.0 divider ratio) and selects current scale
// 250; ArduPilot's hwdef agrees (voltage 11.0, 40 A/V). Neither is measured on
// this board yet, so the baseline stays unverified.
pub const ADC_OBSERVATION_PROFILE: AdcObservationProfile = AdcObservationProfile {
    vbat_divider_ratio: 11.0,
    current_betaflight_scale: 250,
    current_offset_ma: 0,
    battery_max_cell_mv: 4_300,
    battery_detect_cell_mv: 3_000,
    battery_max_cells: 8,
    documented_baseline_verified: false,
};

/// How fast this board runs its rate loop, and the scheduler tick that drives
/// it.
///
/// Kept equal to the Foxeer F405 V2: the ICM42688P runs at a 2 kHz ODR, so
/// every cycle has a new sample. An MPU6500 would limit the loop to 1 kHz.
pub const CONTROL_LOOP_RATE_HZ: u32 = 2_000;
pub const SCHEDULER_TICK_RATE_HZ: u32 = 2_000;

// The scheduler divides down with integer division, so a pair that does not
// divide exactly would run a rate this board does not claim.
const _: () = assert!(ferrowasp_tasks::drone_toolbox::scheduler_divides_exactly(
    SCHEDULER_TICK_RATE_HZ,
    CONTROL_LOOP_RATE_HZ
));

/// Derived, not measured. Betaflight's TBS_LUCID_H7 target aligns GYRO_1 as
/// `CW90_DEG_FLIP`, which maps sensor `[x, y, z]` to Betaflight's
/// forward-left-up board frame as `[y, x, -z]`. Converting that to this
/// crate's forward-right-down frame gives `[y, -x, z]`. The Foxeer profile,
/// which was measured, is the same derivation applied to its `CW270_DEG`.
/// ArduPilot's hwdef disagrees with Betaflight for this board, so the
/// orientation must be bench-verified before arming.
pub const IMU_CONTROL_AXIS_PROFILE: ImuControlAxisProfile = ImuControlAxisProfile {
    gyro_raw_to_dps: 164,
    drone_body_frame: DroneBodyFrame::ForwardRightDown,
    imu_to_board_rotation: FrameRotation::new([1, 0, 2], [1, -1, 1]),
    board_to_drone_rotation: FrameRotation::IDENTITY,
    bias_calibration_samples: 800,
    bias_calibration_max_raw: 1000,
};

pub const IMU_SENSOR_IDENTITY_VERIFIED: bool = false;
pub const IMU_ORIENTATION_VERIFIED: bool = false;
pub const MOTOR_OUTPUT_ORDER_VERIFIED: bool = false;
pub const LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT: [usize; 4] = [1, 2, 3, 4];
pub const FLIGHT_ARMING_ENABLED: bool = IMU_SENSOR_IDENTITY_VERIFIED
    && IMU_ORIENTATION_VERIFIED
    && ADC_OBSERVATION_PROFILE.documented_baseline_verified
    && MOTOR_OUTPUT_ORDER_VERIFIED;
pub const ARMING_INHIBIT_REASON: &str =
    "TBS Lucid H7 IMU orientation, motor order, and ADC scale are not bench-verified";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arming_stays_disabled_until_the_board_is_bench_verified() {
        let verification_flags = [
            ADC_OBSERVATION_PROFILE.documented_baseline_verified,
            IMU_SENSOR_IDENTITY_VERIFIED,
            IMU_ORIENTATION_VERIFIED,
            MOTOR_OUTPUT_ORDER_VERIFIED,
            FLIGHT_ARMING_ENABLED,
        ];

        assert_eq!(verification_flags, [false; 5]);
    }

    #[test]
    fn adc_baseline_uses_the_lucid_betaflight_target_values() {
        assert_eq!(ADC_OBSERVATION_PROFILE.vbat_divider_ratio, 11.0);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_betaflight_scale, 250);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_offset_ma, 0);
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_max_cells, 8);
    }

    #[test]
    fn imu_rotation_is_the_betaflight_cw90_flip_in_forward_right_down() {
        // Betaflight CW90_DEG_FLIP: forward-left-up = [y, x, -z]; negate left
        // and up for forward-right-down.
        let [x, y, z] = [10, 20, -30];
        let betaflight = [y, x, -z];
        let expected = [betaflight[0], -betaflight[1], -betaflight[2]];

        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE
                .imu_to_drone_rotation()
                .map_raw([x as i16, y as i16, z as i16]),
            expected
        );
    }

    #[test]
    fn motor_order_uses_the_standard_betaflight_quad_x_numbering() {
        assert_eq!(LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT, [1, 2, 3, 4]);

        let mut seen = [false; 4];
        for physical_output in LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT {
            assert!((1..=4).contains(&physical_output));
            assert!(!seen[physical_output - 1]);
            seen[physical_output - 1] = true;
        }
        assert_eq!(seen, [true; 4]);
    }
}
