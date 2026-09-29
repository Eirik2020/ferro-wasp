//! The safety master: the only task that arms or disarms the system. It
//! decides on arm requests, completes arming once the actuator reports idle,
//! and disarms on request, on an aborted arming, or when the RC link is
//! invalidated. It commands the actuator only by spawning `actuator_output`.

use crate::prelude::*;

pub fn warn_arming_abort(reason: safety::ArmingAbortReason) {
    match reason {
        safety::ArmingAbortReason::PermitRevoked => {
            warn!("Arming aborted: actuator permission revoked")
        }
        safety::ArmingAbortReason::RcLinkInvalid => {
            warn!("Arming aborted: RC link is not armable")
        }
        safety::ArmingAbortReason::ArmSwitchLow => {
            warn!("Arming aborted: arm switch is low")
        }
        safety::ArmingAbortReason::ThrottleHigh => warn!(
            "Arming aborted: throttle exceeds {}",
            safety::ARMING_MAX_THROTTLE
        ),
        safety::ArmingAbortReason::ImuUnavailable => {
            warn!("Arming aborted: IMU has not produced a valid sample")
        }
        safety::ArmingAbortReason::ImuBiasUncalibrated => {
            warn!("Arming aborted: gyro bias calibration is incomplete")
        }
        safety::ArmingAbortReason::ImuStale => {
            warn!("Arming aborted: IMU sample is stale")
        }
        safety::ArmingAbortReason::EscIdleTelemetryTimeout => {
            warn!("Arming aborted: ESC idle telemetry qualification timed out")
        }
        safety::ArmingAbortReason::EscIdleRpmOutOfRange => {
            warn!("Arming aborted: ESC idle eRPM outside the permitted range")
        }
        safety::ArmingAbortReason::EscIdleQualificationInvalid => {
            warn!("Arming aborted: invalid ESC idle qualification profile")
        }
        safety::ArmingAbortReason::CompletionDeliveryFailed => {
            warn!("Arming aborted: idle completion delivery failed")
        }
    }
}

/// Handle one safety event. Priority is the firmware's, and is the highest
/// software priority in the flight apps, so no other task preempts a
/// decision.
///
/// Whether actuator output is enabled, why not, and whether this is a capped
/// bench validation image are the firmware's build facts; the live arming
/// guard - which pre-arm health it requires - is the board's safety policy.
/// All four are configuration.
// The body moved verbatim from the Foxeer app, where clippy never saw it
// inside `#[rtic::app]`. The unwrap follows its own `is_ok` check; it is kept
// as written so this safety path moves without a logic change.
#[allow(clippy::unnecessary_unwrap)]
#[ferroforge::task(
    local = [
        safety_rc_arm_high_reader: signals::RcArmHighReader,
        safety_rc_throttle_reader: signals::RcThrottleReader,
        safety_rc_link_reader: signals::RcLinkReader,
        safety_arm_writer: signals::SafetyArmWriter,
        rc_link_invalidator: signals::RcLinkInvalidator,
        actuator_arm_permit_writer: signals::ActuatorArmPermitWriter,
        actuator_arm_done_reader: signals::ActuatorArmDoneReader,
    ],
    spawn = [actuator_output(cmd: safety::ActuatorCmd)],
    config = [
        actuator_output_enabled: bool,
        actuator_inhibit_reason: &'static str,
        bench_actuator_validation_enabled: bool,
        validate_live_arming_guard: fn(bool, bool, bool, u32) -> Result<(), safety::ArmingAbortReason>,
    ],
    monotonic = Mono,
)]
pub async fn safety_master(cx: safety_master::Context, event: safety::SafetyEvent) {
    let rc_arm_high = cx.local.safety_rc_arm_high_reader;
    let rc_throttle = cx.local.safety_rc_throttle_reader;
    let rc_link = cx.local.safety_rc_link_reader;
    let system_arm = cx.local.safety_arm_writer;
    let link_invalidator = cx.local.rc_link_invalidator;
    let arm_permit = cx.local.actuator_arm_permit_writer;
    let actuator_done = cx.local.actuator_arm_done_reader;
    let now_us = Mono::now().duration_since_epoch().to_micros();

    match event {
        safety::SafetyEvent::ArmRequested => {
            system_arm.disarm();

            if !CONFIG::ACTUATOR_OUTPUT_ENABLED {
                arm_permit.revoke();
                warn!("Arming inhibited: {}", CONFIG::ACTUATOR_INHIBIT_REASON);
                return;
            }

            let guard = CONFIG::VALIDATE_LIVE_ARMING_GUARD(
                true,
                rc_link.is_armable(now_us),
                rc_arm_high.read(),
                rc_throttle.read(),
            );
            if guard.is_ok() {
                arm_permit.allow();
                info!("Attempting DShot safety arming");

                if cx
                    .spawn
                    .actuator_output(safety::ActuatorCmd::EnterIdle)
                    .is_err()
                {
                    arm_permit.revoke();
                    warn!("Failed to spawn actuator EnterIdle");
                }
            } else {
                arm_permit.revoke();
                warn_arming_abort(guard.unwrap_err());
            }
        }

        safety::SafetyEvent::ActuatorIdling => {
            if CONFIG::ACTUATOR_OUTPUT_ENABLED
                && CONFIG::VALIDATE_LIVE_ARMING_GUARD(
                    arm_permit.is_allowed(),
                    rc_link.is_armable(now_us),
                    rc_arm_high.read(),
                    rc_throttle.read(),
                )
                .is_ok()
                && actuator_done.read()
            {
                arm_permit.revoke();
                system_arm.arm();
                if CONFIG::BENCH_ACTUATOR_VALIDATION_ENABLED {
                    warn!("SYSTEM ARMED FOR CAPPED FOXEER ACTUATOR VALIDATION");
                } else {
                    info!("SYSTEM ARMED");
                }
            } else {
                arm_permit.revoke();
                system_arm.disarm();

                let _ = cx.spawn.actuator_output(safety::ActuatorCmd::Disarm);
                warn!("ARM FAILED after actuator idle");
                info!(
                    "RC throttle {} vs min {}",
                    rc_throttle.read(),
                    safety::ESC_IDLE_THROTTLE
                );
            }
        }

        safety::SafetyEvent::ArmingAborted(reason) => {
            if !arm_permit.revoke() {
                return;
            }
            system_arm.disarm();
            warn_arming_abort(reason);
        }

        safety::SafetyEvent::DisarmRequested => {
            let arming_active = arm_permit.revoke();
            system_arm.disarm();
            info!("SYSTEM DISARMED");

            if cx
                .spawn
                .actuator_output(safety::ActuatorCmd::Disarm)
                .is_err()
                && !arming_active
            {
                warn!("Failed to spawn actuator Disarm");
            }
        }

        safety::SafetyEvent::RcLinkInvalid(reason) => {
            if !link_invalidator.invalidate(reason) {
                return;
            }
            let arming_active = arm_permit.revoke();
            system_arm.disarm();

            if cx
                .spawn
                .actuator_output(safety::ActuatorCmd::Disarm)
                .is_err()
                && !arming_active
            {
                warn!("Failed to spawn actuator Disarm after RC invalidation");
            }

            match reason {
                safety::RcLinkInvalidation::Startup => warn!("RC link invalid at startup"),
                safety::RcLinkInvalidation::TransportDiscontinuity => {
                    warn!("RC link invalidated by transport discontinuity")
                }
                safety::RcLinkInvalidation::DmaError => {
                    warn!("RC link invalidated by USART2 DMA error")
                }
                safety::RcLinkInvalidation::ParserError => {
                    warn!("RC link invalidated by SBUS parser error")
                }
                safety::RcLinkInvalidation::SbusFrameLost => {
                    warn!("RC link invalidated by SBUS frame-lost flag")
                }
                safety::RcLinkInvalidation::SbusFailsafe => {
                    warn!("RC link invalidated by SBUS failsafe flag")
                }
                safety::RcLinkInvalidation::Timeout => {
                    warn!("RC link invalidated by frame timeout")
                }
            }
        }
    }
}

/// Tell the safety master the actuator has reached idle, retrying every
/// millisecond until the notification is accepted: arming cannot complete
/// without it, and dropping it would strand the arming sequence.
#[ferroforge::task(
    spawn = [safety_master(event: safety::SafetyEvent)],
    monotonic = Mono,
)]
pub async fn actuator_idle_notify(cx: actuator_idle_notify::Context) {
    let mut retry_logged = false;
    while cx
        .spawn
        .safety_master(safety::SafetyEvent::ActuatorIdling)
        .is_err()
    {
        if !retry_logged {
            retry_logged = true;
            warn!("Retrying actuator-idle notification");
        }
        Mono::delay(1.millis()).await;
    }
}
