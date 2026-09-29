//! RC input: SBUS frames from the USART2 stream become stick rates,
//! throttle and the arm switch, and arm and disarm requests to the safety
//! master. Any transport, parser or failsafe fault neutralizes the sticks and
//! invalidates the RC link, so a lost receiver fails closed.

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

/// Read SBUS from USART2, publish stick state, qualify the RC link, and
/// report arm and disarm requests and link invalidations to the safety
/// master.
#[ferroforge::task(
    local = [
        rc_rx_reader: stm32_memory::UartOwnedReader<'static>,
        rc_rx_discontinuities: stm32_memory::UartOwnedDiscontinuities<'static>,
        sbus: StreamingParser,
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
    loop {
        let mut bytes = [0; stm32_uart::UART_RX_BUFFER_SIZE];
        let read_len = match cx.local.rc_rx_reader.read(&mut bytes).await {
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
                warn!("USART2 owned RX reader stopped");
                return;
            }
        };

        if let Some(event) = cx.local.rc_rx_discontinuities.take_new() {
            let _ = cx.spawn.safety_master(safety::SafetyEvent::RcLinkInvalid(
                safety::RcLinkInvalidation::TransportDiscontinuity,
            ));
            cx.local.sbus.reset();
            neutralize_rc_input(
                cx.local.arm_qualifier,
                cx.local.rc_rates_writer,
                cx.local.rc_throttle_writer,
                cx.local.rc_arm_high_writer,
            );
            *cx.local.rc_link_reported_valid = false;
            warn!("USART2 RX discontinuity sequence {}", event.sequence);
        }

        for packet in cx.local.sbus.push_bytes(&bytes[..read_len]) {
            let pkt = match packet {
                Ok(packet) => packet,
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

            if let Err(reason) =
                safety::classify_rc_frame_flags(pkt.flags.failsafe, pkt.flags.frame_lost)
            {
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

            let rc_rate_profile = cx.shared.tuning_profile.lock(|profile| profile.rc_rates);
            let rc_cmd = dt::remap_rc_channels_with_profile(
                pkt.channels[0],
                pkt.channels[1],
                pkt.channels[3],
                pkt.channels[2],
                rc_rate_profile,
            );
            let arm_high = pkt.channels[8] > safety::ARM_THRESHOLD;
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
                    | safety::SafetyEvent::RcLinkInvalid(_) => {}
                }

                cx.spawn.safety_master(event).ok();
            }
        }
    }
}
