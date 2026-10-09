//! The ESC telemetry manager: polls each ESC for telemetry over the return
//! line config binds to it, and publishes what comes back.

use crate::prelude::*;
use crate::snapshots::SAFETY_ARMED;

/// Every `ESC_MANAGER_PERIOD_MS`: drain actuator acknowledgements and wire
/// bytes into the manager, request the next ESC's telemetry, and log a
/// summary once per thousand periods. It never commands a motor.
///
/// Which logical motor each physical ESC output drives is the board's, so it
/// is configuration. With no port bound the task ends at once and no ESC is
/// asked for telemetry.
#[ferroforge::task(
    local = [
        esc_telemetry_port: Option<ferrowasp_stm32::uart_port::SerialPortEndpoint>,
        esc_manager_state: esc::EscManager,
        esc_request_producer: esc::EscRequestProducer,
        esc_ack_consumer: esc::EscAckConsumer,
        esc_telemetry_update_producer: esc::EscTelemetryUpdateProducer,
        report_ticks: u16 = 0,
    ],
    config = [esc_output_to_logical_motor: [u8; 4]],
    monotonic = Mono,
)]
pub async fn esc_manager_task(cx: esc_manager_task::Context) {
    let Some(port) = cx.local.esc_telemetry_port.as_mut() else {
        return;
    };
    loop {
        let release = Mono::now();
        let next_release = release + u64::from(ESC_MANAGER_PERIOD_MS).millis();
        let now_ms = release.duration_since_epoch().to_millis();

        if port.discontinuities.take_new().is_some() {
            cx.local.esc_manager_state.record_wire_discontinuity();
        }
        while let Some(ack) = cx.local.esc_ack_consumer.dequeue() {
            if let esc::EscAckOutcome::Sample(update) =
                cx.local.esc_manager_state.on_actuator_ack(ack)
            {
                let _ = cx.local.esc_telemetry_update_producer.enqueue(update);
            }
        }
        let mut bytes = [0; stm32_uart::UART_RX_BUFFER_SIZE];
        while let Ok(read_len @ 1..) = port.reader.try_read(&mut bytes) {
            if port
                .reader
                .current_chunk()
                .is_some_and(|chunk| chunk.uart_error_seen)
            {
                cx.local.esc_manager_state.record_wire_discontinuity();
            }
            for byte in &bytes[..read_len] {
                if let Some(update) = cx.local.esc_manager_state.push_wire_byte(*byte, now_ms) {
                    let _ = cx.local.esc_telemetry_update_producer.enqueue(update);
                }
            }
        }

        cx.local.esc_manager_state.refresh_wire_stats();
        if let Some(timeout) = cx.local.esc_manager_state.poll_timeout(now_ms) {
            match timeout {
                esc::EscManagerTimeout::ActuatorAck(request) => warn!(
                    "Foxeer ESC telemetry stopped after actuator acknowledgement timeout for physical output {} (logical M{}), request {}; resumes after a quiet window while disarmed",
                    request.output.index() + 1,
                    CONFIG::ESC_OUTPUT_TO_LOGICAL_MOTOR[request.output.index()],
                    request.sequence
                ),
                esc::EscManagerTimeout::TelemetryResponse(request) => warn!(
                    "Foxeer ESC telemetry stopped after response timeout for physical output {} (logical M{}), request {}; resumes after a quiet window while disarmed",
                    request.output.index() + 1,
                    CONFIG::ESC_OUTPUT_TO_LOGICAL_MOTOR[request.output.index()],
                    request.sequence
                ),
            }
        }

        if cx
            .local
            .esc_manager_state
            .try_recover(now_ms, SAFETY_ARMED.load(Ordering::Acquire))
        {
            info!("Foxeer ESC telemetry resumed after a quiet window; old samples dropped");
        }

        if let Some(request) = cx.local.esc_manager_state.next_request(now_ms)
            && cx.local.esc_request_producer.enqueue(request).is_ok()
        {
            let marked = cx
                .local
                .esc_manager_state
                .mark_request_queued(request, now_ms);
            debug_assert!(marked);
        }

        *cx.local.report_ticks = cx.local.report_ticks.wrapping_add(1);
        if *cx.local.report_ticks >= 1_000 {
            *cx.local.report_ticks = 0;
            let stats = cx.local.esc_manager_state.stats();
            for (index, observation) in cx.local.esc_manager_state.samples().iter().enumerate() {
                if let Some(observation) = observation {
                    info!(
                        "Foxeer physical ESC output {} (logical M{}) telemetry: {}.{}V {}.{}A {}mAh {}00eRPM {}C",
                        index + 1,
                        CONFIG::ESC_OUTPUT_TO_LOGICAL_MOTOR[index],
                        observation.sample.voltage_cv / 100,
                        observation.sample.voltage_cv % 100,
                        observation.sample.current_ca / 100,
                        observation.sample.current_ca % 100,
                        observation.sample.consumption_mah,
                        observation.sample.erpm_div100,
                        observation.sample.temperature_c
                    );
                }
            }
            info!(
                "Foxeer ESC telemetry manager: queued/started {}/{}, faulted {}, ack timeouts {}, response timeouts {}, recoveries {}, mismatched acks {}, unsolicited {}, valid {}, CRC failures {}, discarded {}",
                stats.requests_queued,
                stats.requests_started,
                cx.local.esc_manager_state.is_faulted(),
                stats.actuator_ack_timeouts,
                stats.telemetry_response_timeouts,
                stats.recoveries,
                stats.mismatched_acks,
                stats.unsolicited_frames,
                stats.wire.valid_frames,
                stats.wire.crc_failures,
                stats.wire.discarded_bytes
            );
        }

        Mono::delay_until(next_release).await;
    }
}
