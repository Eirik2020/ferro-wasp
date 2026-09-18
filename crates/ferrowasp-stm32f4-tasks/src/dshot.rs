//! DShot motor output: the service loop that sends frames and telemetry
//! requests, and the DMA completion of each motor's frame.

use crate::prelude::*;
use ferrowasp_stm32f4::dshot;

pub fn service_dshot_dma_irq(
    bank: &mut impl rtic::Mutex<T = dshot::DshotMotorBank>,
    motor: dshot::DshotMotor,
    stream: u8,
) {
    let event = bank.lock(|dshot| dshot.on_dma_interrupt(motor));
    if event == dshot::DshotInterruptEvent::Spurious {
        warn!(
            "Foxeer DShot received spurious DMA2 Stream{} interrupt",
            stream
        );
    }
}

/// One motor's DShot DMA transfer complete. Selected once per motor: which
/// motor, and which DMA2 stream carries it for the log, are the board's.
#[ferroforge::task(
    shared = [dshot_motors: dshot::DshotMotorBank],
    config = [motor: dshot::DshotMotor, stream: u8],
)]
pub fn dshot_dma_complete(mut cx: dshot_dma_complete::Context) {
    service_dshot_dma_irq(&mut cx.shared.dshot_motors, CONFIG::MOTOR, CONFIG::STREAM);
}

/// Every `DSHOT_SERVICE_PERIOD_MS`: pass one ESC telemetry request to the
/// motor bank, service the bank - which stops the motors itself when a
/// command lease expires or the bank faults - acknowledge a sent request to
/// the ESC manager, and ask the safety master to disarm after an expiry or
/// fault. It does not choose motor commands; `actuator_output` does.
///
/// Which DShot motor each ESC output is wired to is the board's, so it is
/// configuration.
#[ferroforge::task(
    shared = [dshot_motors: dshot::DshotMotorBank],
    local = [
        fault_reported: bool = false,
        disarm_pending: bool = false,
        report_ticks: u16 = 0,
        esc_request_consumer: esc::EscRequestConsumer,
        esc_ack_producer: esc::EscAckProducer,
        esc_actuator_request: Option<esc::EscActuatorRequest> = None,
        esc_actuator_request_submitted: bool = false,
    ],
    spawn = [safety_master(event: safety::SafetyEvent)],
    config = [dshot_motor_for_output: fn(esc::EscOutput) -> dshot::DshotMotor],
    monotonic = Mono,
)]
pub async fn dshot_service(mut cx: dshot_service::Context) {
    loop {
        let release = Mono::now();
        let next_release = release + dshot::DSHOT_SERVICE_PERIOD_MS.millis();
        let now_ms = release.duration_since_epoch().to_millis();
        if cx.local.esc_actuator_request.is_none() {
            *cx.local.esc_actuator_request = cx.local.esc_request_consumer.dequeue();
            *cx.local.esc_actuator_request_submitted = false;
        }

        let (event, telemetry_sent) = cx.shared.dshot_motors.lock(|dshot| {
            if let Some(request) = *cx.local.esc_actuator_request
                && !*cx.local.esc_actuator_request_submitted
            {
                let result = match request.operation {
                    esc::EscOperation::RequestTelemetry => {
                        dshot.request_telemetry(CONFIG::DSHOT_MOTOR_FOR_OUTPUT(request.output))
                    }
                };
                match result {
                    Ok(()) => *cx.local.esc_actuator_request_submitted = true,
                    Err(dshot::DshotTelemetryRequestError::Busy) => {}
                    Err(dshot::DshotTelemetryRequestError::Faulted) => {
                        *cx.local.esc_actuator_request = None;
                    }
                }
            }

            let event = dshot.service(now_ms);
            let telemetry_sent = dshot.take_telemetry_request_sent();
            (event, telemetry_sent)
        });
        if let (Some(request), Some(sent_motor)) = (*cx.local.esc_actuator_request, telemetry_sent)
        {
            if sent_motor == CONFIG::DSHOT_MOTOR_FOR_OUTPUT(request.output) {
                let ack = esc::EscActuatorAck {
                    request,
                    started_at_ms: now_ms,
                };
                if cx.local.esc_ack_producer.enqueue(ack).is_err() {
                    warn!("Foxeer ESC actuator acknowledgement queue full");
                }
            } else {
                warn!("Foxeer ESC telemetry acknowledgement output mismatch");
            }
            *cx.local.esc_actuator_request = None;
            *cx.local.esc_actuator_request_submitted = false;
        }

        match event {
            dshot::DshotServiceEvent::LeaseExpired => {
                warn!("Foxeer DShot command lease expired; stop frames selected");
                *cx.local.disarm_pending = true;
            }
            dshot::DshotServiceEvent::Faulted if !*cx.local.fault_reported => {
                warn!("Foxeer DShot bank faulted; all outputs forced low");
                *cx.local.fault_reported = true;
                *cx.local.disarm_pending = true;
            }
            _ => {}
        }

        if *cx.local.disarm_pending
            && cx
                .spawn
                .safety_master(safety::SafetyEvent::DisarmRequested)
                .is_ok()
        {
            *cx.local.disarm_pending = false;
        }

        *cx.local.report_ticks = cx.local.report_ticks.wrapping_add(1);
        if *cx.local.report_ticks >= 1_000 {
            *cx.local.report_ticks = 0;
            let (requested, stats) = cx
                .shared
                .dshot_motors
                .lock(|dshot| (dshot.requested_values(), dshot.stats()));
            info!(
                "Foxeer DShot values [{}, {}, {}, {}], sets {}/{}, lanes [{}, {}, {}, {}], busy {}, expired {}, timeouts {}, faults {}, at {} ms",
                requested[0],
                requested[1],
                requested[2],
                requested[3],
                stats.frames_completed,
                stats.frames_started,
                stats.lane_completions[0],
                stats.lane_completions[1],
                stats.lane_completions[2],
                stats.lane_completions[3],
                stats.busy_skips,
                stats.lease_expiries,
                stats.frame_timeouts,
                stats.dma_faults,
                now_ms
            );
        }
        Mono::delay_until(next_release).await;
    }
}
