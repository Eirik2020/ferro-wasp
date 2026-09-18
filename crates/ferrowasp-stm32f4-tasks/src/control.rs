//! The control loop: the rate controller, run from the control scheduler's
//! timer interrupt. It turns the latest IMU sample and RC rates into motor
//! outputs and publishes them for `actuator_output`; it never commands a
//! motor itself.

use crate::prelude::*;
use crate::snapshots::*;

/// The time a motor command is stamped with: now, except in the
/// `bench_motor_cmd_stale_rejection` fault-injection image, where the first
/// command is stamped stale so the actuator's rejection can be observed.
pub fn motor_command_timestamp(now_ms: u32, sequence: u32) -> u32 {
    #[cfg(feature = "bench_motor_cmd_stale_rejection")]
    if sequence == 1 {
        return now_ms.wrapping_sub(safety::MOTOR_CMD_MAX_AGE_MS + 1);
    }

    let _ = sequence;
    now_ms
}

/// One control tick. Priority and interrupt are the firmware's.
///
/// The IMU's axis profile, the gyro's raw-to-degrees-per-second scale and
/// the logical-to-physical motor map are the board's, so they are
/// configuration.
// The body moved verbatim from the Foxeer app, where clippy never saw it
// inside `#[rtic::app]`; a bench-image `return` is kept as written so the
// control path moves without a logic change.
#[allow(clippy::needless_return)]
#[ferroforge::task(
    bounds = [control_loop_scheduler: stm32_scheduler::TimerTick],
    local = [
        control_loop_cnt: u32,
        samples_per_control_loop: u32,
        flight_controller: dt::FlightController,
        control_loop_scheduler,
        imu_rate_filter: dt::ImuRateLowPassFilter,
        imu_angle_integrator: dt::GyroAngleIntegrator,
        gyro_axis_map: dt::GyroAxisMap,
        gyro_bias_calibrator: dt::GyroBiasCalibrator,
        imu_last_sequence: u32,
        imu_stale_ticks: u32,
        applied_tuning_seq: u32,
        rc_rates_reader: signals::RcRatesReader,
        control_throttle_reader: signals::RcThrottleReader,
        control_safety_arm_reader: signals::SafetyArmReader,
        control_rc_link_reader: signals::RcLinkReader,
        control_arm_permit_reader: signals::ActuatorArmPermitReader,
        motor_cmd_writer: signals::MotorCmdWriter,
        motor_cmd_seq: u32,
        flash_record_producer: flash_task::RecordProducer,
        rc_link_was_valid: bool = false,
    ],
    shared = [
        imu_data: imu::ImuData,
        imu_angles: [f32; 3],
        imu_rates: [f32; 3],
        tuning_profile: dt::TuningProfile,
        tuning_request_seq: u32,
    ],
    spawn = [
        actuator_output(cmd: safety::ActuatorCmd),
        safety_master(event: safety::SafetyEvent),
    ],
    config = [
        imu_control_axis_profile: ferrowasp_core::frames::ImuControlAxisProfile,
        imu_gyro_raw_to_dps: f32,
        logical_to_physical_motor_output: [usize; 4],
    ],
    monotonic = Mono,
)]
pub fn control_loop(mut cx: control_loop::Context) {
    //info!("PING!");
    // Alias
    let fc = cx.local.flight_controller;
    stm32_scheduler::acknowledge_control_tick(cx.local.control_loop_scheduler);
    let cnt = cx.local.control_loop_cnt;
    let samples_per_control_loop = cx.local.samples_per_control_loop;
    let mut publish_motor_command = |motors, wake| {
        let outcome = actuator_task::publish_motor_command(
            cx.local.motor_cmd_writer,
            cx.local.motor_cmd_seq,
            motors,
            Mono::now().duration_since_epoch().to_millis(),
            wake,
            motor_command_timestamp,
            |command| cx.spawn.actuator_output(command).is_ok(),
        );
        match outcome {
            actuator_task::PublishOutcome::Published => {}
            actuator_task::PublishOutcome::QueueFull => {
                warn!("Motor command queue full; requesting disarm");
                if cx
                    .spawn
                    .safety_master(safety::SafetyEvent::DisarmRequested)
                    .is_err()
                {
                    warn!("Failed to report motor command queue overflow");
                }
            }
            actuator_task::PublishOutcome::WakeRejected => {
                warn!("Actuator command wake rejected; requesting disarm");
                if cx
                    .spawn
                    .safety_master(safety::SafetyEvent::DisarmRequested)
                    .is_err()
                {
                    warn!("Failed to report rejected actuator command wake");
                }
            }
        }
    };
    let mut enqueue_flash_record = |sample| {
        let outcome = flash_task::enqueue_rate_record(
            cx.local.flash_record_producer,
            sample,
            Mono::now().duration_since_epoch().to_micros(),
            FLASH_LOG_RATE_DIVISOR.load(Ordering::Relaxed),
        );
        if outcome == flash_task::RecordEnqueueOutcome::Full {
            FLASH_RECORDS_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    };

    // Incremet Counter
    *cnt += 1;
    CONTROL_ISR_SEQ.fetch_add(1, Ordering::Relaxed);

    if !IMU_TRANSPORT_READY.load(Ordering::Relaxed) {
        *cnt = 0;
        IMU_STALE.store(true, Ordering::Release);
        return;
    }

    // ---- CONTROL LOOP ----
    // Check if required samples per control loop is reached
    if cnt >= samples_per_control_loop {
        *cnt = 0; // Reset sampling counter

        let sensor_accel = cx.shared.imu_data.lock(|imu| imu.acc);
        let drone_gravity =
            CONFIG::IMU_CONTROL_AXIS_PROFILE.sensor_accel_to_drone_gravity(sensor_accel);
        let gyro_raw = [
            IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed) as i16,
            IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed) as i16,
            IMU_LATEST_YAW_RAW.load(Ordering::Relaxed) as i16,
        ];
        let imu_sequence = IMU_LATEST_SEQ.load(Ordering::Relaxed);

        let imu_fresh = imu::classify_sample_freshness(*cx.local.imu_last_sequence, imu_sequence)
            == imu::SampleFreshness::Fresh;
        *cx.local.imu_last_sequence = imu_sequence;
        IMU_STALE.store(!imu_fresh, Ordering::Release);

        let control_armed = cx.local.control_safety_arm_reader.read();
        let gyro_bias_update = cx.local.gyro_bias_calibrator.update_if_fresh(
            control_armed,
            imu_fresh,
            cx.local.gyro_axis_map.map_raw(gyro_raw),
        );
        if gyro_bias_update.newly_calibrated {
            IMU_BIAS_CALIBRATED.store(true, Ordering::Release);
            info!(
                "Gyro bias calibrated raw [{}, {}, {}]",
                gyro_bias_update.bias_raw[0],
                gyro_bias_update.bias_raw[1],
                gyro_bias_update.bias_raw[2]
            );
        }
        let control_gyro_raw = gyro_bias_update.corrected_raw;
        let imu_roll_raw = control_gyro_raw[0] as f32 / CONFIG::IMU_GYRO_RAW_TO_DPS;
        let imu_pitch_raw = control_gyro_raw[1] as f32 / CONFIG::IMU_GYRO_RAW_TO_DPS;
        let imu_yaw_raw = control_gyro_raw[2] as f32 / CONFIG::IMU_GYRO_RAW_TO_DPS;
        CONTROL_ROLL_RAW.store(control_gyro_raw[0], Ordering::Relaxed);
        CONTROL_PITCH_RAW.store(control_gyro_raw[1], Ordering::Relaxed);
        CONTROL_YAW_RAW.store(control_gyro_raw[2], Ordering::Relaxed);
        let (imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered) = cx
            .local
            .imu_rate_filter
            .update(imu_roll_raw, imu_pitch_raw, imu_yaw_raw);
        // The rate controller retains FCU3's historical nose-down-positive
        // pitch convention. Translate its filtered rates back to physical
        // body rates for attitude estimation and external reporting.
        let body_rates = ferrowasp_core::frames::RATE_CONTROLLER_TO_BODY_MAP.map_f32([
            imu_roll_filtered,
            imu_pitch_filtered,
            imu_yaw_filtered,
        ]);
        CONTROL_ROLL_DPS10.store((imu_roll_filtered * 10.0) as i32, Ordering::Relaxed);
        CONTROL_PITCH_DPS10.store((imu_pitch_filtered * 10.0) as i32, Ordering::Relaxed);
        CONTROL_YAW_DPS10.store((imu_yaw_filtered * 10.0) as i32, Ordering::Relaxed);
        CONTROL_RATE_SEQ.fetch_add(1, Ordering::Relaxed);
        cx.shared.imu_rates.lock(|rates| {
            *rates = body_rates;
        });
        if !control_armed {
            // Keep Foxeer aligned with the FCU3 golden app: no
            // PID/filter/setpoint/mixer state crosses an arming boundary.
            fc.reset_control_state();

            let pending_seq = cx.shared.tuning_request_seq.lock(|seq| *seq);
            if pending_seq != *cx.local.applied_tuning_seq {
                let profile = cx.shared.tuning_profile.lock(|profile| *profile);
                fc.apply_tuning_profile(profile);
                cx.local
                    .imu_rate_filter
                    .set_alpha(profile.sanitized().imu_lpf_alpha);
                *cx.local.applied_tuning_seq = pending_seq;
                info!("Applied disarmed OSD tuning profile {}", pending_seq);
            }

            #[cfg(feature = "blackbox_defmt")]
            {
                let rc_raw = cx.local.rc_rates_reader.read();
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    false,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    cx.local.control_throttle_reader.read() as f32,
                    [0.0; 4],
                );
            }
        }

        if !imu_fresh {
            *cx.local.imu_stale_ticks = cx.local.imu_stale_ticks.saturating_add(1);

            if *cx.local.imu_stale_ticks == 1 || (*cx.local.imu_stale_ticks).is_multiple_of(100) {
                warn!(
                    "IMU stale in control loop: seq {}, stale ticks {}",
                    imu_sequence, *cx.local.imu_stale_ticks
                );
            }

            if cx.local.control_safety_arm_reader.read() {
                let _ = cx.spawn.actuator_output(safety::ActuatorCmd::Disarm);
                let _ = cx.spawn.safety_master(safety::SafetyEvent::DisarmRequested);
            }

            return;
        }

        *cx.local.imu_stale_ticks = 0;

        let now_us = Mono::now().duration_since_epoch().to_micros();
        let rc_link = cx.local.control_rc_link_reader.status(now_us);
        if rc_link.valid {
            *cx.local.rc_link_was_valid = true;
        } else {
            if rc_link.timed_out
                && *cx.local.rc_link_was_valid
                && cx
                    .spawn
                    .safety_master(safety::SafetyEvent::RcLinkInvalid(
                        safety::RcLinkInvalidation::Timeout,
                    ))
                    .is_ok()
            {
                *cx.local.rc_link_was_valid = false;
            }

            if control_armed || cx.local.control_arm_permit_reader.read() {
                return;
            }
        }

        let imu_angles = cx.local.imu_angle_integrator.update_with_accel(
            body_rates,
            drone_gravity,
            dt::CONTROL_LOOP_DT_SECONDS,
        );
        cx.shared.imu_angles.lock(|angles| {
            *angles = imu_angles;
        });

        if control_armed {
            // Read rc_inputs
            let rc_raw = cx.local.rc_rates_reader.read();
            #[cfg(all(
                not(feature = "blackbox_defmt"),
                any(
                    feature = "bench_equal_motors",
                    feature = "bench_motor1_only",
                    feature = "bench_motor2_only",
                    feature = "bench_motor3_only",
                    feature = "bench_motor4_only",
                    feature = "bench_logical_motor1_only",
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                )
            ))]
            let _ = rc_raw;

            #[cfg(feature = "bench_motor1_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = [bench_throttle, 0.0, 0.0, 0.0];

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(feature = "bench_motor2_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = [0.0, bench_throttle, 0.0, 0.0];

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(feature = "bench_motor3_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = [0.0, 0.0, bench_throttle, 0.0];

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(feature = "bench_motor4_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = [0.0, 0.0, 0.0, bench_throttle];

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(feature = "bench_logical_motor1_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = remap_motor_outputs(
                    [bench_throttle, 0.0, 0.0, 0.0],
                    CONFIG::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                );

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(feature = "bench_logical_motor2_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = remap_motor_outputs(
                    [0.0, bench_throttle, 0.0, 0.0],
                    CONFIG::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                );

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(feature = "bench_logical_motor3_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = remap_motor_outputs(
                    [0.0, 0.0, bench_throttle, 0.0],
                    CONFIG::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                );

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(feature = "bench_logical_motor4_only")]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = remap_motor_outputs(
                    [0.0, 0.0, 0.0, bench_throttle],
                    CONFIG::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                );

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                {
                    publish_motor_command(
                        motor_commands,
                        safety::ActuatorCmd::ApplyBenchSelectedMotor,
                    );
                }

                return;
            }

            #[cfg(all(
                feature = "bench_equal_motors",
                not(any(
                    feature = "bench_motor1_only",
                    feature = "bench_motor2_only",
                    feature = "bench_motor3_only",
                    feature = "bench_motor4_only",
                    feature = "bench_logical_motor1_only",
                    feature = "bench_logical_motor2_only",
                    feature = "bench_logical_motor3_only",
                    feature = "bench_logical_motor4_only"
                ))
            ))]
            {
                let requested_throttle = cx.local.control_throttle_reader.read() as f32;
                let bench_throttle = requested_throttle.min(BENCH_EQUAL_MOTOR_MAX_THROTTLE);
                let motor_commands = [bench_throttle; 4];

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                    imu_sequence,
                    control_armed,
                    imu_fresh,
                    [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                    [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered],
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                    [0.0; 3],
                    bench_throttle,
                    motor_commands,
                );
                enqueue_flash_record(dt::CompactRateBlackboxSample::from_fields(
                    dt::CompactRateBlackboxFields {
                        seq: CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_seq: imu_sequence,
                        armed: control_armed,
                        imu_fresh,
                        raw_gyro_dps: [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        filtered_gyro_dps: [
                            imu_roll_filtered,
                            imu_pitch_filtered,
                            imu_yaw_filtered,
                        ],
                        command_dps: [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32],
                        pid: [0.0; 3],
                        throttle: bench_throttle,
                        motors: motor_commands,
                    },
                ));
                {
                    publish_motor_command(motor_commands, safety::ActuatorCmd::ApplyLatestThrottle);
                }

                return;
            }

            #[cfg(not(any(
                feature = "bench_equal_motors",
                feature = "bench_motor1_only",
                feature = "bench_motor2_only",
                feature = "bench_motor3_only",
                feature = "bench_motor4_only",
                feature = "bench_logical_motor1_only",
                feature = "bench_logical_motor2_only",
                feature = "bench_logical_motor3_only",
                feature = "bench_logical_motor4_only"
            )))]
            {
                // In your control loop, at fixed rate:
                fc.update_throttle_setpoint(cx.local.control_throttle_reader.read() as f32);
                fc.update_attitude_rate_setpoint(
                    rc_raw.roll as f32,
                    rc_raw.pitch as f32,
                    rc_raw.yaw as f32,
                ); // deg/s or rad/s, but be consistent
                fc.update_rate_measured(imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered); // filtered gyro rates
                fc.update_motor_commands();
                let motor_commands = remap_motor_outputs(
                    fc.get_logical_motor_commands(),
                    CONFIG::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                );

                {
                    let mut sample = fc.blackbox_sample();
                    sample.motors = remap_motor_outputs(
                        fc.get_logical_motor_commands(),
                        CONFIG::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT,
                    );
                    let compact = dt::CompactRateBlackboxSample::from_rate_sample(
                        CONTROL_RATE_SEQ.load(Ordering::Relaxed),
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        sample,
                    );
                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_rate_blackbox(compact);
                    enqueue_flash_record(compact);
                }

                // Apply throttle
                {
                    if control_armed {
                        publish_motor_command(
                            motor_commands,
                            safety::ActuatorCmd::ApplyLatestThrottle,
                        );
                    }
                }
            }
        }
    }
}
