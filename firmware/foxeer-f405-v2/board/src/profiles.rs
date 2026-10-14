use ferrowasp_core::actuator::MotorOutputMap;
use ferrowasp_core::frames::{DroneBodyFrame, FrameRotation, ImuControlAxisProfile};
use ferrowasp_flight::drone_toolbox::{
    IMU_GYRO_LPF_HZ, PidGains, RC_RATE_PROFILE, RateControllerGains, RcChannelMap, TuningProfile,
};
use ferrowasp_flight::flash_storage::StoredConfig;
use ferrowasp_io_core::serial::RcProtocol;

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

// The upstream FOXEERF405V2 Betaflight target uses the default VBAT scale
// (110, represented here as an 11.0 divider ratio) and explicitly selects
// current scale 70. Foxeer also publishes scale 70 for the bundled Reaper 55A
// ESC. It does not override Betaflight's default zero current offset or its
// 4.30 V cell-count detection threshold. Powered bring-up consistently
// reported a plausible 23.2..25.1 V pack. These are the published target
// values, not a fine per-airframe calibration.
pub const ADC_OBSERVATION_PROFILE: AdcObservationProfile = AdcObservationProfile {
    vbat_divider_ratio: 11.0,
    current_betaflight_scale: 70,
    current_offset_ma: 0,
    battery_max_cell_mv: 4_300,
    battery_detect_cell_mv: 3_000,
    battery_max_cells: 8,
    documented_baseline_verified: true,
};

/// How fast this board runs its rate loop, and the scheduler tick that drives
/// it.
///
/// Bounded by the sensor: this board carries an ICM42688P at a 2 kHz ODR, so
/// the loop matches it and every cycle has a new sample. A board with an
/// MPU6500 is limited to 1 kHz and must say so itself rather than inherit this.
///
/// Measured with props off: the loop holds 2 kHz and 4 kHz exactly, so the
/// processor is not the constraint. Going past 2 kHz means raising the sensor
/// ODR, which needs the 1 MHz SPI clock raised with it, which in turn retires
/// MPU6500 support - the part is specified at 1 MHz for register access.
pub const CONTROL_LOOP_RATE_HZ: u32 = 2_000;
pub const SCHEDULER_TICK_RATE_HZ: u32 = 2_000;

// The scheduler divides down with integer division, so a pair that does not
// divide exactly would run a rate this board does not claim.
const _: () = assert!(ferrowasp_flight::drone_toolbox::scheduler_divides_exactly(
    SCHEDULER_TICK_RATE_HZ,
    CONTROL_LOOP_RATE_HZ
));

pub const IMU_CONTROL_AXIS_PROFILE: ImuControlAxisProfile = ImuControlAxisProfile {
    gyro_raw_to_dps: 164,
    drone_body_frame: DroneBodyFrame::ForwardRightDown,
    imu_to_board_rotation: FrameRotation::new([1, 0, 2], [-1, -1, -1]),
    board_to_drone_rotation: FrameRotation::IDENTITY,
    bias_calibration_samples: 800,
    bias_calibration_max_raw: 1000,
};

pub const IMU_SENSOR_IDENTITY_VERIFIED: bool = true;
pub const IMU_ORIENTATION_VERIFIED: bool = true;
pub const M4_COMPLEMENTARY_POLARITY_VERIFIED: bool = true;
pub const MOTOR_OUTPUT_ORDER_VERIFIED: bool = true;
pub const LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT: [usize; 4] = [1, 2, 3, 4];
/// Whether the identity and orientation checks above cover this fitted IMU.
///
/// They were made with the ICM42688-P. The MPU6500 predates this gate and
/// keeps the behaviour it had. An MPU-6000 may sit in a different package
/// orientation, so it samples and reports but cannot arm until its axes and
/// signs are checked on a board that carries one.
pub const fn imu_kind_flight_verified(kind: super::manifest::Spi1ImuKind) -> bool {
    use super::manifest::Spi1ImuKind;
    match kind {
        Spi1ImuKind::Icm42688P | Spi1ImuKind::Mpu6500 => true,
        Spi1ImuKind::Mpu6000 => false,
    }
}

pub const FLIGHT_ARMING_ENABLED: bool = IMU_SENSOR_IDENTITY_VERIFIED
    && IMU_ORIENTATION_VERIFIED
    && ADC_OBSERVATION_PROFILE.documented_baseline_verified
    && M4_COMPLEMENTARY_POLARITY_VERIFIED
    && MOTOR_OUTPUT_ORDER_VERIFIED;
pub const ARMING_INHIBIT_REASON: &str = "Foxeer board flight-verification profile is incomplete";

/// The Foxeer F405 V2's approved P-only tune, the fallback when storage holds
/// no valid configuration. Persisted configuration takes precedence.
pub const DEFAULT_TUNING: TuningProfile = TuningProfile {
    rate_gains: RateControllerGains {
        roll: PidGains {
            p: 2.5,
            i: 0.0,
            d: 0.0,
        },
        pitch: PidGains {
            p: 2.5,
            i: 0.0,
            d: 0.0,
        },
        yaw: PidGains {
            p: 2.0,
            i: 0.0,
            d: 0.0,
        },
    },
    imu_lpf_hz: IMU_GYRO_LPF_HZ,
    rc_rates: RC_RATE_PROFILE,
    // AETR with the arm switch on channel 9, as this board has always read it.
    rc_map: RcChannelMap::AETR_ARM_CH9,
    // The board's own wiring, `LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT`, unchanged.
    motor_map: MotorOutputMap::IDENTITY,
};

/// The configuration this board boots with when storage holds none, and the
/// one a configurator reset restores. The serial bindings stay unset, so the
/// board's default bindings apply.
pub const DEFAULT_STORED_CONFIG: StoredConfig = StoredConfig {
    tuning: DEFAULT_TUNING,
    log_rate_divisor: 1,
    serial_bindings: None,
    rc_protocol: RcProtocol::Sbus,
};

// The approved P-only baseline: changing it is a tuning change and re-gates
// the next flight.
const _: () = {
    let gains = DEFAULT_TUNING.rate_gains;
    assert!(gains.roll.p == 2.5 && gains.pitch.p == 2.5 && gains.yaw.p == 2.0);
    assert!(gains.roll.i == 0.0 && gains.pitch.i == 0.0 && gains.yaw.i == 0.0);
    assert!(gains.roll.d == 0.0 && gains.pitch.d == 0.0 && gains.yaw.d == 0.0);
    // The arm switch it flew with.
    assert!(DEFAULT_TUNING.rc_map.arm_channel() == 9);
    // The motor order it flew with.
    assert!(DEFAULT_TUNING.motor_map.to_config() == 1234);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_adc_baseline_uses_the_foxeer_betaflight_target_values() {
        let verification_flags = [
            ADC_OBSERVATION_PROFILE.documented_baseline_verified,
            IMU_SENSOR_IDENTITY_VERIFIED,
            IMU_ORIENTATION_VERIFIED,
            M4_COMPLEMENTARY_POLARITY_VERIFIED,
            MOTOR_OUTPUT_ORDER_VERIFIED,
            FLIGHT_ARMING_ENABLED,
        ];

        assert_eq!(verification_flags, [true; 6]);
        assert_eq!(ADC_OBSERVATION_PROFILE.vbat_divider_ratio, 11.0);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_betaflight_scale, 70);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_offset_ma, 0);
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_max_cell_mv, 4_300);
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_detect_cell_mv, 3_000);
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_max_cells, 8);
    }

    #[test]
    fn an_unverified_mpu6000_cannot_arm() {
        use super::super::manifest::Spi1ImuKind;
        assert!(imu_kind_flight_verified(Spi1ImuKind::Icm42688P));
        assert!(!imu_kind_flight_verified(Spi1ImuKind::Mpu6000));
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
        let verified = [IMU_SENSOR_IDENTITY_VERIFIED, IMU_ORIENTATION_VERIFIED];
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
    fn battery_cell_detection_matches_the_foxeer_eight_s_limit() {
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_max_cells, 8);
    }

    #[test]
    fn foxeer_motor_order_uses_the_standard_betaflight_quad_x_numbering() {
        assert_eq!(LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT, [1, 2, 3, 4]);
        assert_eq!(
            ferrowasp_core::actuator::remap_motor_outputs(
                [10.0, 20.0, 30.0, 40.0],
                LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
            ),
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
