use embedded_io_async::{Read, Write};
use ferrowasp_stm32f4::memory::{UartOwnedDiscontinuities, UartOwnedReader, UartOwnedWriter};
use ferrowasp_tasks::{
    drone_toolbox as dt,
    osd::{OsdStickRates, OsdTask},
    service_telemetry::FlightServiceTelemetry,
};
use fugit::MillisDurationU32;

use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// Owned byte reader exported by the MSP UART endpoint.
            reader: UartOwnedReader<'static>,
            /// Out-of-band UART4 DMA and framing evidence.
            discontinuities: UartOwnedDiscontinuities<'static>,
            /// Bounded UART4 transmit writer with DMA completion backpressure.
            writer: UartOwnedWriter<'static>,
            /// MSP parser, responder, overlay, and disarmed menu state.
            osd: OsdTask,
            /// Reused fixed-size MSP transmit buffer.
            output: [u8; ferrowasp_mspv1::OSD_TX_BUFFER_LEN],
            /// Ten-millisecond ticks accumulated to the 100 ms refresh cadence.
            refresh_tick: u8,
            /// Sticky transport health latch.
            tx_healthy: bool,
        }
        shared {
            /// Observation-only flight telemetry, including ADC freshness.
            telemetry: FlightServiceTelemetry,
            /// Disarmed runtime tuning profile.
            tuning: dt::TuningProfile,
            /// Sequence consumed by the control task when tuning changes.
            tuning_seq: u32,
        }
        config {
            /// Bounded receive/menu service cadence.
            interval: MillisDurationU32,
        }
        spawns {}
    }

    /// Services MSP DisplayPort replies, telemetry, and the disarmed tuning menu.
    pub async fn msp_osd(mut cx: msp_osd::Context<'_>) {
        loop {
            let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
            let service_snapshot = cx.shared.telemetry.lock(|telemetry| *telemetry);
            let telemetry = service_snapshot.msp_snapshot(now_ms);

            let mut bytes = [0_u8; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES];
            match Mono::timeout_after(cx.config.interval, cx.local.reader.read(&mut bytes)).await {
                Ok(Ok(read_len)) => {
                    // At most one reply is transmitted per service iteration. Remaining bytes are
                    // still consumed by the parser and may advance an incomplete packet.
                    let mut replied = false;
                    for byte in &bytes[..read_len] {
                        if let Some(frame_len) =
                            cx.local.osd.ingest_byte(*byte, &telemetry, cx.local.output)
                            && !replied
                        {
                            replied = true;
                            if *cx.local.tx_healthy
                                && cx
                                    .local
                                    .writer
                                    .write_all(&cx.local.output[..frame_len])
                                    .await
                                    .is_err()
                            {
                                *cx.local.tx_healthy = false;
                                defmt::warn!("MSP DisplayPort reply transport fault");
                            }
                        }
                    }
                }
                Ok(Err(_)) => {
                    *cx.local.tx_healthy = false;
                    defmt::warn!("MSP DisplayPort RX transport fault");
                }
                Err(_) => {}
            }

            if cx.local.discontinuities.take_new().is_some() {
                defmt::warn!("MSP DisplayPort RX discontinuity");
            }

            let menu_active = cx.shared.tuning.lock(|tuning| {
                let before = *tuning;
                let active = cx.local.osd.update_menu(
                    service_snapshot.armed,
                    OsdStickRates {
                        roll: service_snapshot.rc_rates_dps[0],
                        pitch: service_snapshot.rc_rates_dps[1],
                        yaw: service_snapshot.rc_rates_dps[2],
                    },
                    service_snapshot.rc_throttle,
                    tuning,
                );
                if before != *tuning {
                    cx.shared
                        .tuning_seq
                        .lock(|sequence| *sequence = sequence.wrapping_add(1).max(1));
                }
                active
            });

            *cx.local.refresh_tick = cx.local.refresh_tick.wrapping_add(1);
            if *cx.local.refresh_tick >= 10 {
                *cx.local.refresh_tick = 0;

                if *cx.local.tx_healthy
                    && let Some(frame_len) = cx.local.osd.heartbeat_frame(cx.local.output)
                    && cx
                        .local
                        .writer
                        .write_all(&cx.local.output[..frame_len])
                        .await
                        .is_err()
                {
                    *cx.local.tx_healthy = false;
                    defmt::warn!("MSP DisplayPort heartbeat transport fault");
                }

                let frame_len = if menu_active {
                    let tuning = cx.shared.tuning.lock(|profile| *profile);
                    cx.local.osd.next_menu_frame(&tuning, cx.local.output)
                } else {
                    cx.local
                        .osd
                        .next_overlay_frame(&telemetry, cx.local.output)
                };
                if *cx.local.tx_healthy
                    && let Some(frame_len) = frame_len
                    && cx
                        .local
                        .writer
                        .write_all(&cx.local.output[..frame_len])
                        .await
                        .is_err()
                {
                    *cx.local.tx_healthy = false;
                    defmt::warn!("MSP DisplayPort overlay transport fault");
                }
            }
        }
    }
}
