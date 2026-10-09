//! The MSP DisplayPort OSD on the port config binds to it: the refresh task
//! and its writer.

use crate::prelude::*;
use crate::snapshots::*;

/// Write one frame to the OSD, or nothing once a write has failed: the link
/// stays down rather than retrying into a faulted DMA stream.
pub async fn osd_write(
    writer: &mut Option<stm32_memory::UartOwnedWriter<'static>>,
    healthy: &mut bool,
    bytes: &[u8],
) {
    use embedded_io_async::Write;

    let Some(writer) = writer.as_mut().filter(|_| *healthy) else {
        return;
    };

    if let Err(error) = writer.write_all(bytes).await {
        *healthy = false;
        match error {
            SerialFault::DmaTransfer => warn!("OSD TX writer stopped after DMA fault"),
            SerialFault::Disabled => {
                warn!("OSD TX writer stopped because stream is disabled")
            }
            SerialFault::InvalidChunk => warn!("OSD TX writer rejected invalid MSP frame"),
            SerialFault::InvalidState => warn!("OSD TX writer found invalid transport state"),
            SerialFault::QueueOverflow => warn!("OSD TX writer queue overflowed"),
            SerialFault::Timeout => warn!("OSD TX writer timed out"),
            SerialFault::UnsupportedProtocol => {
                warn!("OSD TX writer rejected unsupported protocol")
            }
        }
    }
}

/// Answer the OSD's requests and push the overlay or tuning menu, every
/// 10 ms. Reads RC, arming, battery and attitude state; never writes any of
/// it except the tuning profile, through the menu. With no port bound the
/// task ends at once.
#[ferroforge::task(
    local = [
        osd_port: Option<ferrowasp_stm32::uart_port::SerialPortEndpoint>,
        osd_tx_healthy: bool,
        osd_task: osd::OsdTask,
        osd_tx_buffer: [u8; mspv1::OSD_TX_BUFFER_LEN],
        osd_refresh_tick: u8,
        osd_safety_arm_reader: signals::SafetyArmReader,
        osd_rc_throttle_reader: signals::RcThrottleReader,
        osd_rc_rates_reader: signals::RcRatesReader,
    ],
    shared = [
        battery_voltage_v10: u8,
        battery_cell_count: u8,
        battery_cell_voltage_v100: u16,
        battery_current_ca: i16,
        imu_angles: [f32; 3],
        imu_rates: [f32; 3],
        imu_data: imu::ImuData,
        tuning_profile: dt::TuningProfile,
        tuning_request_seq: u32,
    ],
    monotonic = Mono,
)]
pub async fn osd_refresh(mut cx: osd_refresh::Context) {
    let Some(port) = cx.local.osd_port.as_mut() else {
        return;
    };
    loop {
        let rates = cx.local.osd_rc_rates_reader.read();
        let throttle = cx.local.osd_rc_throttle_reader.read();
        let armed = cx.local.osd_safety_arm_reader.read();
        let battery_voltage_v10 = cx.shared.battery_voltage_v10.lock(|value| *value);
        let battery_cell_count = cx.shared.battery_cell_count.lock(|value| *value);
        let battery_cell_voltage_v100 = cx.shared.battery_cell_voltage_v100.lock(|value| *value);
        let amperage_ca = cx.shared.battery_current_ca.lock(|value| *value);
        let angles = cx.shared.imu_angles.lock(|angles| *angles);
        let imu_rates = cx.shared.imu_rates.lock(|rates| *rates);
        let imu_sequence = IMU_LATEST_SEQ.load(Ordering::Relaxed);
        let imu_raw = [
            IMU_LATEST_ROLL_RAW.load(Ordering::Relaxed) as i16,
            IMU_LATEST_PITCH_RAW.load(Ordering::Relaxed) as i16,
            IMU_LATEST_YAW_RAW.load(Ordering::Relaxed) as i16,
        ];
        let telemetry = mspv1::MspOsdTelemetry {
            armed,
            battery_voltage_v10,
            battery_cell_count,
            battery_cell_voltage_v100,
            amperage_ca,
            rc_roll: osd::map_rate_to_msp_rc(rates.roll),
            rc_pitch: osd::map_rate_to_msp_rc(rates.pitch),
            rc_yaw: osd::map_rate_to_msp_rc(rates.yaw),
            rc_throttle: osd::map_throttle_to_msp_rc(throttle),
            osd_throttle: throttle.min(2000) as u16,
            roll_deg10: (angles[0] * 10.0) as i16,
            pitch_deg10: (angles[1] * 10.0) as i16,
            yaw_deg: angles[2] as i16,
            imu_roll_dps: imu_rates[0] as i16,
            imu_pitch_dps: imu_rates[1] as i16,
            imu_yaw_dps: imu_rates[2] as i16,
            imu_roll_dps10: (imu_rates[0] * 10.0) as i16,
            imu_pitch_dps10: (imu_rates[1] * 10.0) as i16,
            imu_yaw_dps10: (imu_rates[2] * 10.0) as i16,
            imu_raw,
            imu_sequence,
            imu_stale: IMU_STALE.load(Ordering::Relaxed),
            control_isr_sequence: CONTROL_ISR_SEQ.load(Ordering::Relaxed),
            control_sequence: CONTROL_RATE_SEQ.load(Ordering::Relaxed),
            control_raw: [
                CONTROL_ROLL_RAW.load(Ordering::Relaxed) as i16,
                CONTROL_PITCH_RAW.load(Ordering::Relaxed) as i16,
                CONTROL_YAW_RAW.load(Ordering::Relaxed) as i16,
            ],
            control_dps10: [
                CONTROL_ROLL_DPS10.load(Ordering::Relaxed) as i16,
                CONTROL_PITCH_DPS10.load(Ordering::Relaxed) as i16,
                CONTROL_YAW_DPS10.load(Ordering::Relaxed) as i16,
            ],
            ..mspv1::MspOsdTelemetry::default()
        };

        let menu_active = {
            let mut changed = false;
            let active = cx.shared.tuning_profile.lock(|profile| {
                let before = *profile;
                let active = cx.local.osd_task.update_menu(
                    armed,
                    osd::OsdStickRates {
                        roll: rates.roll,
                        pitch: rates.pitch,
                        yaw: rates.yaw,
                    },
                    throttle,
                    profile,
                );
                changed = before != *profile;
                active
            });

            if changed {
                cx.shared.tuning_request_seq.lock(|seq| {
                    *seq = seq.wrapping_add(1);
                });
            }

            active
        };

        let mut bytes = [0; stm32_uart::UART_RX_BUFFER_SIZE];
        loop {
            let read_len = match port.reader.try_read(&mut bytes) {
                Ok(0) => break,
                Ok(read_len) => read_len,
                Err(_) => {
                    warn!("OSD RX stream failed");
                    break;
                }
            };
            for byte in &bytes[..read_len] {
                if let Some(frame_len) =
                    cx.local
                        .osd_task
                        .ingest_byte(*byte, &telemetry, cx.local.osd_tx_buffer)
                {
                    osd_write(
                        &mut port.writer,
                        cx.local.osd_tx_healthy,
                        &cx.local.osd_tx_buffer[..frame_len],
                    )
                    .await;
                }
            }
        }
        if let Some(event) = port.discontinuities.take_new() {
            warn!("OSD RX discontinuity sequence {}", event.sequence);
        }

        *cx.local.osd_refresh_tick = cx.local.osd_refresh_tick.wrapping_add(1);
        if *cx.local.osd_refresh_tick >= 10 {
            *cx.local.osd_refresh_tick = 0;

            if let Some(frame_len) = cx.local.osd_task.heartbeat_frame(cx.local.osd_tx_buffer) {
                osd_write(
                    &mut port.writer,
                    cx.local.osd_tx_healthy,
                    &cx.local.osd_tx_buffer[..frame_len],
                )
                .await;
            }

            if menu_active {
                let tuning = cx.shared.tuning_profile.lock(|profile| *profile);
                if let Some(frame_len) = cx
                    .local
                    .osd_task
                    .next_menu_frame(&tuning, cx.local.osd_tx_buffer)
                {
                    osd_write(
                        &mut port.writer,
                        cx.local.osd_tx_healthy,
                        &cx.local.osd_tx_buffer[..frame_len],
                    )
                    .await;
                }
            } else if let Some(frame_len) = cx
                .local
                .osd_task
                .next_overlay_frame(&telemetry, cx.local.osd_tx_buffer)
            {
                osd_write(
                    &mut port.writer,
                    cx.local.osd_tx_healthy,
                    &cx.local.osd_tx_buffer[..frame_len],
                )
                .await;
            }
        }

        Mono::delay(10.millis()).await;
    }
}
