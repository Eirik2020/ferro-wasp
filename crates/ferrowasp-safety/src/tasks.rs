//! The safety master's tasks (design §16), target-only. The safety master is
//! the only task that arms or disarms the system. It
//! decides on arm requests, completes arming once the actuator reports idle,
//! and disarms on request, on an aborted arming, or when the RC link is
//! invalidated. It commands the actuator only by spawning `actuator_output`,
//! and it alone restarts the board, on a reboot the coordinator passes it.
//!
//! Requirements: ARM-001, ARM-004, ARM-002, INPUT-010, INPUT-019, INPUT-020,
//! SAFETY-015
//! Safety-critical.

use crate::prearm::ImuPreArmReader;
use crate::{self as safety, signals};
use defmt::{info, warn};
use ferrowasp_boundary_contracts::messages as observed;
use ferrowasp_io_contracts::link::LinkProducer;
use ferrowasp_io_contracts::time::TimestampMicros;
use fugit::ExtU64 as _;

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
        safety::ArmingAbortReason::ControlStatusStale => {
            warn!("Arming aborted: no recent IMU status from the control loop")
        }
        safety::ArmingAbortReason::HostConnected => {
            warn!("Arming refused: USB or the configurator link is connected; unplug to arm")
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

/// Tell the observer the arm state, after every change to it.
fn report_arm_state(
    link: &mut LinkProducer<'static, observed::SafetyStatus>,
    now_us: u64,
    armed: bool,
) {
    link.push(TimestampMicros(now_us), observed::SafetyStatus { armed });
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
        // When a bench motor request was last accepted, in milliseconds
        // since boot; arming is refused for a short while after it.
        last_bench_request_ms: Option<u64> = None,
        safety_status_link: LinkProducer<'static, observed::SafetyStatus>,
        safety_imu_prearm: ImuPreArmReader<'static>,
        // Each host link's lease, renewed through the coordinator; arming is
        // refused while one stands, and actuator output reads the flag.
        host_leases: safety::host::HostLeases = safety::host::HostLeases::new(),
        host_lease_writer: signals::HostLeaseWriter,
        safety_reply_link: LinkProducer<'static, safety::SafetyReply>,
    ],
    spawn = [actuator_output(cmd: safety::ActuatorCmd)],
    config = [
        actuator_output_enabled: bool,
        actuator_inhibit_reason: &'static str,
        bench_actuator_validation_enabled: bool,
        validate_live_arming_guard: safety::ArmingGuard,
        reboot: fn(safety::RebootTarget) -> !,
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
    let status_link = cx.local.safety_status_link;
    let now_us = Mono::now().duration_since_epoch().to_micros();

    match event {
        safety::SafetyEvent::ArmRequested => {
            system_arm.disarm();
            report_arm_state(status_link, now_us, system_arm.is_armed());

            if safety::bench_blocks_arming(*cx.local.last_bench_request_ms, now_us / 1_000) {
                arm_permit.revoke();
                warn!("Arming refused: a motor test ran in the last two seconds");
                return;
            }

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
                cx.local.host_leases.present(now_us / 1_000),
                cx.local.safety_imu_prearm.current(TimestampMicros(now_us)),
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
                    cx.local.host_leases.present(now_us / 1_000),
                    cx.local.safety_imu_prearm.current(TimestampMicros(now_us)),
                )
                .is_ok()
                && actuator_done.read()
            {
                arm_permit.revoke();
                system_arm.arm();
                report_arm_state(status_link, now_us, system_arm.is_armed());
                if CONFIG::BENCH_ACTUATOR_VALIDATION_ENABLED {
                    warn!("SYSTEM ARMED FOR CAPPED ACTUATOR VALIDATION");
                } else {
                    info!("SYSTEM ARMED");
                }
            } else {
                arm_permit.revoke();
                system_arm.disarm();
                report_arm_state(status_link, now_us, system_arm.is_armed());

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
            report_arm_state(status_link, now_us, system_arm.is_armed());
            warn_arming_abort(reason);
        }

        safety::SafetyEvent::DisarmRequested => {
            let arming_active = arm_permit.revoke();
            system_arm.disarm();
            report_arm_state(status_link, now_us, system_arm.is_armed());
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

        // A props-off motor check from the configurator, through the
        // coordinator. Decided here, like every other request that can reach
        // the motors, and refused unless the system is disarmed, not arming,
        // and the arm switch is off; the decision goes back to the
        // coordinator by its ticket.
        safety::SafetyEvent::BenchMotor { request, ticket } => {
            let decision = safety::validate_bench_motor_request(
                request,
                system_arm.is_armed(),
                arm_permit.is_allowed(),
                rc_arm_high.read(),
                CONFIG::ACTUATOR_OUTPUT_ENABLED,
            );
            cx.local.safety_reply_link.push(
                TimestampMicros(now_us),
                safety::SafetyReply {
                    ticket,
                    outcome: match decision {
                        Ok(()) => safety::Outcome::Accepted,
                        Err(refusal) => safety::Outcome::Refused(safety::bench_refusal(refusal)),
                    },
                },
            );
            match decision {
                Ok(()) => {
                    if request != safety::BenchMotorRequest::Stop {
                        *cx.local.last_bench_request_ms = Some(now_us / 1_000);
                    }
                    if cx
                        .spawn
                        .actuator_output(safety::ActuatorCmd::BenchMotor(request))
                        .is_err()
                    {
                        warn!("Bench motor request dropped: actuator busy");
                    }
                }
                Err(refusal) => warn!("Bench motor request refused: {}", refusal.as_str()),
            }
        }

        // A restart the storage owner asked for once its store was settled.
        // Only disarmed and not arming: the reset ends the flight firmware,
        // and nothing between the check and the reset can arm, since arming
        // is this task's alone.
        // An accepted reboot never returns, so only a refusal is answered.
        safety::SafetyEvent::Reboot { target, ticket } => {
            match safety::validate_reboot(system_arm.is_armed(), arm_permit.is_allowed()) {
                Ok(()) => {
                    warn!("Rebooting on request");
                    (CONFIG::REBOOT)(target);
                }
                Err(refusal) => {
                    cx.local.safety_reply_link.push(
                        TimestampMicros(now_us),
                        safety::SafetyReply {
                            ticket,
                            outcome: safety::Outcome::Refused(refusal),
                        },
                    );
                }
            }
        }

        safety::SafetyEvent::HostPresent(link) => {
            cx.local.host_leases.renew(link, now_us / 1_000);
            cx.local
                .host_lease_writer
                .write(cx.local.host_leases.until_ms());
        }

        safety::SafetyEvent::RcLinkInvalid(reason) => {
            if !link_invalidator.invalidate(reason) {
                return;
            }
            let arming_active = arm_permit.revoke();
            system_arm.disarm();
            report_arm_state(status_link, now_us, system_arm.is_armed());

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
                    warn!("RC link invalidated by an RC port DMA error")
                }
                safety::RcLinkInvalidation::ParserError => {
                    warn!("RC link invalidated by a receiver parser error")
                }
                safety::RcLinkInvalidation::FrameLost => {
                    warn!("RC link invalidated by a receiver frame-lost report")
                }
                safety::RcLinkInvalidation::Failsafe => {
                    warn!("RC link invalidated by the receiver failsafe flag")
                }
                safety::RcLinkInvalidation::Timeout => {
                    warn!("RC link invalidated by frame timeout")
                }
                safety::RcLinkInvalidation::ChannelMapChanged => {
                    warn!("RC link invalidated: the channel map changed; arm switch must read low")
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
