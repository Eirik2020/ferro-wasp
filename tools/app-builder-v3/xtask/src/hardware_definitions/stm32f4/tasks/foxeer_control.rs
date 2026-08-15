use crate::hardware_definitions::stm32f4::periodic_control::{
    ControlScheduler, acknowledge_control_tick,
};
use crate::rtic::task::Mono;
use ferrowasp_core::{
    frames::FrameRotation,
    safety::{ActuatorCmd, MotorCmd, PreArmHealth, PreArmHealthReport},
    safety_channel::{SafetyConsumer, SafetyProducer},
};
use ferrowasp_drivers::mpu6500::ImuData;
use ferrowasp_io_core::serial::RcInputSnapshot;
use ferrowasp_tasks::drone_toolbox as dt;
use ferrowasp_tasks::{
    flash_storage::{RecordEnqueueOutcome, RecordProducer},
    service_telemetry::{FlightServiceTelemetry, StorageStatus},
};

crate::reusable_task! {
    contract {
        local {
            /// Timer counter that owns and acknowledges the update interrupt.
            scheduler: ControlScheduler,
            /// Scheduler phase accumulated toward the next control step.
            phase: u32,
            /// Exclusive stream of healthy, complete RC snapshots.
            sbus: SafetyConsumer<'static, RcInputSnapshot>,
            /// Exclusive stream of decoded body-frame IMU samples.
            imu: SafetyConsumer<'static, ImuData>,
            /// Exclusive command producer for the safety-owned actuator endpoint.
            motor_commands: SafetyProducer<'static, MotorCmd>,
            /// Observation-only pre-arm health producer consumed by safety.
            prearm_health: SafetyProducer<'static, PreArmHealthReport>,
            /// Number of completed 400 Hz control steps.
            control_loop_cnt: u32,
            /// Application-owned copy of the validated scheduler ratio.
            samples_per_control_loop: u32,
            /// Golden rate controller and Quad-X mixer state.
            flight_controller: dt::FlightController,
            /// Three-axis gyro low-pass filter state.
            imu_rate_filter: dt::ImuRateLowPassFilter,
            /// Complementary attitude estimator state.
            imu_angle_integrator: dt::GyroAngleIntegrator,
            /// Physical-body to legacy-controller compatibility map.
            gyro_axis_map: FrameRotation,
            /// Stationary startup gyro bias calibration state.
            gyro_bias_calibrator: dt::GyroBiasCalibrator,
            /// Last consumed IMU sequence.
            imu_last_sequence: u32,
            /// Consecutive stale-IMU control steps.
            imu_stale_ticks: u32,
            /// Last applied fixed tuning generation.
            applied_tuning_seq: u32,
            /// Sequence assigned to the next accepted motor command.
            motor_cmd_seq: u32,
            /// Previous RC freshness state.
            rc_link_was_valid: bool,
            /// Latest complete healthy RC snapshot.
            latest_rc_input: RcInputSnapshot,
            /// Last observed healthy RC frame count.
            rc_last_valid_frames: u32,
            /// Consecutive control steps without a new RC frame.
            rc_stale_ticks: u32,
            /// Previous safety-owned arming state at the reset boundary.
            control_was_armed: bool,
            /// Bounded blackbox record producer consumed by the SPI2 owner.
            flash_records: RecordProducer,
        }
        shared {
            /// Observation-only telemetry published to MSP/USB/heartbeat.
            telemetry: FlightServiceTelemetry,
            /// Validated disarmed runtime tuning profile.
            tuning: dt::TuningProfile,
            /// Runtime tuning publication sequence.
            tuning_seq: u32,
            /// Bounded blackbox logging divisor.
            log_divisor: u32,
            /// Observation-only storage health and queue-loss count.
            storage: StorageStatus,
        }
        config {
            /// Exact number of scheduler ticks in one control period.
            ticks_per_control: u32,
        }
        spawns {
            /// Wakes the safety-owned actuator consumer after queue publication.
            actuator_wake(request: ferrowasp_core::safety::ActuatorCmd),
        }
    }

    /// Runs the bounded Foxeer control calculation from the TIM4 interrupt.
    ///
    /// This checkpoint has no safety-master arm source. Its priority-15
    /// consumer drains and validates requests but cannot command motor hardware.
    pub fn foxeer_control(mut cx: foxeer_control::Context<'_>) {
        acknowledge_control_tick(cx.local.scheduler);
        *cx.local.phase = cx.local.phase.wrapping_add(1);
        if *cx.local.phase < cx.config.ticks_per_control {
            return;
        }
        *cx.local.phase = 0;

        if *cx.local.samples_per_control_loop != cx.config.ticks_per_control {
            cx.local.flight_controller.reset_control_state();
            cx.local.imu_rate_filter.reset();
            return;
        }
        *cx.local.control_loop_cnt = cx.local.control_loop_cnt.wrapping_add(1);

        let tuning = cx.shared.tuning.lock(|profile| *profile).sanitized();
        let tuning_seq = cx.shared.tuning_seq.lock(|sequence| *sequence);
        if tuning_seq != 0 && tuning_seq != *cx.local.applied_tuning_seq {
            cx.local.flight_controller.apply_tuning_profile(tuning);
            cx.local.imu_rate_filter.set_alpha(tuning.imu_lpf_alpha);
            *cx.local.applied_tuning_seq = tuning_seq;
        }

        let mut rc_observed = false;
        while let Some(snapshot) = cx.local.sbus.try_receive() {
            if snapshot.has_valid_frame && !snapshot.frame_lost && !snapshot.failsafe {
                *cx.local.latest_rc_input = snapshot;
                rc_observed = true;
            } else {
                *cx.local.rc_link_was_valid = false;
            }
        }

        let mut newest_imu = None;
        while let Some(sample) = cx.local.imu.try_receive() {
            newest_imu = Some(ferrowasp_tasks::foxeer_control::ImuControlSample {
                specific_force_body: sample.acc,
                gyro_raw_body: sample.gyro_raw,
                sequence: sample.sequence,
            });
        }
        let previous_imu_sequence = *cx.local.imu_last_sequence;
        let imu_fresh = newest_imu.is_some_and(|sample| {
            sample.sequence != previous_imu_sequence
                && sample
                    .specific_force_body
                    .iter()
                    .all(|value| value.is_finite())
        });
        let raw_gyro_dps = newest_imu.map_or([0.0; 3], |sample| {
            cx.local
                .gyro_axis_map
                .map_i32(sample.gyro_raw_body.map(i32::from))
                .map(|value| value as f32 / ferrowasp_tasks::foxeer_control::FOXEER_GYRO_RAW_TO_DPS)
        });

        // Arming remains hard-inhibited until the safety-master checkpoint.
        let outcome = ferrowasp_tasks::foxeer_control::run_foxeer_control_step(
            ferrowasp_tasks::foxeer_control::FoxeerControlState {
                flight_controller: cx.local.flight_controller,
                imu_rate_filter: cx.local.imu_rate_filter,
                imu_angle_integrator: cx.local.imu_angle_integrator,
                gyro_axis_map: cx.local.gyro_axis_map,
                gyro_bias_calibrator: cx.local.gyro_bias_calibrator,
                imu_last_sequence: cx.local.imu_last_sequence,
                imu_stale_ticks: cx.local.imu_stale_ticks,
                applied_tuning_seq: cx.local.applied_tuning_seq,
                rc_last_valid_frames: cx.local.rc_last_valid_frames,
                rc_stale_ticks: cx.local.rc_stale_ticks,
                rc_link_was_valid: cx.local.rc_link_was_valid,
                control_was_armed: cx.local.control_was_armed,
            },
            ferrowasp_tasks::foxeer_control::FoxeerControlInput {
                rc: cx.local.latest_rc_input,
                rc_observed,
                imu: newest_imu,
                armed: false,
                tuning,
            },
        );

        let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
        let health = PreArmHealthReport {
            health: PreArmHealth {
                imu_ready: *cx.local.imu_last_sequence != 0,
                imu_bias_calibrated: cx.local.gyro_bias_calibrator.ready(),
                imu_fresh,
            },
            sequence: *cx.local.control_loop_cnt,
            observed_at_us: now_us,
        };
        if cx.local.prearm_health.try_send(health).is_err() {
            defmt::warn!("control-to-safety health channel full; evidence rejected");
        }

        cx.shared.telemetry.lock(|telemetry| {
            telemetry.rates_dps = cx.local.imu_rate_filter.values();
            telemetry.angles_deg = cx.local.imu_angle_integrator.angles();
            telemetry.imu_sequence = *cx.local.imu_last_sequence;
            telemetry.control_sequence = *cx.local.control_loop_cnt;
            telemetry.imu_stale = !imu_fresh;
        });

        let compact = dt::CompactRateBlackboxSample::from_rate_sample(
            *cx.local.control_loop_cnt,
            *cx.local.imu_last_sequence,
            false,
            imu_fresh,
            raw_gyro_dps,
            cx.local.flight_controller.blackbox_sample(),
        );
        let log_divisor = cx.shared.log_divisor.lock(|divisor| (*divisor).clamp(1, 16));
        if matches!(
            ferrowasp_tasks::flash_storage::enqueue_rate_record(
                cx.local.flash_records,
                compact,
                now_us,
                log_divisor,
            ),
            RecordEnqueueOutcome::Full
        ) {
            cx.shared.storage.lock(|status| {
                status.dropped_records = status.dropped_records.saturating_add(1)
            });
        }

        if let ferrowasp_tasks::foxeer_control::FoxeerControlOutcome::MotorRequest(motors) = outcome
        {
            let publication = ferrowasp_tasks::foxeer_control::publish_motor_request(
                cx.local.motor_cmd_seq,
                motors,
                now_us / 1_000,
                |command| match cx.local.motor_commands.try_send(command) {
                    Ok(()) => ferrowasp_tasks::foxeer_control::MotorQueueOutcome::Accepted,
                    Err(_) => ferrowasp_tasks::foxeer_control::MotorQueueOutcome::Full,
                },
                || actuator_wake::spawn(ActuatorCmd::ApplyLatestThrottle).is_ok(),
            );
            match publication {
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::Published => {}
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::Inhibited => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                }
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::InvalidValues => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                    defmt::warn!("invalid motor request; control failed closed");
                }
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::QueueFull => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                    defmt::warn!("motor command queue full; control failed closed");
                }
                ferrowasp_tasks::foxeer_control::MotorPublishOutcome::WakeRejected => {
                    cx.local.flight_controller.reset_control_state();
                    cx.local.imu_rate_filter.reset();
                    defmt::warn!("actuator wake rejected; control failed closed");
                }
            }
        }
    }
}
