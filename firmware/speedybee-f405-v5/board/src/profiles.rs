//! Requirements: CTRL-002, CTRL-015, MOTOR-001, CONFIG-033

use ferrowasp_config::owner::BoardRules;
use ferrowasp_config::stored::StoredConfig;
use ferrowasp_control::calibration::accel_level::AccelLevel;
use ferrowasp_control::frames::{DroneBodyFrame, FrameRotation, ImuControlAxisProfile};
use ferrowasp_flight_control::actuator::MotorOutputMap;
use ferrowasp_flight_control::drone_toolbox::IMU_GYRO_LPF_HZ;
use ferrowasp_flight_control::drone_toolbox::PidGains;
use ferrowasp_flight_control::drone_toolbox::RC_RATE_PROFILE;
use ferrowasp_flight_control::drone_toolbox::RateControllerGains;
use ferrowasp_flight_control::drone_toolbox::RcChannelMap;
use ferrowasp_flight_control::drone_toolbox::TuningProfile;
use ferrowasp_serial::RcProtocol;

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

// The upstream SPEEDYBEEF405V5 Betaflight target uses the default VBAT scale
// (110, represented here as an 11.0 divider ratio) and explicitly selects
// current scale 36, the current meter on the ADC, and the voltage meter on
// the ADC. The board is specified for 2-6S, so the cell limit is 6. Nothing
// is measured on this board yet, so the baseline stays unverified until the
// bench bring-up reads a known pack.
pub const ADC_OBSERVATION_PROFILE: AdcObservationProfile = AdcObservationProfile {
    vbat_divider_ratio: 11.0,
    current_betaflight_scale: 36,
    current_offset_ma: 0,
    battery_max_cell_mv: 4_300,
    battery_detect_cell_mv: 3_000,
    battery_max_cells: 6,
    documented_baseline_verified: false,
};

/// How fast this board runs its rate loop, and the scheduler tick that drives
/// it.
///
/// Foxeer-derived: this board carries the same ICM42688-P at a 2 kHz ODR, so
/// the loop matches it and every cycle has a new sample, exactly as the
/// Foxeer F405 V2's proven profile does.
pub const CONTROL_LOOP_RATE_HZ: u32 = 2_000;
pub const SCHEDULER_TICK_RATE_HZ: u32 = 2_000;

// The scheduler divides down with integer division, so a pair that does not
// divide exactly would run a rate this board does not claim.
const _: () = assert!(
    ferrowasp_flight_control::drone_toolbox::scheduler_divides_exactly(
        SCHEDULER_TICK_RATE_HZ,
        CONTROL_LOOP_RATE_HZ
    )
);

/// Derived, not measured. The SPEEDYBEEF405V5 Betaflight target aligns
/// GYRO_1 as `CW270_DEG`, the same alignment as the Foxeer F405 V2's target,
/// so the derivation is the one the Foxeer's measured profile confirms:
/// Betaflight's forward-left-up CW270 reads `[-y, x, z]`, and negating left
/// and up for this crate's forward-right-down frame gives `[-y, -x, -z]`.
/// This board has not been measured yet, so the orientation stays unverified
/// and arming stays inhibited until the bench confirms it.
pub const IMU_CONTROL_AXIS_PROFILE: ImuControlAxisProfile = ImuControlAxisProfile {
    gyro_counts_per_dps10: 164,
    drone_body_frame: DroneBodyFrame::ForwardRightDown,
    imu_to_board_rotation: FrameRotation::new([1, 0, 2], [-1, -1, -1]),
    board_to_drone_rotation: FrameRotation::IDENTITY,
    bias_calibration_samples: 800,
    bias_calibration_max_raw: 1000,
};

pub const IMU_SENSOR_IDENTITY_VERIFIED: bool = false;
pub const IMU_ORIENTATION_VERIFIED: bool = false;
pub const MOTOR_OUTPUT_ORDER_VERIFIED: bool = false;
pub const LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT: [usize; 4] = [1, 2, 3, 4];
/// Whether the identity and orientation checks above cover this fitted IMU.
///
/// The fitted part is the ICM42688-P; none has been checked on this board
/// yet. Bench verification marks the ICM42688-P; the MPU6500 and MPU-6000 may
/// sit in a different package orientation, so they sample and report but
/// cannot arm until their axes and signs are checked on a board that carries
/// one.
pub const fn imu_kind_flight_verified(kind: super::manifest::Spi1ImuKind) -> bool {
    use super::manifest::Spi1ImuKind;
    match kind {
        Spi1ImuKind::Icm42688P | Spi1ImuKind::Mpu6500 | Spi1ImuKind::Mpu6000 => false,
    }
}

pub const FLIGHT_ARMING_ENABLED: bool = IMU_SENSOR_IDENTITY_VERIFIED
    && IMU_ORIENTATION_VERIFIED
    && ADC_OBSERVATION_PROFILE.documented_baseline_verified
    && MOTOR_OUTPUT_ORDER_VERIFIED;
pub const ARMING_INHIBIT_REASON: &str =
    "SpeedyBee F405 V5 IMU orientation, motor order, and ADC scale are not bench-verified";

/// The SpeedyBee F405 V5's fallback tune when storage holds no valid
/// configuration. It starts at the Foxeer F405 V2's approved P-only values
/// and is not yet tuned on this board; persisted configuration takes
/// precedence.
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
    // AETR with the arm switch on channel 9, Foxeer-derived: the SBUS
    // receiver on the O4 connector reads the same mapping the Foxeer's
    // proven profile reads.
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
    accel_level: AccelLevel::ZERO,
};

/// What every configuration change, and every saved configuration at boot,
/// is checked against: the ports this board routes and its default
/// bindings.
pub const BOARD_RULES: BoardRules = BoardRules {
    routes: crate::serial::SERIAL_ROUTES,
    default_bindings: crate::serial::DEFAULT_SERIAL_BINDINGS,
};

// The approved P-only baseline: changing it is a tuning change and re-gates
// the next flight.
const _: () = {
    let gains = DEFAULT_TUNING.rate_gains;
    assert!(gains.roll.p == 2.5 && gains.pitch.p == 2.5 && gains.yaw.p == 2.0);
    assert!(gains.roll.i == 0.0 && gains.pitch.i == 0.0 && gains.yaw.i == 0.0);
    assert!(gains.roll.d == 0.0 && gains.pitch.d == 0.0 && gains.yaw.d == 0.0);
    // The arm switch the Foxeer flew with.
    assert!(DEFAULT_TUNING.rc_map.arm_channel() == 9);
    // The motor order the board wires.
    assert!(DEFAULT_TUNING.motor_map.to_config() == 1234);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_configuration_passes_the_boards_rules() {
        assert!(ferrowasp_config::owner::validate(
            &DEFAULT_STORED_CONFIG,
            &BOARD_RULES
        ));
    }

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
    fn no_fitted_imu_is_verified_for_flight_yet() {
        use super::super::manifest::Spi1ImuKind;
        assert!(!imu_kind_flight_verified(Spi1ImuKind::Icm42688P));
        assert!(!imu_kind_flight_verified(Spi1ImuKind::Mpu6500));
        assert!(!imu_kind_flight_verified(Spi1ImuKind::Mpu6000));
    }

    #[test]
    fn adc_baseline_uses_the_speedybee_betaflight_target_values() {
        assert_eq!(ADC_OBSERVATION_PROFILE.vbat_divider_ratio, 11.0);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_betaflight_scale, 36);
        assert_eq!(ADC_OBSERVATION_PROFILE.current_offset_ma, 0);
        assert_eq!(ADC_OBSERVATION_PROFILE.battery_max_cells, 6);
    }

    #[test]
    fn imu_rotation_is_the_betaflight_cw270_in_forward_right_down() {
        // The same derivation the Foxeer's measured CW270 profile confirms:
        // the Foxeer F405 V2's bench-measured mapping for these inputs is
        // the oracle this board's derived profile must reproduce.
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
        assert_eq!(
            IMU_CONTROL_AXIS_PROFILE.sensor_accel_to_drone_gravity([0.0, 0.0, 1.0]),
            [0.0, 0.0, 1.0]
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

    #[test]
    fn the_control_loop_matches_the_fitted_sensors_2khz_odr() {
        assert_eq!(CONTROL_LOOP_RATE_HZ, 2_000);
        assert_eq!(SCHEDULER_TICK_RATE_HZ, 2_000);
    }
}
