//! Reusable, HAL-independent Foxeer F405 V2 control-step behavior.
//!
//! The physical IMU installation rotation is applied by the board's decoder.
//! This module applies only the explicit physical-body to legacy-controller
//! compatibility map. It computes bounded motor requests but owns neither a
//! motor peripheral nor actuator authority.

use ferrowasp_core::{
    actuator::remap_motor_outputs,
    frames::{FrameRotation, RATE_CONTROLLER_TO_BODY_MAP},
    safety::{ESC_MAX_THROTTLE, MOTOR_CMD_MAX_AGE_MS, MotorCmd},
};
use ferrowasp_io_core::serial::RcInputSnapshot;

use crate::drone_toolbox::{
    CONTROL_LOOP_DT_SECONDS, FlightController, GyroAngleIntegrator, GyroBiasCalibrator,
    ImuRateLowPassFilter, TuningProfile, remap_rc_channels_with_profile,
};

/// MPU6500 raw counts per degree per second at the golden ±2000 dps setting.
pub const FOXEER_GYRO_RAW_TO_DPS: f32 = 16.4;

/// Physical Foxeer outputs use the golden Betaflight Quad-X logical order.
pub const FOXEER_LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT: [usize; 4] = [1, 2, 3, 4];

/// Number of 400 Hz control intervals spanning the 100 ms RC timeout.
pub const FOXEER_RC_FRESH_CONTROL_TICKS: u32 = 40;

/// Fixed tuning generation applied by this version of the generated task.
pub const FOXEER_TUNING_SEQUENCE: u32 = 1;

/// Body-frame inertial data consumed by one control step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImuControlSample {
    /// Body-frame specific force in g, already installation-rotated once.
    pub specific_force_body: [f32; 3],
    /// Body-frame raw angular rate, already installation-rotated once.
    pub gyro_raw_body: [i16; 3],
    /// Monotonic decoded-sample sequence.
    pub sequence: u32,
}

/// Inputs captured without blocking for one 400 Hz control step.
#[derive(Clone, Copy, Debug)]
pub struct FoxeerControlInput<'a> {
    /// Latest complete RC snapshot retained by the task.
    pub rc: &'a RcInputSnapshot,
    /// Whether this step received that snapshot from the exclusive channel.
    pub rc_observed: bool,
    /// Newest IMU channel sample, or `None` when the channel was empty.
    pub imu: Option<ImuControlSample>,
    /// Safety-owned arming state. Outer code may observe but not grant it.
    pub armed: bool,
    /// Validated runtime tuning, including the RC-rate mapping profile.
    pub tuning: TuningProfile,
}

/// Exclusive persistent state used by [`run_foxeer_control_step`].
pub struct FoxeerControlState<'a> {
    /// Golden rate controller and Quad-X mixer.
    pub flight_controller: &'a mut FlightController,
    /// Three-axis raw-rate low-pass filter.
    pub imu_rate_filter: &'a mut ImuRateLowPassFilter,
    /// Complementary body-angle estimator.
    pub imu_angle_integrator: &'a mut GyroAngleIntegrator,
    /// Physical-body to legacy-controller compatibility map.
    pub gyro_axis_map: &'a mut FrameRotation,
    /// Stationary startup raw-gyro bias calibration.
    pub gyro_bias_calibrator: &'a mut GyroBiasCalibrator,
    /// Last consumed decoded IMU sequence.
    pub imu_last_sequence: &'a mut u32,
    /// Consecutive stale IMU control steps.
    pub imu_stale_ticks: &'a mut u32,
    /// Last applied fixed tuning generation.
    pub applied_tuning_seq: &'a mut u32,
    /// Last observed valid RC frame count.
    pub rc_last_valid_frames: &'a mut u32,
    /// Consecutive control steps without a new healthy RC frame.
    pub rc_stale_ticks: &'a mut u32,
    /// Whether RC was valid before the current freshness transition.
    pub rc_link_was_valid: &'a mut bool,
    /// Arming state used to detect reset boundaries.
    pub control_was_armed: &'a mut bool,
}

/// Why a control step deliberately produced no actuator request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlInhibitReason {
    /// Safety state is disarmed.
    Disarmed,
    /// No new decoded IMU sequence was available.
    StaleImu,
    /// The IMU sample contained a non-finite physical value.
    InvalidImu,
    /// No healthy RC frame has been observed.
    RcUnavailable,
    /// The last healthy RC frame exceeded the 100 ms freshness policy.
    StaleRc,
    /// Safety attempted to arm before stationary gyro calibration completed.
    GyroBiasUncalibrated,
    /// Controller or mixer output was non-finite or outside the request range.
    InvalidMotorRequest,
}

/// Bounded result of one control step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FoxeerControlOutcome {
    /// No request may be published; the reason is explicit and bounded.
    Inhibited(ControlInhibitReason),
    /// A validated physical-order motor request is ready for publication.
    MotorRequest([f32; 4]),
}

/// Result returned by the bounded queue adapter supplied by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotorQueueOutcome {
    /// The authoritative queue accepted the command.
    Accepted,
    /// The bounded queue rejected the command because it was full.
    Full,
    /// This build deliberately has no actuator channel attached yet.
    Inhibited,
}

/// Complete fail-closed result of motor-request publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotorPublishOutcome {
    /// Queue insertion and actuator-task wake both succeeded.
    Published,
    /// No authoritative channel is attached in this inhibited build.
    Inhibited,
    /// A motor value was non-finite or outside `0..=2000`.
    InvalidValues,
    /// The bounded authoritative queue was full.
    QueueFull,
    /// The command was queued, but waking its safety-owned consumer failed.
    WakeRejected,
}

/// Why a queued motor command is not safe for an actuator consumer to accept.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotorCommandValidationError {
    /// The command timestamp exceeds the golden maximum age.
    Stale,
    /// At least one motor value is non-finite or outside `0..=2000`.
    InvalidValues,
}

/// Executes one bounded, allocation-free Foxeer control step.
pub fn run_foxeer_control_step(
    mut state: FoxeerControlState<'_>,
    input: FoxeerControlInput<'_>,
) -> FoxeerControlOutcome {
    update_rc_freshness(&mut state, input.rc, input.rc_observed);

    if input.armed != *state.control_was_armed {
        state.flight_controller.reset_control_state();
        state.imu_rate_filter.reset();
    }
    *state.control_was_armed = input.armed;

    let Some(imu) = input.imu else {
        return fail_closed(state, ControlInhibitReason::StaleImu);
    };
    if imu.sequence == *state.imu_last_sequence {
        return fail_closed(state, ControlInhibitReason::StaleImu);
    }
    *state.imu_last_sequence = imu.sequence;
    *state.imu_stale_ticks = 0;

    if !imu
        .specific_force_body
        .iter()
        .all(|value| value.is_finite())
    {
        return fail_closed(state, ControlInhibitReason::InvalidImu);
    }

    let controller_raw = state.gyro_axis_map.map_raw(imu.gyro_raw_body);
    let bias = state
        .gyro_bias_calibrator
        .update_if_fresh(input.armed, true, controller_raw);
    let rates = bias
        .corrected_raw
        .map(|value| value as f32 / FOXEER_GYRO_RAW_TO_DPS);
    if !rates.iter().all(|value| value.is_finite()) {
        return fail_closed(state, ControlInhibitReason::InvalidImu);
    }

    let filtered = state.imu_rate_filter.update(rates[0], rates[1], rates[2]);
    let filtered = [filtered.0, filtered.1, filtered.2];
    if !filtered.iter().all(|value| value.is_finite()) {
        return fail_closed(state, ControlInhibitReason::InvalidImu);
    }

    let body_rates = RATE_CONTROLLER_TO_BODY_MAP.map_f32(filtered);
    let gravity_body = imu.specific_force_body.map(|value| -value);
    let angles = state.imu_angle_integrator.update_with_accel(
        body_rates,
        gravity_body,
        CONTROL_LOOP_DT_SECONDS,
    );
    if !angles.iter().all(|value| value.is_finite()) {
        return fail_closed(state, ControlInhibitReason::InvalidImu);
    }

    if !input.armed {
        state.flight_controller.reset_control_state();
        apply_fixed_tuning(&mut state);
        return FoxeerControlOutcome::Inhibited(ControlInhibitReason::Disarmed);
    }
    if !state.gyro_bias_calibrator.ready() {
        return fail_closed(state, ControlInhibitReason::GyroBiasUncalibrated);
    }
    if !*state.rc_link_was_valid {
        let reason = if input.rc.has_valid_frame {
            ControlInhibitReason::StaleRc
        } else {
            ControlInhibitReason::RcUnavailable
        };
        return fail_closed(state, reason);
    }

    let rc = remap_rc_channels_with_profile(
        input.rc.channels[0],
        input.rc.channels[1],
        input.rc.channels[3],
        input.rc.channels[2],
        input.tuning.sanitized().rc_rates,
    );
    state
        .flight_controller
        .update_throttle_setpoint(rc.throttle as f32);
    state
        .flight_controller
        .update_attitude_rate_setpoint(rc.roll_dps, rc.pitch_dps, rc.yaw_dps);
    state
        .flight_controller
        .update_rate_measured(filtered[0], filtered[1], filtered[2]);
    state.flight_controller.update_motor_commands();

    let motors = remap_motor_outputs(
        state.flight_controller.get_logical_motor_commands(),
        FOXEER_LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
    );
    if !valid_motor_request(motors) {
        return fail_closed(state, ControlInhibitReason::InvalidMotorRequest);
    }

    FoxeerControlOutcome::MotorRequest(motors)
}

/// Validates, sequences, timestamps, queues, and wakes one motor request.
///
/// Sequence state advances only after the queue accepts the command, matching
/// the golden app. A rejected wake is still reported even though the accepted
/// command consumed that sequence number.
pub fn publish_motor_request<Send, Wake>(
    sequence: &mut u32,
    motors: [f32; 4],
    now_ms: u32,
    send: Send,
    wake: Wake,
) -> MotorPublishOutcome
where
    Send: FnOnce(MotorCmd) -> MotorQueueOutcome,
    Wake: FnOnce() -> bool,
{
    if !valid_motor_request(motors) {
        return MotorPublishOutcome::InvalidValues;
    }

    let next_sequence = sequence.wrapping_add(1);
    let command = MotorCmd {
        motors,
        seq: next_sequence,
        issued_at_ms: now_ms,
    };
    match send(command) {
        MotorQueueOutcome::Full => MotorPublishOutcome::QueueFull,
        MotorQueueOutcome::Inhibited => MotorPublishOutcome::Inhibited,
        MotorQueueOutcome::Accepted => {
            *sequence = next_sequence;
            if wake() {
                MotorPublishOutcome::Published
            } else {
                MotorPublishOutcome::WakeRejected
            }
        }
    }
}

/// Validates the command boundary used by the following actuator checkpoint.
pub fn validate_motor_command(
    command: &MotorCmd,
    now_ms: u32,
) -> Result<(), MotorCommandValidationError> {
    if !command.is_fresh(now_ms, MOTOR_CMD_MAX_AGE_MS) {
        return Err(MotorCommandValidationError::Stale);
    }
    if !valid_motor_request(command.motors) {
        return Err(MotorCommandValidationError::InvalidValues);
    }
    Ok(())
}

fn update_rc_freshness(
    state: &mut FoxeerControlState<'_>,
    snapshot: &RcInputSnapshot,
    observed: bool,
) {
    let healthy = observed
        && snapshot.has_valid_frame
        && !snapshot.frame_lost
        && !snapshot.failsafe
        && snapshot.valid_frames != *state.rc_last_valid_frames;
    if healthy {
        *state.rc_last_valid_frames = snapshot.valid_frames;
        *state.rc_stale_ticks = 0;
        *state.rc_link_was_valid = true;
        return;
    }

    *state.rc_stale_ticks = state.rc_stale_ticks.saturating_add(1);
    if !snapshot.has_valid_frame || *state.rc_stale_ticks > FOXEER_RC_FRESH_CONTROL_TICKS {
        *state.rc_link_was_valid = false;
    }
}

fn apply_fixed_tuning(state: &mut FoxeerControlState<'_>) {
    // A nonzero sequence may have been applied by the generated runtime
    // configuration owner. The fallback is boot-only and must not overwrite it.
    if *state.applied_tuning_seq != 0 {
        return;
    }
    let profile = TuningProfile::default_foxeer_f405_v2();
    state.flight_controller.apply_tuning_profile(profile);
    state
        .imu_rate_filter
        .set_alpha(profile.sanitized().imu_lpf_alpha);
    *state.applied_tuning_seq = FOXEER_TUNING_SEQUENCE;
}

fn fail_closed(
    state: FoxeerControlState<'_>,
    reason: ControlInhibitReason,
) -> FoxeerControlOutcome {
    *state.imu_stale_ticks = match reason {
        ControlInhibitReason::StaleImu => state.imu_stale_ticks.saturating_add(1),
        _ => *state.imu_stale_ticks,
    };
    state.flight_controller.reset_control_state();
    state.imu_rate_filter.reset();
    FoxeerControlOutcome::Inhibited(reason)
}

fn valid_motor_request(motors: [f32; 4]) -> bool {
    motors
        .iter()
        .all(|motor| motor.is_finite() && (0.0..=ESC_MAX_THROTTLE).contains(motor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drone_toolbox::{
        FlightControllerConfig, IMU_GYRO_LPF_ALPHA, RATE_CONTROLLER_OUTPUT_LIMIT, RateController,
    };
    use ferrowasp_core::frames::BODY_RATE_TO_RATE_CONTROLLER_MAP;

    struct Fixture {
        flight_controller: FlightController,
        imu_rate_filter: ImuRateLowPassFilter,
        imu_angle_integrator: GyroAngleIntegrator,
        gyro_axis_map: FrameRotation,
        gyro_bias_calibrator: GyroBiasCalibrator,
        imu_last_sequence: u32,
        imu_stale_ticks: u32,
        applied_tuning_seq: u32,
        rc_last_valid_frames: u32,
        rc_stale_ticks: u32,
        rc_link_was_valid: bool,
        control_was_armed: bool,
        rc: RcInputSnapshot,
    }

    impl Fixture {
        fn new(required_bias_samples: u32) -> Self {
            let profile = TuningProfile::default_foxeer_f405_v2();
            Self {
                flight_controller: FlightController::new(
                    FlightControllerConfig::default(),
                    RateController::new(profile.rate_gains, RATE_CONTROLLER_OUTPUT_LIMIT),
                ),
                imu_rate_filter: ImuRateLowPassFilter::new(IMU_GYRO_LPF_ALPHA),
                imu_angle_integrator: GyroAngleIntegrator::new(),
                gyro_axis_map: BODY_RATE_TO_RATE_CONTROLLER_MAP,
                gyro_bias_calibrator: GyroBiasCalibrator::new(required_bias_samples, 1_000),
                imu_last_sequence: 0,
                imu_stale_ticks: 0,
                applied_tuning_seq: 0,
                rc_last_valid_frames: 0,
                rc_stale_ticks: 0,
                rc_link_was_valid: false,
                control_was_armed: false,
                rc: RcInputSnapshot::new(),
            }
        }

        fn state(&mut self) -> FoxeerControlState<'_> {
            FoxeerControlState {
                flight_controller: &mut self.flight_controller,
                imu_rate_filter: &mut self.imu_rate_filter,
                imu_angle_integrator: &mut self.imu_angle_integrator,
                gyro_axis_map: &mut self.gyro_axis_map,
                gyro_bias_calibrator: &mut self.gyro_bias_calibrator,
                imu_last_sequence: &mut self.imu_last_sequence,
                imu_stale_ticks: &mut self.imu_stale_ticks,
                applied_tuning_seq: &mut self.applied_tuning_seq,
                rc_last_valid_frames: &mut self.rc_last_valid_frames,
                rc_stale_ticks: &mut self.rc_stale_ticks,
                rc_link_was_valid: &mut self.rc_link_was_valid,
                control_was_armed: &mut self.control_was_armed,
            }
        }

        fn step(
            &mut self,
            imu: Option<ImuControlSample>,
            rc_observed: bool,
            armed: bool,
        ) -> FoxeerControlOutcome {
            let rc = self.rc;
            run_foxeer_control_step(
                self.state(),
                FoxeerControlInput {
                    rc: &rc,
                    rc_observed,
                    imu,
                    armed,
                    tuning: TuningProfile::default_foxeer_f405_v2(),
                },
            )
        }
    }

    fn imu(sequence: u32, gyro_raw_body: [i16; 3]) -> ImuControlSample {
        ImuControlSample {
            specific_force_body: [0.0, 0.0, -1.0],
            gyro_raw_body,
            sequence,
        }
    }

    #[test]
    fn representative_command_matches_the_golden_reusable_math_and_motor_order() {
        let mut fixture = Fixture::new(1);
        fixture.rc.record_frame(
            [
                1_392, 792, 1_192, 1_592, 992, 992, 992, 992, 992, 992, 992, 992, 992, 992, 992,
                992,
            ],
            false,
            false,
            false,
            false,
        );
        assert_eq!(
            fixture.step(Some(imu(1, [10, 20, 30])), true, false),
            FoxeerControlOutcome::Inhibited(ControlInhibitReason::Disarmed)
        );

        let measured_raw = [174, -144, 194];
        let outcome = fixture.step(Some(imu(2, measured_raw)), true, true);

        let rc = remap_rc_channels_with_profile(
            fixture.rc.channels[0],
            fixture.rc.channels[1],
            fixture.rc.channels[3],
            fixture.rc.channels[2],
            TuningProfile::default_foxeer_f405_v2().rc_rates,
        );
        let corrected = BODY_RATE_TO_RATE_CONTROLLER_MAP
            .map_raw(measured_raw)
            .into_iter()
            .zip([10, -20, 30])
            .map(|(sample, bias)| (sample - bias) as f32 / FOXEER_GYRO_RAW_TO_DPS)
            .collect::<heapless::Vec<f32, 3>>();
        let mut reference = FlightController::new(
            FlightControllerConfig::default(),
            RateController::new(
                TuningProfile::default_foxeer_f405_v2().rate_gains,
                RATE_CONTROLLER_OUTPUT_LIMIT,
            ),
        );
        reference.update_throttle_setpoint(rc.throttle as f32);
        reference.update_attitude_rate_setpoint(rc.roll_dps, rc.pitch_dps, rc.yaw_dps);
        reference.update_rate_measured(corrected[0], corrected[1], corrected[2]);
        reference.update_motor_commands();
        let expected = reference.get_logical_motor_commands();

        let FoxeerControlOutcome::MotorRequest(actual) = outcome else {
            panic!("expected a motor request, got {outcome:?}");
        };
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 0.001);
        }
    }

    #[test]
    fn stale_imu_and_rc_inputs_fail_closed() {
        let mut fixture = Fixture::new(1);
        fixture
            .rc
            .record_frame([992; 16], false, false, false, false);
        fixture.step(Some(imu(1, [0; 3])), true, false);
        fixture.step(Some(imu(2, [0; 3])), true, true);

        assert_eq!(
            fixture.step(None, false, true),
            FoxeerControlOutcome::Inhibited(ControlInhibitReason::StaleImu)
        );
        assert_eq!(fixture.imu_stale_ticks, 1);

        for sequence in 3..=43 {
            let _ = fixture.step(Some(imu(sequence, [0; 3])), false, true);
        }
        assert_eq!(
            fixture.step(Some(imu(44, [0; 3])), false, true),
            FoxeerControlOutcome::Inhibited(ControlInhibitReason::StaleRc)
        );
        assert!(!fixture.rc_link_was_valid);
    }

    #[test]
    fn non_finite_imu_values_fail_closed() {
        let mut fixture = Fixture::new(1);
        fixture
            .rc
            .record_frame([992; 16], false, false, false, false);

        assert_eq!(
            fixture.step(
                Some(ImuControlSample {
                    specific_force_body: [f32::NAN, 0.0, -1.0],
                    gyro_raw_body: [0; 3],
                    sequence: 1,
                }),
                true,
                true,
            ),
            FoxeerControlOutcome::Inhibited(ControlInhibitReason::InvalidImu)
        );
    }

    #[test]
    fn publication_is_sequenced_timestamped_and_fail_closed() {
        let mut sequence = 9;
        let mut captured = None;
        assert_eq!(
            publish_motor_request(
                &mut sequence,
                [100.0, 200.0, 300.0, 400.0],
                123,
                |command| {
                    captured = Some(command);
                    MotorQueueOutcome::Accepted
                },
                || true,
            ),
            MotorPublishOutcome::Published
        );
        let command = captured.unwrap();
        assert_eq!(command.seq, 10);
        assert_eq!(command.issued_at_ms, 123);
        assert_eq!(command.motors, [100.0, 200.0, 300.0, 400.0]);
        assert_eq!(sequence, 10);

        assert_eq!(
            publish_motor_request(
                &mut sequence,
                [0.0; 4],
                124,
                |_| MotorQueueOutcome::Full,
                || true,
            ),
            MotorPublishOutcome::QueueFull
        );
        assert_eq!(sequence, 10);
        assert_eq!(
            publish_motor_request(
                &mut sequence,
                [0.0; 4],
                125,
                |_| MotorQueueOutcome::Accepted,
                || false,
            ),
            MotorPublishOutcome::WakeRejected
        );
        assert_eq!(sequence, 11);
        assert_eq!(
            publish_motor_request(
                &mut sequence,
                [f32::NAN, 0.0, 0.0, 0.0],
                126,
                |_| MotorQueueOutcome::Accepted,
                || true,
            ),
            MotorPublishOutcome::InvalidValues
        );
        assert_eq!(sequence, 11);
        assert_eq!(
            publish_motor_request(
                &mut sequence,
                [0.0; 4],
                127,
                |_| MotorQueueOutcome::Inhibited,
                || true,
            ),
            MotorPublishOutcome::Inhibited
        );
        assert_eq!(sequence, 11);
    }

    #[test]
    fn consumer_boundary_rejects_stale_and_invalid_commands() {
        let valid = MotorCmd {
            motors: [100.0, 200.0, 300.0, 400.0],
            seq: 7,
            issued_at_ms: 1_000,
        };
        assert_eq!(validate_motor_command(&valid, 1_020), Ok(()));
        assert_eq!(
            validate_motor_command(&valid, 1_021),
            Err(MotorCommandValidationError::Stale)
        );

        let invalid = MotorCmd {
            motors: [f32::INFINITY, 0.0, 0.0, 0.0],
            ..valid
        };
        assert_eq!(
            validate_motor_command(&invalid, 1_000),
            Err(MotorCommandValidationError::InvalidValues)
        );
    }

    #[test]
    fn filter_reset_discards_pre_arm_history() {
        let mut filter = ImuRateLowPassFilter::new(0.55);
        assert_eq!(filter.update(10.0, 20.0, 30.0), (10.0, 20.0, 30.0));
        let _ = filter.update(20.0, 30.0, 40.0);
        filter.reset();
        assert_eq!(filter.update(-1.0, -2.0, -3.0), (-1.0, -2.0, -3.0));
    }
}
