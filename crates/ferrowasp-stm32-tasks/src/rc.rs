//! RC input: SBUS or CRSF frames from the port config bound to RC, read
//! through the configured channel map, become stick rates, throttle and the
//! arm switch, and arm and disarm requests to the safety master. Any
//! transport, parser or failsafe fault neutralizes the sticks and invalidates
//! the RC link, so a lost receiver fails closed.

use crate::prelude::*;

/// Neutral sticks, zero throttle, arm switch low, and the arm qualifier
/// reset: what the rest of the firmware sees while the link is not valid.
pub fn neutralize_rc_input(
    arm_qualifier: &mut safety::ArmQualifier,
    rates: &signals::RcRatesWriter,
    throttle: &signals::RcThrottleWriter,
    arm_high: &signals::RcArmHighWriter,
) {
    arm_qualifier.reset();
    rates.write(safety::RcRates::default());
    throttle.write(0);
    arm_high.write(false);
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
    loop {
        let mut bytes = [0; stm32_uart::UART_RX_BUFFER_SIZE];
        let read_len = match port.reader.read(&mut bytes).await {
            Ok(read_len) => read_len,
            Err(_) => {
                let _ = cx.spawn.safety_master(safety::SafetyEvent::RcLinkInvalid(
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                ));
                neutralize_rc_input(
                    cx.local.arm_qualifier,
                    cx.local.rc_rates_writer,
                    cx.local.rc_throttle_writer,
                    cx.local.rc_arm_high_writer,
                );
                *cx.local.rc_link_reported_valid = false;
                warn!("RC input stream stopped");
                return;
            }
        };

        if let Some(event) = port.discontinuities.take_new() {
            let reason = match event.cause {
                Discontinuity::DmaError => safety::RcLinkInvalidation::DmaError,
                _ => safety::RcLinkInvalidation::TransportDiscontinuity,
            };
            let _ = cx
                .spawn
                .safety_master(safety::SafetyEvent::RcLinkInvalid(reason));
            cx.local.rc_receiver.reset();
            neutralize_rc_input(
                cx.local.arm_qualifier,
                cx.local.rc_rates_writer,
                cx.local.rc_throttle_writer,
                cx.local.rc_arm_high_writer,
            );
            *cx.local.rc_link_reported_valid = false;
            warn!("RC input discontinuity sequence {}", event.sequence);
        }

        for &byte in &bytes[..read_len] {
            let Some(frame) = cx.local.rc_receiver.push_byte(byte) else {
                continue;
            };
            let frame = match frame {
                Ok(frame) => frame,
                Err(_) => {
                    let _ = cx.spawn.safety_master(safety::SafetyEvent::RcLinkInvalid(
                        safety::RcLinkInvalidation::ParserError,
                    ));
                    neutralize_rc_input(
                        cx.local.arm_qualifier,
                        cx.local.rc_rates_writer,
                        cx.local.rc_throttle_writer,
                        cx.local.rc_arm_high_writer,
                    );
                    *cx.local.rc_link_reported_valid = false;
                    continue;
                }
            };

            if let Err(reason) = safety::classify_rc_frame_flags(frame.failsafe, frame.frame_lost) {
                let _ = cx
                    .spawn
                    .safety_master(safety::SafetyEvent::RcLinkInvalid(reason));
                neutralize_rc_input(
                    cx.local.arm_qualifier,
                    cx.local.rc_rates_writer,
                    cx.local.rc_throttle_writer,
                    cx.local.rc_arm_high_writer,
                );
                *cx.local.rc_link_reported_valid = false;
                continue;
            }

            let (rc_rate_profile, rc_map) = cx
                .shared
                .tuning_profile
                .lock(|profile| (profile.rc_rates, profile.rc_map));
            let map_changed = cx
                .local
                .rc_map_in_use
                .replace(rc_map)
                .is_some_and(|in_use| in_use != rc_map);
            if map_changed {
                let _ = cx.spawn.safety_master(safety::SafetyEvent::RcLinkInvalid(
                    safety::RcLinkInvalidation::ChannelMapChanged,
                ));
                neutralize_rc_input(
                    cx.local.arm_qualifier,
                    cx.local.rc_rates_writer,
                    cx.local.rc_throttle_writer,
                    cx.local.rc_arm_high_writer,
                );
                *cx.local.rc_link_reported_valid = false;
                continue;
            }
            for (snapshot, raw) in crate::snapshots::RC_CHANNELS_US.iter().zip(frame.channels) {
                snapshot.store(
                    ferrowasp_flight::rc_receiver::channel_us(raw),
                    Ordering::Relaxed,
                );
            }
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

/// How often battery telemetry goes back to the receiver.
const RC_TELEMETRY_PERIOD_MS: u64 = 200;

/// Send battery telemetry back through the RC receiver, for the radio to
/// show. It owns the RC port's writer, so RC input never waits on a
/// transmit; it runs at a low priority and only reads snapshots.
///
/// Ends at once when the RC port has no transmit path: SBUS, or a port
/// without transmit DMA. Stops, with a warning, if the transmit path faults,
/// which leaves RC input untouched.
#[ferroforge::task(
    local = [rc_telemetry_writer: Option<stm32_memory::UartOwnedWriter<'static>>],
    monotonic = Mono,
)]
pub async fn rc_telemetry(cx: rc_telemetry::Context) {
    use embedded_io_async::Write;
    use ferrowasp_drivers::crsf;

    let Some(writer) = cx.local.rc_telemetry_writer.as_mut() else {
        return;
    };
    loop {
        let voltage_dv = crate::snapshots::BATTERY_VOLTAGE_V10_SNAPSHOT.load(Ordering::Relaxed);
        let current_ca = crate::snapshots::BATTERY_CURRENT_CA_SNAPSHOT.load(Ordering::Relaxed);
        let frame = crsf::encode_battery(crsf::BatteryTelemetry {
            voltage_dv: u16::try_from(voltage_dv).unwrap_or(u16::MAX),
            current_da: u16::try_from(current_ca.max(0) / 10).unwrap_or(u16::MAX),
            used_mah: 0,
            remaining_percent: 0,
        });
        if writer.write_all(&frame).await.is_err() {
            warn!("RC telemetry stopped: the RC port transmit path faulted");
            return;
        }
        Mono::delay(RC_TELEMETRY_PERIOD_MS.millis()).await;
    }
}
