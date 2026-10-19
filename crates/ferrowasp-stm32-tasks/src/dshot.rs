//! DShot motor output: the service loop that sends frames and telemetry
//! requests, and the DMA completion of each motor's frame.

use crate::prelude::backend::dshot;
use crate::prelude::backend::dshot::DshotMotors;
use crate::prelude::*;
use crate::snapshots::{
    DSHOT_ACTIVE_LANES, DSHOT_LAST_RELEASE_MS, DSHOT_REPLIES_INVALID, DSHOT_REPLIES_VALID,
    DSHOT_REPLY_ERPM_DIV100,
};

/// How often the service reports the bank's state to the log.
const DSHOT_REPORT_PERIOD_MS: u64 = 2_000;

pub fn service_dshot_dma_irq<Bank>(bank: &mut Bank, motor: dshot::DshotMotor, stream: u8)
where
    Bank: rtic::Mutex,
    Bank::T: DshotMotors,
{
    let event = bank.lock(|dshot| dshot.on_dma_interrupt(motor));
    if event == dshot::DshotInterruptEvent::Spurious {
        warn!("DShot received spurious DMA2 Stream{} interrupt", stream);
    }
}

/// One motor's DShot DMA transfer complete. Selected once per motor: which
/// motor, and which DMA2 stream carries it for the log, are the board's.
#[ferroforge::task(
    bounds = [dshot_motors: dshot::DshotMotors],
    shared = [dshot_motors],
    config = [motor: dshot::DshotMotor, stream: u8],
)]
pub fn dshot_dma_complete(mut cx: dshot_dma_complete::Context) {
    service_dshot_dma_irq(&mut cx.shared.dshot_motors, CONFIG::MOTOR, CONFIG::STREAM);
}

/// One release of the motor bank (ADR-0046): pass one ESC telemetry request
/// to the bank, service it - which starts a frame set, and stops the motors
/// itself when a command lease expires or the bank faults - acknowledge a
/// sent request to the ESC manager, and ask the safety master to disarm after
/// an expiry or fault. It does not choose motor commands; `actuator_output`
/// does.
///
/// It is the only frame starter. `actuator_output` releases it after each
/// command it accepts, so frames follow the control loop; `dshot_fallback`
/// releases it whenever no frame set has started for
/// `DSHOT_SERVICE_PERIOD_MS`. A release that finds it already pending or
/// running folds into that run.
///
/// Which DShot motor each ESC output is wired to is the board's, so it is
/// configuration.
#[ferroforge::task(
    bounds = [dshot_motors: dshot::DshotMotors],
    shared = [dshot_motors],
    local = [
        fault_reported: bool = false,
        disarm_pending: bool = false,
        last_report_ms: u64 = 0,
        // Each lane's latest valid eRPM/100 from bidirectional replies.
        reply_erpm_div100: [Option<u32>; 4] = [None; 4],
        esc_request_consumer: esc::EscRequestConsumer,
        esc_ack_producer: esc::EscAckProducer,
        esc_actuator_request: Option<esc::EscActuatorRequest> = None,
        esc_actuator_request_submitted: bool = false,
        esc_actuator_request_taken_ms: u64 = 0,
    ],
    spawn = [safety_master(event: safety::SafetyEvent)],
    config = [dshot_motor_for_output: fn(esc::EscOutput) -> dshot::DshotMotor],
    monotonic = Mono,
)]
pub async fn dshot_service(mut cx: dshot_service::Context) {
    let now_ms = Mono::now().duration_since_epoch().to_millis();
    if cx.local.esc_actuator_request.is_none() {
        *cx.local.esc_actuator_request = cx.local.esc_request_consumer.dequeue();
        *cx.local.esc_actuator_request_submitted = false;
        *cx.local.esc_actuator_request_taken_ms = now_ms;
    }
    // The ESC manager gives up on an acknowledgement after the same
    // time. Drop the request unsent, so no frame it would cause can be
    // credited to a later one.
    let expired = cx.local.esc_actuator_request.is_some()
        && now_ms.saturating_sub(*cx.local.esc_actuator_request_taken_ms)
            >= u64::from(esc::ESC_REQUEST_EXPIRY_MS);

    let (event, telemetry_sent) = cx.shared.dshot_motors.lock(|dshot| {
        if expired {
            if *cx.local.esc_actuator_request_submitted {
                dshot.cancel_telemetry_request();
            }
            *cx.local.esc_actuator_request = None;
            *cx.local.esc_actuator_request_submitted = false;
        }
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

        let frames_before = dshot.stats().frames_started;
        let event = dshot.service(now_ms);
        if let Some(replies) = dshot.take_replies() {
            for (latest, reply) in cx.local.reply_erpm_div100.iter_mut().zip(replies) {
                if let Ok(dshot::DshotReply::Erpm { erpm_div100 }) = reply {
                    *latest = Some(erpm_div100);
                }
            }
            // For the `esc` command, by physical ESC output, so each line
            // matches that output's UART telemetry.
            let stats = dshot.reply_stats();
            for output in esc::EscOutput::ALL {
                let lane = CONFIG::DSHOT_MOTOR_FOR_OUTPUT(output).index();
                let slot = output.index();
                DSHOT_REPLY_ERPM_DIV100[slot].store(
                    cx.local.reply_erpm_div100[lane].unwrap_or(u32::MAX),
                    Ordering::Relaxed,
                );
                DSHOT_REPLIES_VALID[slot].store(stats.valid[lane], Ordering::Relaxed);
                DSHOT_REPLIES_INVALID[slot].store(stats.invalid[lane], Ordering::Relaxed);
            }
        }
        if dshot.stats().frames_started != frames_before {
            // Wrapping milliseconds: the fallback compares differences only.
            // A release that found the bank busy started nothing, so it does
            // not hold the fallback off.
            DSHOT_LAST_RELEASE_MS.store(now_ms as u32, Ordering::Relaxed);
        }
        let telemetry_sent = dshot.take_telemetry_request_sent();
        let active =
            dshot
                .requested_values()
                .iter()
                .enumerate()
                .fold(0u8, |mask, (lane, value)| {
                    if *value != 0 {
                        mask | (1 << lane)
                    } else {
                        mask
                    }
                });
        DSHOT_ACTIVE_LANES.store(active, Ordering::Relaxed);
        (event, telemetry_sent)
    });
    if let (Some(request), Some(sent_motor)) = (*cx.local.esc_actuator_request, telemetry_sent) {
        if sent_motor == CONFIG::DSHOT_MOTOR_FOR_OUTPUT(request.output) {
            let ack = esc::EscActuatorAck {
                request,
                started_at_ms: now_ms,
            };
            if cx.local.esc_ack_producer.enqueue(ack).is_err() {
                warn!("ESC actuator acknowledgement queue full");
            }
        } else {
            warn!("ESC telemetry acknowledgement output mismatch");
        }
        *cx.local.esc_actuator_request = None;
        *cx.local.esc_actuator_request_submitted = false;
    }

    match event {
        dshot::DshotServiceEvent::LeaseExpired => {
            warn!("DShot command lease expired; stop frames selected");
            *cx.local.disarm_pending = true;
        }
        dshot::DshotServiceEvent::Faulted if !*cx.local.fault_reported => {
            warn!("DShot bank faulted; all outputs forced low");
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

    if now_ms.saturating_sub(*cx.local.last_report_ms) >= DSHOT_REPORT_PERIOD_MS {
        *cx.local.last_report_ms = now_ms;
        let (requested, stats, bidirectional, replies) = cx.shared.dshot_motors.lock(|dshot| {
            (
                dshot.requested_values(),
                dshot.stats(),
                dshot.is_bidirectional(),
                dshot.reply_stats(),
            )
        });
        info!(
            "DShot values [{}, {}, {}, {}], sets {}/{}, lanes [{}, {}, {}, {}], busy {}, expired {}, timeouts {}, faults {}, at {} ms",
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
        if bidirectional {
            // -1: no valid reply from that lane yet.
            let erpm = cx
                .local
                .reply_erpm_div100
                .map(|erpm| erpm.map_or(-1, i64::from));
            info!(
                "DShot replies eRPM/100 [{}, {}, {}, {}], valid [{}, {}, {}, {}], invalid [{}, {}, {}, {}]",
                erpm[0],
                erpm[1],
                erpm[2],
                erpm[3],
                replies.valid[0],
                replies.valid[1],
                replies.valid[2],
                replies.valid[3],
                replies.invalid[0],
                replies.invalid[1],
                replies.invalid[2],
                replies.invalid[3]
            );
        }
    }
}

/// Keeps frames going when no command releases them (ADR-0046): while
/// disarmed, during guarded idle, or after the control loop stops. Every
/// `DSHOT_SERVICE_PERIOD_MS` it releases the service if no frame set has
/// started in that time, so the ESCs always see frames and an unrenewed lease
/// is always found. Absolute deadlines keep its own cadence from drifting.
#[ferroforge::task(
    spawn = [dshot_service()],
    monotonic = Mono,
)]
pub async fn dshot_fallback(mut cx: dshot_fallback::Context) {
    let mut next_check = Mono::now();
    loop {
        let now_ms = next_check.duration_since_epoch().to_millis() as u32;
        if dshot::fallback_release_due(now_ms, DSHOT_LAST_RELEASE_MS.load(Ordering::Relaxed)) {
            // Already pending or running: that run is the release.
            let _ = cx.spawn.dshot_service();
        }
        next_check += u64::from(dshot::DSHOT_SERVICE_PERIOD_MS).millis();
        Mono::delay_until(next_check).await;
    }
}
