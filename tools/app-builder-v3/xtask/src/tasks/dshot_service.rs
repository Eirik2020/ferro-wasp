use ferrowasp_core::safety::ActuatorPreparationReport;
use ferrowasp_tasks::esc_manager::{
    EscAckProducer, EscActuatorAck, EscActuatorRequest, EscOutput, EscRequestConsumer,
};
use fugit::MillisDurationU32;

use crate::{
    hardware_definitions::stm32f4::dshot_authoring::{
        DshotMotor, DshotMotorBank, DshotServiceEvent, DshotTelemetryRequestError,
    },
    rtic::task::Mono,
};

crate::reusable_task! {
    contract {
        local {
            /// Manager-to-actuator telemetry request consumer.
            requests: EscRequestConsumer,
            /// Actuator-to-manager exact-request acknowledgement producer.
            acknowledgements: EscAckProducer,
            /// At most one request being submitted to the next DShot frame.
            pending_request: Option<EscActuatorRequest>,
            /// Whether the pending request has been accepted by the bank.
            request_submitted: bool,
            /// Prevents repeated reports from a latched bank fault.
            fault_reported: bool,
        }
        shared {
            /// Indivisible four-lane DShot containment unit.
            bank: DshotMotorBank,
        }
        config {
            /// Independent physical-output gate.
            output_enabled: bool,
            /// Bounded service cadence.
            interval: MillisDurationU32,
        }
        spawns {
            /// Routes terminal backend events into the safety master.
            fault(report: ferrowasp_core::safety::ActuatorPreparationReport),
        }
    }

    /// Services DShot frames, command leases, and telemetry request acknowledgement.
    pub async fn dshot_service(mut cx: dshot_service::Context<'_>) {
        loop {
            if !cx.config.output_enabled {
                // Deliberately do not call `service`: even stop frames remain disabled.
                Mono::delay(cx.config.interval).await;
                continue;
            }
            let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
            if cx.local.pending_request.is_none() {
                *cx.local.pending_request = cx.local.requests.dequeue();
                *cx.local.request_submitted = false;
            }

            let (event, sent) = cx.shared.bank.lock(|bank| {
                if let Some(request) = *cx.local.pending_request
                    && !*cx.local.request_submitted
                {
                    let motor = match request.output {
                        EscOutput::Output1 => DshotMotor::Motor1,
                        EscOutput::Output2 => DshotMotor::Motor2,
                        EscOutput::Output3 => DshotMotor::Motor3,
                        EscOutput::Output4 => DshotMotor::Motor4,
                    };
                    match bank.request_telemetry(motor) {
                        Ok(()) => *cx.local.request_submitted = true,
                        Err(DshotTelemetryRequestError::Busy) => {}
                        Err(DshotTelemetryRequestError::Faulted) => {
                            *cx.local.pending_request = None;
                        }
                    }
                }
                let event = bank.service(now_ms);
                (event, bank.take_telemetry_request_sent())
            });

            if let (Some(request), Some(sent_motor)) = (*cx.local.pending_request, sent) {
                let expected = match request.output {
                    EscOutput::Output1 => DshotMotor::Motor1,
                    EscOutput::Output2 => DshotMotor::Motor2,
                    EscOutput::Output3 => DshotMotor::Motor3,
                    EscOutput::Output4 => DshotMotor::Motor4,
                };
                if sent_motor == expected {
                    let _ = cx.local.acknowledgements.enqueue(EscActuatorAck {
                        request,
                        started_at_ms: now_ms,
                    });
                } else {
                    let _ = fault::spawn(ActuatorPreparationReport::Faulted);
                }
                *cx.local.pending_request = None;
                *cx.local.request_submitted = false;
            }

            let terminal = matches!(event, DshotServiceEvent::LeaseExpired)
                || matches!(event, DshotServiceEvent::Faulted) && !*cx.local.fault_reported;
            if terminal {
                if matches!(event, DshotServiceEvent::Faulted) {
                    *cx.local.fault_reported = true;
                }
                if fault::spawn(ActuatorPreparationReport::Faulted).is_err() {
                    defmt::warn!("DShot terminal fault report rejected");
                }
            }
            Mono::delay(cx.config.interval).await;
        }
    }
}
