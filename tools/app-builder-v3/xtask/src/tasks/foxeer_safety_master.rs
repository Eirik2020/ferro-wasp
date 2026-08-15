use embedded_io_async::Read;
use ferrowasp_core::{
    safety::{
        ActuatorAuthority, ActuatorGuardReport, ActuatorPreparationReport, PreArmHealthReport,
        RcLinkInvalidation,
    },
    safety_channel::{SafetyConsumer, SafetyProducer},
};
use ferrowasp_io_core::serial::RcInputSnapshot;
use ferrowasp_stm32f4::memory::{UartOwnedDiscontinuities, UartOwnedReader};
use ferrowasp_tasks::{drone_toolbox as dt, service_telemetry::FlightServiceTelemetry};
use fugit::MillisDurationU32;

use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// Owned byte reader exported by the boot-selected SBUS endpoint.
            reader: UartOwnedReader<'static>,
            /// Out-of-band DMA, overflow, framing, and reconfiguration evidence.
            discontinuities: UartOwnedDiscontinuities<'static>,
            /// Exclusive output-inhibited RC-link and arming state machine.
            foxeer_safety_state: ferrowasp_tasks::foxeer_safety::FoxeerSafetyMaster,
            /// Exclusive stream of safety-qualified RC snapshots to control.
            control: SafetyProducer<'static, RcInputSnapshot>,
            /// Observation-only IMU health evidence produced by control.
            health: SafetyConsumer<'static, PreArmHealthReport>,
            /// Exclusive safety-to-actuator authority publication path.
            actuator_guards: SafetyProducer<'static, ActuatorGuardReport>,
            /// Exclusive actuator preparation completion path.
            actuator_completions: SafetyConsumer<'static, ActuatorPreparationReport>,
            /// Exclusive physical-service fault path.
            actuator_faults: SafetyConsumer<'static, ActuatorPreparationReport>,
        }
        shared {
            /// Latest safety-qualified SBUS channel 3 value.
            channel3: u32,
            /// Observation-only copy of safety-owned RC and arming state.
            telemetry: FlightServiceTelemetry,
            /// Validated RC-rate mapping profile.
            tuning: dt::TuningProfile,
        }
        config {
            /// Maximum interval between timeout and health revalidation.
            poll_interval: MillisDurationU32,
        }
        spawns {
            /// Requests a bounded operation from the safety-owned actuator task.
            actuator_request(request: ferrowasp_core::safety::ActuatorCmd),
        }
    }

    /// Owns golden RC-link recovery and all arming/permit decisions.
    pub async fn foxeer_safety_master(mut cx: foxeer_safety_master::Context<'_>) {
        loop {
            let mut bytes = [0; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES];
            let read = Mono::timeout_after(
                cx.config.poll_interval,
                cx.local.reader.read(&mut bytes),
            )
            .await;
            let now_us = Mono::now().duration_since_epoch().to_micros() as u32;

            let mut apply = |output: ferrowasp_tasks::foxeer_safety::SafetyMasterOutput| {
                if let Some(snapshot) = output.control_snapshot
                {
                    let tuning = cx.shared.tuning.lock(|profile| *profile);
                    let mapped = dt::remap_rc_channels_with_profile(
                        snapshot.channels[0],
                        snapshot.channels[1],
                        snapshot.channels[3],
                        snapshot.channels[2],
                        tuning.sanitized().rc_rates,
                    );
                    cx.shared.telemetry.lock(|telemetry| {
                        telemetry.rc_link_valid = true;
                        telemetry.rc_rates_dps = [
                            mapped.roll_dps as i16,
                            mapped.pitch_dps as i16,
                            mapped.yaw_dps as i16,
                        ];
                        telemetry.rc_throttle = mapped.throttle;
                    });
                    if cx.local.control.try_send(snapshot).is_err() {
                        defmt::warn!("safety-to-control RC channel full; sample rejected");
                    }
                }
                if let Some(value) = output.channel3 {
                    cx.shared.channel3.lock(|channel| *channel = u32::from(value));
                }
                if let Some(request) = output.actuator
                    && actuator_request::spawn(request).is_err()
                {
                    defmt::warn!("safety actuator request rejected");
                }
                if output.event.is_some() {
                    defmt::info!("safety-master state transition");
                }
            };

            while let Some(report) = cx.local.health.try_receive() {
                apply(
                    cx.local
                        .foxeer_safety_state
                        .observe_control_health(report, now_us),
                );
            }

            while let Some(report) = cx.local.actuator_completions.try_receive() {
                apply(cx.local.foxeer_safety_state.actuator_report(report, now_us));
            }
            while let Some(report) = cx.local.actuator_faults.try_receive() {
                apply(cx.local.foxeer_safety_state.actuator_report(report, now_us));
            }

            if let Some(discontinuity) = cx.local.discontinuities.take_new() {
                let reason = if matches!(
                    discontinuity.cause,
                    ferrowasp_io_core::serial::Discontinuity::DmaError
                ) {
                    RcLinkInvalidation::DmaError
                } else {
                    RcLinkInvalidation::TransportDiscontinuity
                };
                apply(cx.local.foxeer_safety_state.transport_fault(reason));
            }

            match read {
                Ok(Ok(read_len)) => cx.local.foxeer_safety_state.consume_bytes(
                    &bytes[..read_len],
                    now_us,
                    &mut apply,
                ),
                Ok(Err(_)) => {
                    apply(cx.local.foxeer_safety_state.transport_fault(
                        RcLinkInvalidation::TransportDiscontinuity,
                    ));
                    defmt::warn!("SBUS reader stopped after a transport fault");
                    return;
                }
                Err(_) => apply(cx.local.foxeer_safety_state.poll(now_us)),
            }

            drop(apply);
            let link = cx.local.foxeer_safety_state.link_status(now_us);
            let armed = matches!(
                cx.local.foxeer_safety_state.arm_state(),
                ferrowasp_core::safety::ArmingState::Armed
            );
            cx.shared.telemetry.lock(|telemetry| {
                telemetry.rc_link_valid = link.valid;
                telemetry.armed = armed;
                if !link.valid {
                    telemetry.rc_rates_dps = [0; 3];
                    telemetry.rc_throttle = 0;
                }
            });

            let guard = cx
                .local
                .foxeer_safety_state
                .actuator_guard_report(now_us);
            if guard.authority != ActuatorAuthority::Inhibited
                && cx.local.actuator_guards.try_send(guard).is_err()
            {
                let _ = cx.local.foxeer_safety_state.actuator_report(
                    ActuatorPreparationReport::Faulted,
                    now_us,
                );
                defmt::warn!("safety-to-actuator authority channel full");
            }
        }
    }
}
