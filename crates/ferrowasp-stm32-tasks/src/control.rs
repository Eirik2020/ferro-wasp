//! The control loop: the rate controller, run from the control scheduler's
//! timer interrupt. It turns the latest IMU sample and RC rates into motor
//! outputs and publishes them for `actuator_output`; it never commands a
//! motor itself.

use crate::prelude::*;
use crate::snapshots::*;

/// The time a motor command is stamped with: now, except in the
/// `bench_motor_cmd_stale_rejection` fault-injection image, where the first
/// command is stamped stale so the actuator's rejection can be observed.
pub fn motor_command_timestamp(now_ms: u64, sequence: u32) -> u64 {
    #[cfg(feature = "bench_motor_cmd_stale_rejection")]
    if sequence == 1 {
        return now_ms.wrapping_sub(u64::from(safety::MOTOR_CMD_MAX_AGE_MS) + 1);
    }

    let _ = sequence;
    now_ms
}

/// The control tick in progress, sent to the observer when the loop returns
/// by whichever path; the body fills it as the tick learns each value. The
/// link never blocks: a full one refuses the tick and counts it.
struct TickReport<'a> {
    link: &'a mut LinkProducer<'static, observed::ControlTick>,
    at: TimestampMicros,
    tick: observed::ControlTick,
}

impl Drop for TickReport<'_> {
    fn drop(&mut self) {
        self.link.push(self.at, self.tick);
    }
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
        control_tick_link: LinkProducer<'static, observed::ControlTick>,
        // The latest attitude estimate, reported on every tick, including
        // those that return before the estimator runs.
        attitude_deg: [f32; 3] = [0.0; 3],
        // Control-timer interrupts and rate-loop ticks since boot, and the
        // isolated stale ticks the loop has ridden through.
        control_isr_seq: u32 = 0,
        control_rate_seq: u32 = 0,
        tolerated_stale_ticks: u32 = 0,
        rc_link_was_valid: bool = false,
        motor_order: ferrowasp_flight_control::actuator::MotorOutputMap =
            ferrowasp_flight_control::actuator::MotorOutputMap::IDENTITY,
    ],
    shared = [
        imu_data: imu::ImuData,
        tuning_profile: dt::TuningProfile,
        tuning_request_seq: u32,
    ],
    spawn = [
        actuator_output(cmd: safety::ActuatorCmd),
        safety_master(event: safety::SafetyEvent),
    ],
    // `control_loop_rate_hz` is a board fact, not a shared one: it is bounded
    // by the sensor the board carries and the bus it sits on. A board with an
    // ICM42688P at 2 kHz and one with an MPU6500 at 1 kHz cannot share a rate
    // without one of them running on samples it has already seen.
    config = [
        imu_control_axis_profile: ferrowasp_control::frames::ImuControlAxisProfile,
        imu_gyro_counts_per_dps: f32,
        logical_to_physical_motor_output: [usize; 4],
        control_loop_rate_hz: u32,
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
    // Incremet Counter
    *cnt += 1;
    *cx.local.control_isr_seq = cx.local.control_isr_seq.wrapping_add(1);

    let tick_at = TimestampMicros(Mono::now().duration_since_epoch().to_micros());
    if !IMU_TRANSPORT_READY.load(Ordering::Relaxed) {
        *cnt = 0;
        IMU_STALE.store(true, Ordering::Release);
        cx.local.control_tick_link.push(
            tick_at,
            observed::ControlTick {
                isr_seq: *cx.local.control_isr_seq,
                imu_stale: true,
                armed: cx.local.control_safety_arm_reader.read(),
                attitude_deg: *cx.local.attitude_deg,
                ..observed::ControlTick::default()
            },
        );
        return;
    }

    // ---- CONTROL LOOP ----
    // Check if required samples per control loop is reached
    if cnt >= samples_per_control_loop {
        *cnt = 0; // Reset sampling counter
        let mut report = TickReport {
            link: cx.local.control_tick_link,
            at: tick_at,
            tick: observed::ControlTick {
                isr_seq: *cx.local.control_isr_seq,
                tolerated_stale_ticks: *cx.local.tolerated_stale_ticks,
                attitude_deg: *cx.local.attitude_deg,
                ..observed::ControlTick::default()
            },
        };

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
        *cx.local.imu_stale_ticks = if imu_fresh {
            0
        } else {
            cx.local.imu_stale_ticks.saturating_add(1)
        };
        // Stale for pre-arm, the OSD and USB status means stale beyond the
        // tolerance, the same rule that disarms below.
        let imu_stale = safety::imu_stale_beyond_tolerance(*cx.local.imu_stale_ticks);
        IMU_STALE.store(imu_stale, Ordering::Release);

        let control_armed = cx.local.control_safety_arm_reader.read();
        report.tick.imu_seq = imu_sequence;
        report.tick.imu_fresh = imu_fresh;
        report.tick.imu_stale = imu_stale;
        report.tick.armed = control_armed;
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
        report.tick.bias_calibrated = IMU_BIAS_CALIBRATED.load(Ordering::Relaxed);
        let control_gyro_raw = gyro_bias_update.corrected_raw;
        report.tick.gyro_raw = control_gyro_raw;
        let imu_roll_raw = control_gyro_raw[0] as f32 / CONFIG::IMU_GYRO_COUNTS_PER_DPS;
        let imu_pitch_raw = control_gyro_raw[1] as f32 / CONFIG::IMU_GYRO_COUNTS_PER_DPS;
        let imu_yaw_raw = control_gyro_raw[2] as f32 / CONFIG::IMU_GYRO_COUNTS_PER_DPS;
        let (imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered) = cx
            .local
            .imu_rate_filter
            .update(imu_roll_raw, imu_pitch_raw, imu_yaw_raw);
        // The rate controller retains FCU3's historical nose-down-positive
        // pitch convention. Translate its filtered rates back to physical
        // body rates for attitude estimation and external reporting.
        let body_rates = ferrowasp_control::frames::RATE_CONTROLLER_TO_BODY_MAP.map_f32([
            imu_roll_filtered,
            imu_pitch_filtered,
            imu_yaw_filtered,
        ]);
        report.tick.raw_gyro_dps = [imu_roll_raw, imu_pitch_raw, imu_yaw_raw];
        report.tick.filtered_gyro_dps = [imu_roll_filtered, imu_pitch_filtered, imu_yaw_filtered];
        report.tick.body_rates_dps = body_rates;
        // Bench-only: exercise the blackbox write path while disarmed.
        //
        // Every stored record is otherwise an armed record, so the flash write
        // path is only ever tested by flying. That leaves the one question a
        // loop-rate change actually turns on - whether the store keeps up at
        // this many records per second - answerable only in the air.
        //
        // This records with motors at zero and the armed flag false, so the
        // rate is real but nothing actuates. It consumes the same flight log
        // as a real flight, so it is never in a flight image: the powered
        // props-off gate rejects bench images by name.
        //
        // It fills a 16 MB log in minutes and, unlike a flight, never stops.
        // Erasing while this image is running is pointless - the erase
        // completes, verifies empty, and the running firmware refills the
        // region before anyone can look. Flash a non-bench image first, then
        // erase. Download anything worth keeping before either.
        // A disarmed tick's commands, PID terms and motors stay zero.
        #[cfg(feature = "bench_blackbox")]
        if !control_armed {
            report.tick.logged = true;
        }

        *cx.local.control_rate_seq = cx.local.control_rate_seq.wrapping_add(1);
        report.tick.control_seq = *cx.local.control_rate_seq;
        if !control_armed {
            // Keep Foxeer aligned with the FCU3 golden app: no
            // PID/filter/setpoint/mixer state crosses an arming boundary.
            fc.reset_control_state();

            let pending_seq = cx.shared.tuning_request_seq.lock(|seq| *seq);
            if pending_seq != *cx.local.applied_tuning_seq {
                let profile = cx.shared.tuning_profile.lock(|profile| *profile);
                fc.apply_tuning_profile(profile);
                // Like every tuning change, a new motor order takes effect
                // only here, while disarmed.
                *cx.local.motor_order = profile.motor_map;
                // The stored value is a corner in hertz; the filter wants a
                // one-pole coefficient, which only exists relative to a rate.
                cx.local.imu_rate_filter.set_alpha(dt::gyro_lpf_alpha(
                    profile.sanitized().imu_lpf_hz,
                    CONFIG::CONTROL_LOOP_RATE_HZ as f32,
                ));
                *cx.local.applied_tuning_seq = pending_seq;
                info!("Applied disarmed OSD tuning profile {}", pending_seq);
            }

            #[cfg(feature = "blackbox_defmt")]
            {
                let rc_raw = cx.local.rc_rates_reader.read();
                dt::emit_compact_blackbox(
                    *cx.local.control_rate_seq,
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

        // The mixer's map: the pilot's motor order over the board's wiring.
        let motor_output_map = cx
            .local
            .motor_order
            .compose(CONFIG::LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT);
        MOTOR_OUTPUT_MAP_SNAPSHOT.store(
            ferrowasp_flight_control::actuator::pack_motor_map(motor_output_map),
            Ordering::Release,
        );

        if !imu_fresh {
            // A tolerated tick skips this update; the motors hold the
            // previous command.
            if !imu_stale {
                *cx.local.tolerated_stale_ticks = cx.local.tolerated_stale_ticks.wrapping_add(1);
                report.tick.tolerated_stale_ticks = *cx.local.tolerated_stale_ticks;
                return;
            }

            let stale_ticks = *cx.local.imu_stale_ticks;
            if stale_ticks == safety::IMU_STALE_TICKS_TOLERATED + 1
                || stale_ticks.is_multiple_of(100)
            {
                warn!(
                    "IMU stale in control loop: seq {}, stale ticks {}",
                    imu_sequence, stale_ticks
                );
            }

            if cx.local.control_safety_arm_reader.read() {
                let _ = cx.spawn.actuator_output(safety::ActuatorCmd::Disarm);
                let _ = cx.spawn.safety_master(safety::SafetyEvent::DisarmRequested);
            }

            return;
        }

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
            accel_level().apply(drone_gravity),
            1.0 / CONFIG::CONTROL_LOOP_RATE_HZ as f32,
        );
        *cx.local.attitude_deg = imu_angles;
        report.tick.attitude_deg = imu_angles;

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
                    *cx.local.control_rate_seq,
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
                    *cx.local.control_rate_seq,
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
                    *cx.local.control_rate_seq,
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
                    *cx.local.control_rate_seq,
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
                let motor_commands =
                    remap_motor_outputs([bench_throttle, 0.0, 0.0, 0.0], motor_output_map);

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    *cx.local.control_rate_seq,
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
                let motor_commands =
                    remap_motor_outputs([0.0, bench_throttle, 0.0, 0.0], motor_output_map);

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    *cx.local.control_rate_seq,
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
                let motor_commands =
                    remap_motor_outputs([0.0, 0.0, bench_throttle, 0.0], motor_output_map);

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    *cx.local.control_rate_seq,
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
                let motor_commands =
                    remap_motor_outputs([0.0, 0.0, 0.0, bench_throttle], motor_output_map);

                #[cfg(feature = "blackbox_defmt")]
                dt::emit_compact_blackbox(
                    *cx.local.control_rate_seq,
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
                    *cx.local.control_rate_seq,
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
                report.tick.logged = true;
                report.tick.command_dps =
                    [rc_raw.roll as f32, rc_raw.pitch as f32, rc_raw.yaw as f32];
                report.tick.throttle = bench_throttle;
                report.tick.motors = motor_commands;
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
                fc.update_motor_commands_dt(1.0 / CONFIG::CONTROL_LOOP_RATE_HZ as f32);
                let motor_commands =
                    remap_motor_outputs(fc.get_logical_motor_commands(), motor_output_map);

                {
                    let mut sample = fc.blackbox_sample();
                    sample.motors =
                        remap_motor_outputs(fc.get_logical_motor_commands(), motor_output_map);
                    #[cfg(feature = "blackbox_defmt")]
                    dt::emit_rate_blackbox(dt::CompactRateBlackboxSample::from_rate_sample(
                        *cx.local.control_rate_seq,
                        imu_sequence,
                        control_armed,
                        imu_fresh,
                        [imu_roll_raw, imu_pitch_raw, imu_yaw_raw],
                        sample,
                    ));
                    report.tick.logged = true;
                    report.tick.filtered_gyro_dps = sample.measurement;
                    report.tick.command_dps = sample.setpoint;
                    report.tick.pid = sample.pid;
                    report.tick.throttle = sample.throttle;
                    report.tick.motors = sample.motors;
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
