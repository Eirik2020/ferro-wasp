use ferrowasp_core::frames::{DroneBodyFrame, FrameRotation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputProfile {
    RcPwmBringup,
    FlightPwm,
    FlightDshotTimerDmaDeferred,
}

pub const DEFAULT_OUTPUT_PROFILE: OutputProfile = OutputProfile::RcPwmBringup;
pub const RC_PWM_NOTE: &str =
    "Foxeer M1-M4 use conventional 400 Hz, 1000..2000 us RC PWM during bring-up.";
pub const PWM_DMA_NOTE: &str =
    "Foxeer TIM1/TIM8 motor DMA and DShot output are intentionally deferred.";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdcObservationProfile {
    pub vbat_divider_ratio: f32,
    pub current_betaflight_scale: u32,
    pub battery_cell_count: u8,
    pub calibrated: bool,
}

pub const ADC_OBSERVATION_PROFILE: AdcObservationProfile = AdcObservationProfile {
    vbat_divider_ratio: 11.0,
    current_betaflight_scale: 70,
    battery_cell_count: 0,
    calibrated: false,
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
}

pub const IMU_CONTROL_AXIS_PROFILE: ImuControlAxisProfile = ImuControlAxisProfile {
    gyro_raw_to_dps: 164,
    drone_body_frame: DroneBodyFrame::ForwardRightDown,
    imu_to_board_rotation: FrameRotation::IDENTITY,
    board_to_drone_rotation: FrameRotation::IDENTITY,
    bias_calibration_samples: 800,
    bias_calibration_max_raw: 1000,
    sensor_identity_verified: false,
    orientation_verified: false,
};

pub const M4_COMPLEMENTARY_POLARITY_VERIFIED: bool = false;
pub const MOTOR_OUTPUT_ORDER_VERIFIED: bool = false;
pub const LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT: [usize; 4] = [1, 2, 3, 4];
pub const FLIGHT_ARMING_ENABLED: bool = IMU_CONTROL_AXIS_PROFILE.sensor_identity_verified
    && IMU_CONTROL_AXIS_PROFILE.orientation_verified
    && ADC_OBSERVATION_PROFILE.calibrated
    && M4_COMPLEMENTARY_POLARITY_VERIFIED
    && MOTOR_OUTPUT_ORDER_VERIFIED;
pub const ARMING_INHIBIT_REASON: &str =
    "Foxeer IMU/orientation, ADC, motor order, and M4 CH3N polarity are unverified";

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
    fn provisional_profiles_keep_flight_arming_inhibited() {
        let verification_flags = [
            ADC_OBSERVATION_PROFILE.calibrated,
            IMU_CONTROL_AXIS_PROFILE.sensor_identity_verified,
            IMU_CONTROL_AXIS_PROFILE.orientation_verified,
            M4_COMPLEMENTARY_POLARITY_VERIFIED,
            MOTOR_OUTPUT_ORDER_VERIFIED,
            FLIGHT_ARMING_ENABLED,
        ];

        assert_eq!(verification_flags, [false; 6]);
    }

    #[test]
    fn imu_frame_profile_preserves_the_provisional_identity_mapping() {
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.drone_body_frame,
            DroneBodyFrame::ForwardRightDown
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.imu_to_board_rotation,
            FrameRotation::IDENTITY
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.board_to_drone_rotation,
            FrameRotation::IDENTITY
        );
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE
                .imu_to_drone_rotation()
                .map_raw([10, 20, -30]),
            [10, 20, -30]
        );
    }

    #[test]
    fn unknown_battery_cell_count_does_not_invent_airframe_configuration() {
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_cell_count, 0);
    }

    #[test]
    fn rc_pwm_command_mapping_matches_the_live_fcu_contract() {
        assert_eq!(DEFAULT_OUTPUT_PROFILE, OutputProfile::RcPwmBringup);
        assert_eq!(pwm_command_to_pulse_width_us(0), Some(1000));
        assert_eq!(pwm_command_to_pulse_width_us(1000), Some(1500));
        assert_eq!(pwm_command_to_pulse_width_us(2000), Some(2000));
        assert_eq!(pwm_command_to_pulse_width_us(2001), None);
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
