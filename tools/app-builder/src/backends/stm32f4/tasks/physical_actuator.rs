use ferrowasp_core::{
    safety::{
        ActuatorAuthority, ActuatorCmd, ActuatorGuardReport, ActuatorPreparationReport,
        ArmingAbortReason, MOTOR_CMD_MAX_AGE_MS, MotorCmd,
    },
    safety_channel::{SafetyConsumer, SafetyProducer},
};
use ferrowasp_tasks::esc_manager::{
    DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_IDLE_THROTTLE_COMMAND, DSHOT_PREARM_STOP_HOLD_MS,
    EscIdleQualification, EscIdleQualificationFailure, EscIdleQualificationStatus,
    EscTelemetryUpdateConsumer,
};
use fugit::MillisDurationU32;

use crate::{backends::stm32f4::task_authoring::dshot::DshotMotorBank, rtic::task::Mono};

crate::reusable_task! {
    contract {
        local {
            /// Exclusive command receive handle owned by actuator safety.
            commands: SafetyConsumer<'static, MotorCmd>,
            /// Exclusive safety-owned authority observations.
            guards: SafetyConsumer<'static, ActuatorGuardReport>,
            /// Exclusive preparation completion path into safety.
            completions: SafetyProducer<'static, ActuatorPreparationReport>,
            /// Fresh physical-output ESC observations used only for pre-arm qualification.
            telemetry: EscTelemetryUpdateConsumer,
        }
        shared {
            /// Indivisible four-lane DShot containment unit.
            bank: DshotMotorBank,
        }
        config {
            /// Independent physical-output gate.
            output_enabled: bool,
            /// Guard/lease/qualification polling cadence.
            poll_interval: MillisDurationU32,
        }
        spawns {
            /// Fail-closed fallback when the direct completion path cannot report.
            fault(report: ferrowasp_core::safety::ActuatorPreparationReport),
        }
    }

    /// Sole adapter allowed to translate fresh safety-approved requests into DShot commands.
    pub async fn physical_actuator(
        mut cx: physical_actuator::Context<'_>,
        request: ferrowasp_core::safety::ActuatorCmd,
    ) {
        let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
        if !cx.config.output_enabled {
            cx.shared.bank.lock(|bank| bank.command_stop());
            let _ = ferrowasp_tasks::actuator::handle_inhibited_actuator_wake(
                request,
                now_ms,
                || cx.local.commands.try_receive(),
            );
            if matches!(request, ActuatorCmd::EnterIdle) {
                let report = ActuatorPreparationReport::Aborted(ArmingAbortReason::PermitRevoked);
                let _ = cx.local.completions.try_send(report);
            }
            return;
        }

        match request {
            ActuatorCmd::Disarm | ActuatorCmd::Calibrate => {
                for _ in 0..ferrowasp_tasks::actuator::MOTOR_COMMAND_USABLE_CAPACITY {
                    if cx.local.commands.try_receive().is_none() {
                        break;
                    }
                }
                cx.shared.bank.lock(|bank| bank.command_stop());
            }
            ActuatorCmd::ApplyLatestThrottle => {
                let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
                let now_ms = now_us / 1_000;
                let command = ferrowasp_tasks::actuator::take_physical_motor_command(
                    now_us,
                    now_ms,
                    || cx.local.guards.try_receive(),
                    || cx.local.commands.try_receive(),
                );
                match command {
                    Ok(values) => {
                        let applied = cx.shared.bank.lock(|bank| {
                            bank.command_throttles(values, now_ms, MOTOR_CMD_MAX_AGE_MS)
                        });
                        if applied.is_err() {
                            cx.shared.bank.lock(|bank| bank.command_stop());
                            let _ = fault::spawn(ActuatorPreparationReport::Faulted);
                        }
                    }
                    Err(_) => {
                        cx.shared.bank.lock(|bank| bank.command_stop());
                        let _ = fault::spawn(ActuatorPreparationReport::Faulted);
                    }
                }
            }
            ActuatorCmd::EnterIdle => {
                cx.shared.bank.lock(|bank| bank.command_stop());
                let stop_iterations = DSHOT_PREARM_STOP_HOLD_MS / 10;
                let mut abort_reason = None;
                for _ in 0..stop_iterations {
                    let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
                    if ferrowasp_tasks::actuator::take_latest_guard(
                        now_us,
                        ActuatorAuthority::Preparing,
                        || cx.local.guards.try_receive(),
                    )
                    .is_err()
                    {
                        abort_reason = Some(ArmingAbortReason::PermitRevoked);
                        break;
                    }
                    cx.shared.bank.lock(|bank| bank.command_stop());
                    Mono::delay(cx.config.poll_interval).await;
                }

                for _ in 0..ferrowasp_tasks::esc_manager::ESC_TELEMETRY_UPDATE_QUEUE_CAPACITY {
                    if cx.local.telemetry.dequeue().is_none() {
                        break;
                    }
                }
                let started_ms = Mono::now().duration_since_epoch().to_millis() as u32;
                let mut qualification =
                    EscIdleQualification::new(DSHOT_IDLE_QUALIFICATION_CONFIG, started_ms);
                while abort_reason.is_none() {
                    let now_us = Mono::now().duration_since_epoch().to_micros() as u32;
                    let now_ms = now_us / 1_000;
                    if ferrowasp_tasks::actuator::take_latest_guard(
                        now_us,
                        ActuatorAuthority::Preparing,
                        || cx.local.guards.try_receive(),
                    )
                    .is_err()
                    {
                        abort_reason = Some(ArmingAbortReason::PermitRevoked);
                        break;
                    }
                    let idle = [DSHOT_IDLE_THROTTLE_COMMAND; 4];
                    if cx
                        .shared
                        .bank
                        .lock(|bank| bank.command_throttles(idle, now_ms, MOTOR_CMD_MAX_AGE_MS))
                        .is_err()
                    {
                        abort_reason = Some(ArmingAbortReason::CompletionDeliveryFailed);
                        break;
                    }

                    let mut status = EscIdleQualificationStatus::Pending;
                    for _ in 0..ferrowasp_tasks::esc_manager::ESC_TELEMETRY_UPDATE_QUEUE_CAPACITY {
                        let Some(update) = cx.local.telemetry.dequeue() else {
                            break;
                        };
                        status = qualification.observe(update, now_ms);
                    }
                    if matches!(status, EscIdleQualificationStatus::Pending) {
                        status = qualification.status(now_ms);
                    }
                    match status {
                        EscIdleQualificationStatus::Pending => {}
                        EscIdleQualificationStatus::Qualified => break,
                        EscIdleQualificationStatus::Failed(
                            EscIdleQualificationFailure::Overspeed { .. },
                        ) => abort_reason = Some(ArmingAbortReason::EscIdleRpmOutOfRange),
                        EscIdleQualificationStatus::Failed(
                            EscIdleQualificationFailure::Timeout { .. },
                        ) => abort_reason = Some(ArmingAbortReason::EscIdleTelemetryTimeout),
                        EscIdleQualificationStatus::Failed(
                            EscIdleQualificationFailure::InvalidConfig,
                        ) => abort_reason = Some(ArmingAbortReason::EscIdleQualificationInvalid),
                    }
                    if abort_reason.is_none()
                        && !matches!(status, EscIdleQualificationStatus::Qualified)
                    {
                        Mono::delay(cx.config.poll_interval).await;
                    } else {
                        break;
                    }
                }

                let report = match abort_reason {
                    Some(reason) => {
                        cx.shared.bank.lock(|bank| bank.command_stop());
                        ActuatorPreparationReport::Aborted(reason)
                    }
                    None => ActuatorPreparationReport::Qualified,
                };
                if cx.local.completions.try_send(report).is_err() {
                    cx.shared.bank.lock(|bank| bank.command_stop());
                    let _ = fault::spawn(ActuatorPreparationReport::Faulted);
                }
            }
        }
    }
}
