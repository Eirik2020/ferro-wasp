use ferrowasp_core::safety::ActuatorPreparationReport;
use ferrowasp_tasks::esc_manager::{
    EscAckConsumer, EscAckOutcome, EscManager, EscRequestProducer, EscTelemetryUpdateProducer,
};
use fugit::MillisDurationU32;

use crate::{
    hardware_definitions::stm32f4::dshot_authoring::{UartRxParserSide, UartRxReadStatus},
    rtic::task::Mono,
};

crate::reusable_task! {
    contract {
        local {
            /// Parser side of the USART1 DMA buffer pool.
            parser: UartRxParserSide,
            /// Legacy request association, parsing, and qualification sample state.
            manager: EscManager,
            /// Manager-to-DShot request producer.
            requests: EscRequestProducer,
            /// DShot-to-manager exact-request acknowledgement consumer.
            acknowledgements: EscAckConsumer,
            /// Fresh physical-output observation producer for the actuator.
            updates: EscTelemetryUpdateProducer,
            /// Bounded diagnostic cadence state.
            report_ticks: u16,
        }
        shared {
            /// Sticky DMA/UART discontinuity observation.
            discontinuity: bool,
        }
        config {
            /// Independent physical-output and telemetry-request gate.
            output_enabled: bool,
            /// Bounded manager cadence.
            interval: MillisDurationU32,
        }
        spawns {
            /// Routes association timeouts into the safety master.
            fault(report: ferrowasp_core::safety::ActuatorPreparationReport),
        }
    }

    /// Parses bounded legacy telemetry and maintains exact request association.
    pub async fn esc_manager(mut cx: esc_manager::Context<'_>) {
        loop {
            let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
            let had_discontinuity = cx.shared.discontinuity.lock(|flag| {
                let value = *flag;
                *flag = false;
                value
            });
            if had_discontinuity {
                cx.local.manager.record_wire_discontinuity();
            }

            for _ in 0..ferrowasp_tasks::esc_manager::ESC_ACK_QUEUE_CAPACITY {
                let Some(ack) = cx.local.acknowledgements.dequeue() else {
                    break;
                };
                if let EscAckOutcome::Sample(update) = cx.local.manager.on_actuator_ack(ack) {
                    let _ = cx.local.updates.enqueue(update);
                }
            }

            let mut bytes = [0; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES];
            for _ in 0..ferrowasp_stm32f4::memory::UART_RX_BUFFERS_PER_PORT {
                match cx.local.parser.read_chunk_with_status(&mut bytes) {
                    UartRxReadStatus::NoChunk => break,
                    UartRxReadStatus::RecycleError => {
                        cx.local.manager.record_wire_discontinuity();
                        let _ = fault::spawn(ActuatorPreparationReport::Faulted);
                        break;
                    }
                    UartRxReadStatus::Chunk {
                        len,
                        uart_error_seen,
                    } => {
                        if uart_error_seen {
                            cx.local.manager.record_wire_discontinuity();
                        }
                        for byte in &bytes[..len] {
                            if let Some(update) = cx.local.manager.push_wire_byte(*byte, now_ms) {
                                let _ = cx.local.updates.enqueue(update);
                            }
                        }
                    }
                }
            }

            cx.local.manager.refresh_wire_stats();
            if cx.local.manager.poll_timeout(now_ms).is_some()
                && fault::spawn(ActuatorPreparationReport::Faulted).is_err()
            {
                defmt::warn!("ESC manager timeout fault report rejected");
            }
            if cx.config.output_enabled
                && let Some(request) = cx.local.manager.next_request(now_ms)
                && cx.local.requests.enqueue(request).is_ok()
            {
                let _ = cx.local.manager.mark_request_queued(request, now_ms);
            }

            *cx.local.report_ticks = cx.local.report_ticks.wrapping_add(1);
            Mono::delay(cx.config.interval).await;
        }
    }
}
