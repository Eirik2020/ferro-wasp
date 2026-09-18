//! The actuator output path: the only task that commands the motors. It
//! prepares DShot idle when arming - a stop-frame hold, then eRPM
//! qualification of every output - and applies fresh motor commands only
//! while the system is armed. `ActuatorHardware` is the one place a motor
//! command reaches the DShot bank.

use crate::prelude::*;
use ferrowasp_stm32f4::dshot;

/// A live arming guard: permit, RC link armable, arm switch high, throttle.
/// Which pre-arm health it also requires is the board's safety policy.
pub type ArmingGuard = fn(bool, bool, bool, u32) -> Result<(), safety::ArmingAbortReason>;

pub struct ActuatorHardware<'a, SharedDshot>
where
    SharedDshot: rtic::Mutex<T = dshot::DshotMotorBank>,
{
    dshot: &'a mut SharedDshot,
}

impl<'a, SharedDshot> ActuatorHardware<'a, SharedDshot>
where
    SharedDshot: rtic::Mutex<T = dshot::DshotMotorBank>,
{
    pub fn new(dshot: &'a mut SharedDshot) -> Self {
        Self { dshot }
    }

    pub fn force_off(&mut self) {
        self.dshot.lock(|dshot| dshot.command_stop());
    }

    pub fn apply(&mut self, values: [f32; 4], now_ms: u32) -> bool {
        self.apply_with_lease(values, now_ms, safety::MOTOR_CMD_MAX_AGE_MS)
    }

    pub fn apply_with_lease(&mut self, values: [f32; 4], now_ms: u32, lease_ms: u32) -> bool {
        let commands = values.map(throttle_to_u16);
        let result = self
            .dshot
            .lock(|dshot| dshot.command_throttles(commands, now_ms, lease_ms))
            .map_err(|_| ());

        if result.is_err() {
            warn!("Foxeer DShot command rejected");
            self.force_off();
            false
        } else {
            true
        }
    }

    pub fn abort_arming<Report>(
        &mut self,
        done: &signals::ActuatorArmDoneWriter,
        reason: safety::ArmingAbortReason,
        message: &str,
        report: Report,
    ) where
        Report: FnOnce(safety::ArmingAbortReason) -> bool,
    {
        done.clear();
        self.force_off();
        warn!("{}", message);
        if !report(reason) {
            warn!("Failed to report aborted DShot idle qualification");
        }
    }
}

pub fn current_live_arming_guard(
    guard: ArmingGuard,
    permit: &signals::ActuatorArmPermitReader,
    rc_link: &signals::RcLinkReader,
    arm_high: &signals::RcArmHighReader,
    throttle: &signals::RcThrottleReader,
    now_us: u32,
) -> Result<(), safety::ArmingAbortReason> {
    guard(
        permit.read(),
        rc_link.is_armable(now_us),
        arm_high.read(),
        throttle.read(),
    )
}

// Eight arguments: the seven it had in the app, plus the board's guard it
// used to name directly.
#[allow(clippy::too_many_arguments)]
pub async fn wait_live_arming_hold<Now, Delay, DelayFuture>(
    guard: ArmingGuard,
    permit: &signals::ActuatorArmPermitReader,
    rc_link: &signals::RcLinkReader,
    arm_high: &signals::RcArmHighReader,
    throttle: &signals::RcThrottleReader,
    hold_ms: u32,
    mut now_us: Now,
    delay_ms: Delay,
) -> Result<(), safety::ArmingAbortReason>
where
    Now: FnMut() -> u32,
    Delay: FnMut(u32) -> DelayFuture,
    DelayFuture: core::future::Future<Output = ()>,
{
    ferrowasp_tasks::arming::wait_hold(
        hold_ms,
        ARMING_GUARD_POLL_MS,
        || current_live_arming_guard(guard, permit, rc_link, arm_high, throttle, now_us()),
        delay_ms,
    )
    .await
}

pub fn take_fresh_motor_outputs(
    reader: &mut safety::signals::MotorCmdReader,
    now_ms: u32,
) -> Option<[f32; 4]> {
    match reader.take_latest_fresh(now_ms, safety::MOTOR_CMD_MAX_AGE_MS) {
        Ok(command) => Some(command.motors),
        Err(safety::MotorCmdReadError::Missing) => {
            warn!("Actuator command refused: motor command queue empty");
            None
        }
        Err(safety::MotorCmdReadError::Stale { seq, age_ms }) => {
            warn!(
                "Actuator command refused: stale motor command seq {}, age {} ms",
                seq, age_ms
            );
            None
        }
    }
}

#[cfg(feature = "bench_dshot_idle_output1_not_running")]
pub fn inject_idle_qualification_fault(
    mut update: esc::EscTelemetryUpdate,
) -> esc::EscTelemetryUpdate {
    if update.output == esc::EscOutput::Output1 {
        update.observation.sample.erpm_div100 = 0;
    }
    update
}

#[cfg(not(feature = "bench_dshot_idle_output1_not_running"))]
pub const fn inject_idle_qualification_fault(
    update: esc::EscTelemetryUpdate,
) -> esc::EscTelemetryUpdate {
    update
}

/// Carry out one actuator command. Priority is the firmware's.
///
/// The board's live arming guard, the DShot idle command, whether actuator
/// output is enabled and why not, and which logical motor each physical ESC
/// output drives are configuration.
// The body moved verbatim from the Foxeer app, where clippy never saw it
// inside `#[rtic::app]`; its trailing `return` is kept as written so the
// motor path moves without a logic change.
#[allow(unused_mut, clippy::needless_return)]
#[ferroforge::task(
    shared = [dshot_motors: dshot::DshotMotorBank],
    local = [
        actuator_safety_arm_reader: signals::SafetyArmReader,
        actuator_arm_permit_reader: signals::ActuatorArmPermitReader,
        actuator_rc_arm_high_reader: signals::RcArmHighReader,
        actuator_rc_throttle_reader: signals::RcThrottleReader,
        actuator_rc_link_reader: signals::RcLinkReader,
        actuator_arm_done_writer: signals::ActuatorArmDoneWriter,
        motor_cmd_reader: signals::MotorCmdReader,
        esc_telemetry_update_consumer: esc::EscTelemetryUpdateConsumer,
    ],
    spawn = [safety_master(event: safety::SafetyEvent), actuator_idle_notify()],
    config = [
        validate_live_arming_guard: ArmingGuard,
        dshot_idle_command: f32,
        actuator_output_enabled: bool,
        actuator_inhibit_reason: &'static str,
        esc_output_to_logical_motor: [u8; 4],
    ],
    monotonic = Mono,
)]
pub async fn actuator_output(mut cx: actuator_output::Context, cmd: safety::ActuatorCmd) {
    let mut actuator = ActuatorHardware::new(&mut cx.shared.dshot_motors);

    if !CONFIG::ACTUATOR_OUTPUT_ENABLED {
        actuator.force_off();
        if !matches!(cmd, safety::ActuatorCmd::Disarm) {
            warn!(
                "Actuator command inhibited: {}",
                CONFIG::ACTUATOR_INHIBIT_REASON
            );
        }
        return;
    }

    let safety_armed = cx.local.actuator_safety_arm_reader.read();
    let output = match cmd {
        safety::ActuatorCmd::Disarm => {
            cx.local.motor_cmd_reader.discard_all();
            cx.local.actuator_arm_done_writer.clear();
            actuator.force_off();
            return;
        }

        safety::ActuatorCmd::EnterIdle => {
            cx.local.motor_cmd_reader.discard_all();
            if let Err(reason) = current_live_arming_guard(
                CONFIG::VALIDATE_LIVE_ARMING_GUARD,
                cx.local.actuator_arm_permit_reader,
                cx.local.actuator_rc_link_reader,
                cx.local.actuator_rc_arm_high_reader,
                cx.local.actuator_rc_throttle_reader,
                Mono::now().duration_since_epoch().to_micros(),
            ) {
                cx.local.actuator_arm_done_writer.clear();
                actuator.force_off();
                if cx
                    .spawn
                    .safety_master(safety::SafetyEvent::ArmingAborted(reason))
                    .is_err()
                {
                    warn!("Failed to report rejected actuator preparation");
                }
                return;
            }

            let prepared_output = {
                info!(
                    "Preparing DShot actuators with {} ms of stop frames",
                    DSHOT_PREARM_STOP_HOLD_MS
                );
                cx.local.actuator_arm_done_writer.clear();
                actuator.force_off();
                if let Err(reason) = wait_live_arming_hold(
                    CONFIG::VALIDATE_LIVE_ARMING_GUARD,
                    cx.local.actuator_arm_permit_reader,
                    cx.local.actuator_rc_link_reader,
                    cx.local.actuator_rc_arm_high_reader,
                    cx.local.actuator_rc_throttle_reader,
                    DSHOT_PREARM_STOP_HOLD_MS,
                    || Mono::now().duration_since_epoch().to_micros(),
                    |delay_ms| Mono::delay(delay_ms.millis()),
                )
                .await
                {
                    cx.local.actuator_arm_done_writer.clear();
                    actuator.force_off();
                    if cx
                        .spawn
                        .safety_master(safety::SafetyEvent::ArmingAborted(reason))
                        .is_err()
                    {
                        warn!("Failed to report aborted DShot pre-arm stop hold");
                    }
                    return;
                }

                // Remove samples accumulated while stopped. Qualification
                // accepts only responses observed after idle spin starts.
                while cx.local.esc_telemetry_update_consumer.dequeue().is_some() {}

                let qualification_started_ms = Mono::now().duration_since_epoch().to_millis();
                let mut qualification = esc::EscIdleQualification::new(
                    DSHOT_IDLE_QUALIFICATION_CONFIG,
                    qualification_started_ms,
                );
                info!(
                    "DShot pre-arm stop complete; qualifying idle eRPM {}00..{}00 with {} samples/physical output",
                    DSHOT_IDLE_QUALIFICATION_CONFIG.min_erpm_div100,
                    DSHOT_IDLE_QUALIFICATION_CONFIG.max_erpm_div100,
                    DSHOT_IDLE_QUALIFICATION_CONFIG.required_consecutive_samples
                );

                loop {
                    if let Err(reason) = current_live_arming_guard(
                        CONFIG::VALIDATE_LIVE_ARMING_GUARD,
                        cx.local.actuator_arm_permit_reader,
                        cx.local.actuator_rc_link_reader,
                        cx.local.actuator_rc_arm_high_reader,
                        cx.local.actuator_rc_throttle_reader,
                        Mono::now().duration_since_epoch().to_micros(),
                    ) {
                        actuator.abort_arming(
                            cx.local.actuator_arm_done_writer,
                            reason,
                            "DShot idle qualification aborted by arming guard",
                            |reason| {
                                cx.spawn
                                    .safety_master(safety::SafetyEvent::ArmingAborted(reason))
                                    .is_ok()
                            },
                        );
                        return;
                    }

                    // Idle remains under the temporary arm permit. Renew
                    // its bounded lease while the system is still disarmed.
                    if !actuator.apply_with_lease(
                        [CONFIG::DSHOT_IDLE_COMMAND; 4],
                        Mono::now().duration_since_epoch().to_millis(),
                        safety::MOTOR_CMD_MAX_AGE_MS,
                    ) {
                        return;
                    }
                    let now_ms = Mono::now().duration_since_epoch().to_millis();
                    let mut status = esc::EscIdleQualificationStatus::Pending;
                    while let Some(update) = cx.local.esc_telemetry_update_consumer.dequeue() {
                        status =
                            qualification.observe(inject_idle_qualification_fault(update), now_ms);
                    }
                    if status == esc::EscIdleQualificationStatus::Pending {
                        status = qualification.status(now_ms);
                    }

                    match status {
                        esc::EscIdleQualificationStatus::Pending => {}
                        esc::EscIdleQualificationStatus::Qualified => break,
                        esc::EscIdleQualificationStatus::Failed(
                            esc::EscIdleQualificationFailure::Overspeed {
                                output,
                                erpm_div100,
                            },
                        ) => {
                            warn!(
                                "Foxeer physical ESC output {} (logical M{}) idle qualification overspeed: {}00 eRPM",
                                output.index() + 1,
                                CONFIG::ESC_OUTPUT_TO_LOGICAL_MOTOR[output.index()],
                                erpm_div100
                            );
                            actuator.abort_arming(
                            cx.local.actuator_arm_done_writer,
                            safety::ArmingAbortReason::EscIdleRpmOutOfRange,
                            "Foxeer DShot idle qualification rejected an overspeed physical output",
                            |reason| {
                                cx.spawn.safety_master(safety::SafetyEvent::ArmingAborted(reason))
                                    .is_ok()
                            },
                        );
                            return;
                        }
                        esc::EscIdleQualificationStatus::Failed(
                            esc::EscIdleQualificationFailure::Timeout {
                                consecutive_samples,
                            },
                        ) => {
                            warn!(
                                "Foxeer DShot idle qualification timeout; physical outputs 1/2/3/4 samples [{}, {}, {}, {}]",
                                consecutive_samples[0],
                                consecutive_samples[1],
                                consecutive_samples[2],
                                consecutive_samples[3]
                            );
                            for (index, samples) in consecutive_samples.iter().copied().enumerate()
                            {
                                if samples
                                    < DSHOT_IDLE_QUALIFICATION_CONFIG.required_consecutive_samples
                                {
                                    warn!(
                                        "Foxeer physical ESC output {} (logical M{}) idle qualification failed: motor not running or RPM evidence invalid ({} of {} samples)",
                                        index + 1,
                                        CONFIG::ESC_OUTPUT_TO_LOGICAL_MOTOR[index],
                                        samples,
                                        DSHOT_IDLE_QUALIFICATION_CONFIG
                                            .required_consecutive_samples
                                    );
                                }
                            }
                            actuator.abort_arming(
                            cx.local.actuator_arm_done_writer,
                            safety::ArmingAbortReason::EscIdleTelemetryTimeout,
                            "Foxeer DShot idle qualification did not prove all physical outputs turning",
                            |reason| {
                                cx.spawn.safety_master(safety::SafetyEvent::ArmingAborted(reason))
                                    .is_ok()
                            },
                        );
                            return;
                        }
                        esc::EscIdleQualificationStatus::Failed(
                            esc::EscIdleQualificationFailure::InvalidConfig,
                        ) => {
                            actuator.abort_arming(
                                cx.local.actuator_arm_done_writer,
                                safety::ArmingAbortReason::EscIdleQualificationInvalid,
                                "Foxeer DShot idle qualification profile is invalid",
                                |reason| {
                                    cx.spawn
                                        .safety_master(safety::SafetyEvent::ArmingAborted(reason))
                                        .is_ok()
                                },
                            );
                            return;
                        }
                    }

                    Mono::delay(10.millis()).await;
                }

                info!(
                    "Foxeer DShot idle eRPM qualified; physical outputs 1/2/3/4 samples [{}, {}, {}, {}]",
                    qualification.consecutive_samples()[0],
                    qualification.consecutive_samples()[1],
                    qualification.consecutive_samples()[2],
                    qualification.consecutive_samples()[3]
                );
                [CONFIG::DSHOT_IDLE_COMMAND; 4]
            };

            cx.local.actuator_arm_done_writer.set_done();

            if cx.spawn.actuator_idle_notify().is_err() {
                cx.local.actuator_arm_done_writer.clear();
                actuator.force_off();
                if cx
                    .spawn
                    .safety_master(safety::SafetyEvent::ArmingAborted(
                        safety::ArmingAbortReason::CompletionDeliveryFailed,
                    ))
                    .is_err()
                {
                    warn!("Failed to report actuator preparation completion failure");
                }
                return;
            }

            prepared_output
        }

        safety::ActuatorCmd::ApplyLatestThrottle if safety_armed => {
            match take_fresh_motor_outputs(
                cx.local.motor_cmd_reader,
                Mono::now().duration_since_epoch().to_millis(),
            ) {
                Some(throttles) => match safety::validate_active_motor_outputs_with_idle(
                    throttles,
                    CONFIG::DSHOT_IDLE_COMMAND,
                ) {
                    Ok(outputs) => outputs,
                    Err(_) => {
                        warn!("Actuator command refused: invalid motor output");
                        [safety::ESC_LOW_THROTTLE; 4]
                    }
                },
                None => [safety::ESC_LOW_THROTTLE; 4],
            }
        }

        #[cfg(any(
            feature = "bench_motor1_only",
            feature = "bench_motor2_only",
            feature = "bench_motor3_only",
            feature = "bench_motor4_only",
            feature = "bench_logical_motor1_only",
            feature = "bench_logical_motor2_only",
            feature = "bench_logical_motor3_only",
            feature = "bench_logical_motor4_only"
        ))]
        safety::ActuatorCmd::ApplyBenchSelectedMotor if safety_armed => {
            let mut outputs = [safety::ESC_LOW_THROTTLE; 4];

            if let Some(throttles) = take_fresh_motor_outputs(
                cx.local.motor_cmd_reader,
                Mono::now().duration_since_epoch().to_millis(),
            ) {
                for index in 0..4 {
                    if !throttles[index].is_finite() {
                        warn!("Bench selected motor command refused: invalid motor output");
                        outputs = [safety::ESC_LOW_THROTTLE; 4];
                        break;
                    }

                    if throttles[index] > 0.0 {
                        let idle = CONFIG::DSHOT_IDLE_COMMAND;
                        outputs[index] = throttles[index].clamp(idle, safety::ESC_MAX_THROTTLE);
                    }
                }
            }

            outputs
        }

        _ => {
            warn!("Actuator command refused");
            actuator.force_off();
            return;
        }
    };

    if !actuator.apply(output, Mono::now().duration_since_epoch().to_millis()) {
        return;
    }
}
