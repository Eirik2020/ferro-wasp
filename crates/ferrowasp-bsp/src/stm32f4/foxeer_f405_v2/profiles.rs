use ferrowasp_core::frames::{BODY_RATE_TO_RATE_CONTROLLER_MAP, DroneBodyFrame, FrameRotation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputProfile {
    RcPwmBringup,
    FlightPwm,
    FlightDshotTimerDmaDeferred,
}

#[cfg(not(feature = "foxeer-f405-v2-dshot-four"))]
pub const DEFAULT_OUTPUT_PROFILE: OutputProfile = OutputProfile::RcPwmBringup;
#[cfg(feature = "foxeer-f405-v2-dshot-four")]
pub const DEFAULT_OUTPUT_PROFILE: OutputProfile = OutputProfile::FlightDshotTimerDmaDeferred;
pub const RC_PWM_NOTE: &str =
    "Foxeer M1-M4 use conventional 400 Hz, 1000..2000 us RC PWM during bring-up.";
pub const PWM_DMA_NOTE: &str =
    "Foxeer uses TIM1/TIM8 static PWM by default and gated timer-DMA DShot600 for commissioning.";
// DShot stop frames already run continuously while disarmed. Match FCU3's
// short guarded safety-recheck dwell rather than applying the PWM ESC arming
// delay to the digital protocol.
pub const DSHOT_PREARM_STOP_HOLD_MS: u32 = 100;
pub const DSHOT_IDLE_THROTTLE_COMMAND: u16 = 65;
// Match the FCU3 golden arming policy. Foxeer target evidence measured
// 6,600..7,200 eRPM at this idle command, leaving substantial margin from
// both the stopped-motor floor and overspeed ceiling.
pub const DSHOT_IDLE_QUALIFICATION_MIN_ERPM_DIV100: u16 = 30;
pub const DSHOT_IDLE_QUALIFICATION_MAX_ERPM_DIV100: u16 = 100;
pub const DSHOT_IDLE_QUALIFICATION_SPINUP_GRACE_MS: u32 = 250;
pub const DSHOT_IDLE_QUALIFICATION_TIMEOUT_MS: u32 = 1_200;
pub const DSHOT_IDLE_QUALIFICATION_MAX_SAMPLE_AGE_MS: u32 = 200;
pub const DSHOT_IDLE_QUALIFICATION_CONSECUTIVE_SAMPLES: u8 = 3;

const _: () = {
    assert!(DSHOT_PREARM_STOP_HOLD_MS > 0);
    assert!(DSHOT_IDLE_THROTTLE_COMMAND <= 250);
    assert!(DSHOT_IDLE_QUALIFICATION_MIN_ERPM_DIV100 > 0);
    assert!(DSHOT_IDLE_QUALIFICATION_MIN_ERPM_DIV100 < DSHOT_IDLE_QUALIFICATION_MAX_ERPM_DIV100);
    assert!(DSHOT_IDLE_QUALIFICATION_SPINUP_GRACE_MS < DSHOT_IDLE_QUALIFICATION_TIMEOUT_MS);
    assert!(DSHOT_IDLE_QUALIFICATION_CONSECUTIVE_SAMPLES > 0);
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdcObservationProfile {
    pub vbat_divider_ratio: f32,
    pub current_betaflight_scale: u32,
    pub battery_cell_count: u8,
    pub documented_baseline_verified: bool,
    pub current_offset_calibrated: bool,
}

// The upstream FOXEERF405V2 Betaflight target uses the default VBAT scale
// (110, represented here as an 11.0 divider ratio) and explicitly selects
// current scale 70. Foxeer also publishes scale 70 for the bundled Reaper 55A
// ESC. Powered bring-up consistently reported a plausible 23.2..24.0 V pack.
// This is sufficient as a provisional flight baseline, not fine calibration.
pub const ADC_OBSERVATION_PROFILE: AdcObservationProfile = AdcObservationProfile {
    vbat_divider_ratio: 11.0,
    current_betaflight_scale: 70,
    battery_cell_count: 0,
    documented_baseline_verified: true,
    current_offset_calibrated: false,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuControlAxisProfile {
    pub gyro_raw_to_dps: u32,
    pub drone_body_frame: DroneBodyFrame,
    pub imu_to_board_rotation: FrameRotation,
    pub board_to_drone_rotation: FrameRotation,
    pub bias_calibration_samples: u32,
    pub bias_calibration_max_raw: i32,
    pub sensor_identity_verified: bool,
    pub orientation_verified: bool,
}

impl ImuControlAxisProfile {
    pub const fn imu_to_drone_rotation(self) -> FrameRotation {
        self.imu_to_board_rotation
            .then(self.board_to_drone_rotation)
    }

    /// Maps sensor angular rates into the established FCU3 rate/mixer sign
    /// convention. This is intentionally distinct from the physical
    /// sensor-to-body rotation used by acceleration and orientation reporting.
    pub const fn imu_to_rate_controller_map(self) -> FrameRotation {
        self.imu_to_drone_rotation()
            .then(BODY_RATE_TO_RATE_CONTROLLER_MAP)
    }

    /// Converts sensor specific force into the drone-frame gravity direction
    /// consumed by the current complementary attitude estimator.
    pub fn sensor_accel_to_drone_gravity(self, sensor_accel: [f32; 3]) -> [f32; 3] {
        let specific_force = self.imu_to_drone_rotation().map_f32(sensor_accel);
        [-specific_force[0], -specific_force[1], -specific_force[2]]
    }
}

pub const IMU_CONTROL_AXIS_PROFILE: ImuControlAxisProfile = ImuControlAxisProfile {
    gyro_raw_to_dps: 164,
    drone_body_frame: DroneBodyFrame::ForwardRightDown,
    imu_to_board_rotation: FrameRotation::new([1, 0, 2], [-1, -1, -1]),
    board_to_drone_rotation: FrameRotation::IDENTITY,
    bias_calibration_samples: 800,
    bias_calibration_max_raw: 1000,
    sensor_identity_verified: true,
    orientation_verified: true,
};

pub const M4_COMPLEMENTARY_POLARITY_VERIFIED: bool = true;
pub const MOTOR_OUTPUT_ORDER_VERIFIED: bool = true;
pub const LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT: [usize; 4] = [1, 2, 3, 4];
pub const FLIGHT_ARMING_ENABLED: bool = IMU_CONTROL_AXIS_PROFILE.sensor_identity_verified
    && IMU_CONTROL_AXIS_PROFILE.orientation_verified
    && ADC_OBSERVATION_PROFILE.documented_baseline_verified
    && M4_COMPLEMENTARY_POLARITY_VERIFIED
    && MOTOR_OUTPUT_ORDER_VERIFIED;
pub const ARMING_INHIBIT_REASON: &str = "Foxeer board flight-verification profile is incomplete";

pub const ESC_PWM_FREQUENCY_HZ: u32 = 400;
pub const ESC_PWM_MIN_US: u16 = 1000;
pub const ESC_PWM_MAX_US: u16 = 2000;
pub const ESC_COMMAND_MAX: u16 = 2000;

pub fn remap_motor_outputs(logical: [f32; 4]) -> [f32; 4] {
    let mut physical = [0.0; 4];

    for (logical_index, physical_output) in
        LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT.iter().copied().enumerate()
    {
        if (1..=4).contains(&physical_output) {
            physical[physical_output - 1] = logical[logical_index];
        }
    }

    physical
}

pub const fn pwm_command_to_pulse_width_us(command: u16) -> Option<u16> {
    if command > ESC_COMMAND_MAX {
        return None;
    }

    let pulse_span = (ESC_PWM_MAX_US - ESC_PWM_MIN_US) as u32;
    let scaled = (command as u32 * pulse_span) / ESC_COMMAND_MAX as u32;
    Some(ESC_PWM_MIN_US + scaled as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_adc_baseline_allows_flight_without_claiming_fine_current_calibration() {
        let verification_flags = [
            ADC_OBSERVATION_PROFILE.documented_baseline_verified,
            IMU_CONTROL_AXIS_PROFILE.sensor_identity_verified,
            IMU_CONTROL_AXIS_PROFILE.orientation_verified,
            M4_COMPLEMENTARY_POLARITY_VERIFIED,
            MOTOR_OUTPUT_ORDER_VERIFIED,
            FLIGHT_ARMING_ENABLED,
            ADC_OBSERVATION_PROFILE.current_offset_calibrated,
        ];

        assert_eq!(
            verification_flags,
            [true, true, true, true, true, true, false]
        );
        assert_eq!(ADC_OBSERVATION_PROFILE.vbat_divider_ratio, 11.0);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_betaflight_scale, 70);
    }

    #[test]
    fn imu_frame_profile_matches_the_measured_foxeer_orientation() {
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.drone_body_frame,
            DroneBodyFrame::ForwardRightDown
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.imu_to_board_rotation,
            FrameRotation::new([1, 0, 2], [-1, -1, -1])
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.board_to_drone_rotation,
            FrameRotation::IDENTITY
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE
                .imu_to_drone_rotation()
                .map_raw([10, 20, -30]),
            [-20, -10, 30]
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE
                .imu_to_rate_controller_map()
                .map_raw([10, 20, -30]),
            [-20, 10, 30]
        );
        let verified = [
            IMU_CONTROL_AXIS_PROFILE.sensor_identity_verified,
            IMU_CONTROL_AXIS_PROFILE.orientation_verified,
        ];
        assert_eq!(verified, [true; 2]);
    }

    #[test]
    fn measured_nose_up_rate_maps_negative_for_the_golden_controller() {
        // Target evidence established physical nose-up as negative sensor X.
        let sensor_nose_up = [-100, 0, 0];
        let body = IMU_CONTROL_AXIS_PROFILE
            .imu_to_drone_rotation()
            .map_raw(sensor_nose_up);
        let controller = IMU_CONTROL_AXIS_PROFILE
            .imu_to_rate_controller_map()
            .map_raw(sensor_nose_up);

        assert_eq!(body, [0, 100, 0]);
        assert_eq!(controller, [0, -100, 0]);
    }

    #[test]
    fn measured_nose_down_rate_maps_positive_for_the_golden_controller() {
        // The failed-hop blackbox established physical nose-down as positive
        // sensor X. The controller must therefore see positive pitch rate and
        // command the opposing front-motor correction.
        let sensor_nose_down = [100, 0, 0];
        let controller = IMU_CONTROL_AXIS_PROFILE
            .imu_to_rate_controller_map()
            .map_raw(sensor_nose_down);

        assert_eq!(controller, [0, 100, 0]);
    }

    #[test]
    fn controller_compatibility_preserves_verified_roll_and_yaw_signs() {
        let map = IMU_CONTROL_AXIS_PROFILE.imu_to_rate_controller_map();

        // Target evidence: negative sensor Y is physical right-side-down,
        // and negative sensor Z is a rightward yaw.
        assert_eq!(map.map_raw([0, -100, 0]), [100, 0, 0]);
        assert_eq!(map.map_raw([0, 0, -100]), [0, 0, 100]);
    }

    #[test]
    fn measured_acceleration_maps_to_estimator_gravity_convention() {
        let level = IMU_CONTROL_AXIS_PROFILE.sensor_accel_to_drone_gravity([0.0, 0.0, 1.0]);
        assert_eq!(level, [0.0, 0.0, 1.0]);

        let nose_up = IMU_CONTROL_AXIS_PROFILE.sensor_accel_to_drone_gravity([0.0, -0.58, 0.81]);
        assert_eq!(nose_up, [-0.58, 0.0, 0.81]);

        let right_side_down =
            IMU_CONTROL_AXIS_PROFILE.sensor_accel_to_drone_gravity([0.78, 0.0, 0.62]);
        assert_eq!(right_side_down, [0.0, 0.78, 0.62]);
    }

    #[test]
    fn unknown_battery_cell_count_does_not_invent_airframe_configuration() {
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_cell_count, 0);
    }

    #[test]
    fn rc_pwm_command_mapping_matches_the_live_fcu_contract() {
        assert_eq!(pwm_command_to_pulse_width_us(0), Some(1000));
        assert_eq!(pwm_command_to_pulse_width_us(1000), Some(1500));
        assert_eq!(pwm_command_to_pulse_width_us(2000), Some(2000));
        assert_eq!(pwm_command_to_pulse_width_us(2001), None);
    }

    #[cfg(feature = "foxeer-f405-v2-dshot-four")]
    #[test]
    fn dshot_build_identifies_the_flight_output_profile() {
        assert_eq!(
            DEFAULT_OUTPUT_PROFILE,
            OutputProfile::FlightDshotTimerDmaDeferred
        );
    }

    #[cfg(not(feature = "foxeer-f405-v2-dshot-four"))]
    #[test]
    fn pwm_fallback_identifies_the_bringup_output_profile() {
        assert_eq!(DEFAULT_OUTPUT_PROFILE, OutputProfile::RcPwmBringup);
    }

    #[test]
    fn dshot_prearm_profile_matches_the_shared_fcu3_policy() {
        assert_eq!(DSHOT_PREARM_STOP_HOLD_MS, 100);
        assert_eq!(DSHOT_IDLE_THROTTLE_COMMAND, 65);
        assert_eq!(DSHOT_IDLE_QUALIFICATION_MIN_ERPM_DIV100, 30);
        assert_eq!(DSHOT_IDLE_QUALIFICATION_MAX_ERPM_DIV100, 100);
        assert_eq!(DSHOT_IDLE_QUALIFICATION_SPINUP_GRACE_MS, 250);
        assert_eq!(DSHOT_IDLE_QUALIFICATION_TIMEOUT_MS, 1_200);
        assert_eq!(DSHOT_IDLE_QUALIFICATION_MAX_SAMPLE_AGE_MS, 200);
        assert_eq!(DSHOT_IDLE_QUALIFICATION_CONSECUTIVE_SAMPLES, 3);
    }

    #[test]
    fn foxeer_motor_order_uses_the_standard_betaflight_quad_x_numbering() {
        assert_eq!(LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT, [1, 2, 3, 4]);
        assert_eq!(
            remap_motor_outputs([10.0, 20.0, 30.0, 40.0]),
            [10.0, 20.0, 30.0, 40.0]
        );

        let mut seen = [false; 4];
        for physical_output in LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT {
            assert!((1..=4).contains(&physical_output));
            assert!(!seen[physical_output - 1]);
            seen[physical_output - 1] = true;
        }
        assert_eq!(seen, [true; 4]);
    }
}
