//! RC input: SBUS or CRSF frames from the port config bound to RC, read
//! through the configured channel map, become stick rates, throttle and the
//! arm switch, and arm and disarm requests to the safety master. Any
//! transport, parser or failsafe fault neutralizes the sticks and invalidates
//! the RC link, so a lost receiver fails closed.
//!
//! Requirements: INPUT-006

use crate::prelude::*;

/// What RC input last reported to the observer, and its link there.
struct RcReport<'a> {
    link: &'a mut LinkProducer<'static, observed::RcStatus>,
    status: &'a mut observed::RcStatus,
}

impl RcReport<'_> {
    fn send(&mut self, at: TimestampMicros) {
        self.link.push(at, *self.status);
    }
}

/// Carries out a fault's response (`safety::rc_fault_response`): report the
/// invalidation to the safety master, drop any partial frame if bytes were
/// lost, and neutralize the outputs; the observer sees the link invalid and
/// the outputs neutral, with the last frame's channels.
#[allow(clippy::too_many_arguments)]
fn respond_to_fault(
    fault: safety::RcInputFault,
    receiver: &mut RcReceiver,
    arm_qualifier: &mut safety::ArmQualifier,
    rates: &signals::RcRatesWriter,
    throttle: &signals::RcThrottleWriter,
    arm_high: &signals::RcArmHighWriter,
    reported_valid: &mut bool,
    observed: &mut RcReport<'_>,
    at: TimestampMicros,
    report: impl FnOnce(safety::SafetyEvent),
) -> safety::RcFaultResponse {
    let response = safety::rc_fault_response(fault);
    report(safety::SafetyEvent::RcLinkInvalid(response.invalidation));
    if response.reset_parser {
        receiver.reset();
    }
    safety::neutralize_rc_outputs(arm_qualifier, rates, throttle, arm_high);
    *reported_valid = false;
    *observed.status = observed::RcStatus {
        channels_us: observed.status.channels_us,
        ..observed::RcStatus::default()
    };
    observed.send(at);
    response
}

/// Read the receiver from the bound port, publish stick state, qualify the RC
/// link, and report arm and disarm requests and link invalidations to the
/// safety master. With no port bound the task ends at once: the RC link never
/// becomes valid, so the craft cannot arm.
///
/// The port's transport records faults on the stream rather than reporting
/// them itself, and a fault wakes this task even when no bytes follow, so it
/// invalidates the link here as soon as the transport sees the fault.
///
/// The channel map comes from the tuning profile, which a disarmed
/// configuration change can replace. A changed map may read the arm switch
/// from a channel that is already high, which would otherwise look like the
/// pilot flipping it; so a change invalidates the link like a lost receiver,
/// and arming needs the new arm channel seen low first.
///
/// What each fault does is decided by `safety::rc_fault_response`, which is
/// host-tested; this body only wires it.
#[ferroforge::task(
    local = [
        rc_port: Option<ferrowasp_stm32::uart_port::SerialPortEndpoint>,
        rc_receiver: RcReceiver,
        rc_map_in_use: Option<dt::RcChannelMap> = None,
        arm_qualifier: safety::ArmQualifier,
        rc_rates_writer: signals::RcRatesWriter,
        rc_throttle_writer: signals::RcThrottleWriter,
        rc_arm_high_writer: signals::RcArmHighWriter,
        rc_link_frame_writer: signals::RcLinkFrameWriter,
        rc_link_reported_valid: bool = false,
        rc_link_reported_invalidation_seq: u32 = 0,
        rc_status_link: LinkProducer<'static, observed::RcStatus>,
        rc_status: observed::RcStatus = observed::RcStatus {
            channels_us: [0; 16],
            valid: false,
            armable: false,
            arm_high: false,
            throttle: 0,
            rates_dps: [0; 3],
        },
    ],
    shared = [tuning_profile: dt::TuningProfile],
    spawn = [safety_master(event: safety::SafetyEvent)],
    monotonic = Mono,
)]
pub async fn rc_input(mut cx: rc_input::Context) {
    let Some(port) = cx.local.rc_port.as_mut() else {
        warn!("RC input has no serial port bound; the RC link stays invalid");
        return;
    };
    let mut observed = RcReport {
        link: cx.local.rc_status_link,
        status: cx.local.rc_status,
    };
    let now = || TimestampMicros(Mono::now().duration_since_epoch().to_micros());
    loop {
        let mut bytes = [0; stm32_uart::UART_RX_BUFFER_SIZE];
        let read_len = match port.reader.read(&mut bytes).await {
            Ok(read_len) => read_len,
            Err(_) => {
                let response = respond_to_fault(
                    safety::RcInputFault::StreamStopped,
                    cx.local.rc_receiver,
                    cx.local.arm_qualifier,
                    cx.local.rc_rates_writer,
                    cx.local.rc_throttle_writer,
                    cx.local.rc_arm_high_writer,
                    cx.local.rc_link_reported_valid,
                    &mut observed,
                    now(),
                    |event| {
                        let _ = cx.spawn.safety_master(event);
                    },
                );
                warn!("RC input stream stopped");
                if response.stop {
                    return;
                }
                continue;
            }
        };

        if let Some(event) = port.discontinuities.take_new() {
            respond_to_fault(
                safety::RcInputFault::Discontinuity {
                    dma: matches!(event.cause, Discontinuity::DmaError),
                },
                cx.local.rc_receiver,
                cx.local.arm_qualifier,
                cx.local.rc_rates_writer,
                cx.local.rc_throttle_writer,
                cx.local.rc_arm_high_writer,
                cx.local.rc_link_reported_valid,
                &mut observed,
                now(),
                |event| {
                    let _ = cx.spawn.safety_master(event);
                },
            );
            warn!("RC input discontinuity sequence {}", event.sequence);
        }

        for &byte in &bytes[..read_len] {
            let Some(frame) = cx.local.rc_receiver.push_byte(byte) else {
                continue;
            };
            let frame = match frame {
                Ok(frame) => frame,
                Err(_) => {
                    respond_to_fault(
                        safety::RcInputFault::ParseError,
                        cx.local.rc_receiver,
                        cx.local.arm_qualifier,
                        cx.local.rc_rates_writer,
                        cx.local.rc_throttle_writer,
                        cx.local.rc_arm_high_writer,
                        cx.local.rc_link_reported_valid,
                        &mut observed,
                        now(),
                        |event| {
                            let _ = cx.spawn.safety_master(event);
                        },
                    );
                    continue;
                }
            };

            if let Err(reason) = safety::classify_rc_frame_flags(frame.failsafe, frame.frame_lost) {
                respond_to_fault(
                    safety::RcInputFault::Flagged(reason),
                    cx.local.rc_receiver,
                    cx.local.arm_qualifier,
                    cx.local.rc_rates_writer,
                    cx.local.rc_throttle_writer,
                    cx.local.rc_arm_high_writer,
                    cx.local.rc_link_reported_valid,
                    &mut observed,
                    now(),
                    |event| {
                        let _ = cx.spawn.safety_master(event);
                    },
                );
                continue;
            }

            let (rc_rate_profile, rc_map) = cx
                .shared
                .tuning_profile
                .lock(|profile| (profile.rc_rates, profile.rc_map));
            if dt::RcChannelMap::replaced(cx.local.rc_map_in_use, rc_map) {
                respond_to_fault(
                    safety::RcInputFault::ChannelMapChanged,
                    cx.local.rc_receiver,
                    cx.local.arm_qualifier,
                    cx.local.rc_rates_writer,
                    cx.local.rc_throttle_writer,
                    cx.local.rc_arm_high_writer,
                    cx.local.rc_link_reported_valid,
                    &mut observed,
                    now(),
                    |event| {
                        let _ = cx.spawn.safety_master(event);
                    },
                );
                continue;
            }
            observed.status.channels_us =
                frame.channels.map(ferrowasp_input::rc_receiver::channel_us);
            let sticks = rc_map.sticks(&frame.channels);
            let rc_cmd = dt::remap_rc_channels_with_profile(
                sticks.roll,
                sticks.pitch,
                sticks.yaw,
                sticks.throttle,
                rc_rate_profile,
            );
            let arm_high = rc_map.arm(&frame.channels) > safety::ARM_THRESHOLD;
            let now_us = Mono::now().duration_since_epoch().to_micros();
            cx.local.rc_rates_writer.write(safety::RcRates {
                roll: rc_cmd.roll_dps as i16,
                pitch: rc_cmd.pitch_dps as i16,
                yaw: rc_cmd.yaw_dps as i16,
            });
            cx.local.rc_throttle_writer.write(rc_cmd.throttle);
            cx.local.rc_arm_high_writer.write(arm_high);
            let link = cx
                .local
                .rc_link_frame_writer
                .observe_healthy_frame(now_us, arm_high);
            if link.invalidation_sequence != *cx.local.rc_link_reported_invalidation_seq {
                *cx.local.rc_link_reported_invalidation_seq = link.invalidation_sequence;
                *cx.local.rc_link_reported_valid = false;
            }
            if link.valid && !*cx.local.rc_link_reported_valid {
                *cx.local.rc_link_reported_valid = true;
                info!("RC link valid after healthy-frame qualification");
            }
            observed.status.valid = link.valid;
            observed.status.armable = link.armable;
            observed.status.arm_high = arm_high;
            observed.status.throttle = rc_cmd.throttle;
            observed.status.rates_dps = [
                rc_cmd.roll_dps as i16,
                rc_cmd.pitch_dps as i16,
                rc_cmd.yaw_dps as i16,
            ];
            observed.send(TimestampMicros(now_us));

            if !link.valid || (arm_high && !link.armable) {
                cx.local.arm_qualifier.reset();
                continue;
            }

            if let Some(event) = cx.local.arm_qualifier.update(arm_high, now_us) {
                match event {
                    safety::SafetyEvent::ArmRequested => info!("RC Requests ARM!"),
                    safety::SafetyEvent::DisarmRequested => info!("RC Requests Disarm!"),
                    safety::SafetyEvent::ActuatorIdling
                    | safety::SafetyEvent::ArmingAborted(_)
                    | safety::SafetyEvent::RcLinkInvalid(_)
                    | safety::SafetyEvent::BenchMotor(_) => {}
                }

                cx.spawn.safety_master(event).ok();
            }
        }
    }
}
