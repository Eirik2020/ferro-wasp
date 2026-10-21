//! The pilot-facing outputs' RTIC task definitions (ADR-0060, ADR-0065),
//! target-only: the MSP DisplayPort OSD on the port config binds to it, its
//! writer, and battery telemetry back through the RC receiver.

use crate as osd;
use defmt::warn;
use ferrowasp_border::observer::Ledger;
use ferrowasp_flight_control::drone_toolbox as dt;
use ferrowasp_io_contracts::time::TimestampMicros;
use ferrowasp_mspv1 as mspv1;
use ferrowasp_safety as safety;
use ferrowasp_safety::signals;
use ferrowasp_serial::SerialFault;
use ferrowasp_stm32_critical::memory as stm32_memory;
use ferrowasp_stm32_critical::uart_common as stm32_uart;
use fugit::ExtU64 as _;

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
/// 10 ms. Reads RC, battery, IMU and control state from the ledger; never
/// writes any of it except the tuning profile, through the menu, which it
/// opens only while the arm-state flag reads disarmed. With no port bound the
/// task ends at once.
#[ferroforge::task(
    local = [
        osd_port: Option<ferrowasp_stm32_critical::uart_port::SerialPortEndpoint>,
        osd_tx_healthy: bool,
        osd_task: osd::OsdTask,
        osd_tx_buffer: [u8; mspv1::OSD_TX_BUFFER_LEN],
        osd_refresh_tick: u8,
        osd_safety_arm_reader: signals::SafetyArmReader,
        osd_stall_watch: osd::StallWatch = osd::StallWatch::new(),
    ],
    shared = [
        ledger: Ledger,
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
        let (observer, control, imu, rc, battery) = cx.shared.ledger.lock(|ledger| {
            (
                ledger.observer(),
                ledger.control(),
                ledger.imu().value,
                ledger.rc().value,
                ledger.battery(),
            )
        });
        let stale = cx.local.osd_stall_watch.observe(
            &observer,
            &control,
            &battery,
            TimestampMicros(Mono::now().duration_since_epoch().to_micros()),
        );
        let (control, battery) = (control.value, battery.value);
        let rates = safety::RcRates {
            roll: rc.rates_dps[0],
            pitch: rc.rates_dps[1],
            yaw: rc.rates_dps[2],
        };
        let throttle = rc.throttle;
        // A gate, so the flag rather than the ledger: a stalled observer must
        // never open the tuning menu while armed.
        let armed = cx.local.osd_safety_arm_reader.read();
        let angles = control.attitude_deg;
        let imu_rates = control.body_rates_dps;
        let telemetry = mspv1::MspOsdTelemetry {
            armed,
            battery_voltage_v10: battery.voltage_dv.min(u16::from(u8::MAX)) as u8,
            battery_cell_count: battery.cell_count,
            battery_cell_voltage_v100: battery.cell_voltage_cv,
            amperage_ca: battery.current_ca,
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
            imu_raw: imu.gyro_raw,
            imu_sequence: imu.sample_seq,
            imu_stale: control.imu_stale,
            control_isr_sequence: control.isr_seq,
            control_sequence: control.control_seq,
            control_raw: control.gyro_raw.map(|raw| raw as i16),
            control_dps10: control
                .filtered_gyro_dps
                .map(|rate| (rate * 10.0) as i32 as i16),
            data_stale: stale.data,
            battery_stale: stale.battery,
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

/// How often battery telemetry goes back to the receiver.
const RC_TELEMETRY_PERIOD_MS: u64 = 200;

/// Send battery telemetry back through the RC receiver, for the radio to
/// show. It owns the RC port's writer, so RC input never waits on a
/// transmit; it runs at a low priority and only reads the ledger.
///
/// Ends at once when the RC port has no transmit path: SBUS, or a port
/// without transmit DMA. Stops, with a warning, if the transmit path faults,
/// which leaves RC input untouched.
#[ferroforge::task(
    local = [rc_telemetry_writer: Option<stm32_memory::UartOwnedWriter<'static>>],
    shared = [ledger: Ledger],
    monotonic = Mono,
)]
pub async fn rc_telemetry(mut cx: rc_telemetry::Context) {
    use embedded_io_async::Write;
    use ferrowasp_crsf as crsf;

    let Some(writer) = cx.local.rc_telemetry_writer.as_mut() else {
        return;
    };
    loop {
        let battery = cx.shared.ledger.lock(|ledger| ledger.battery().value);
        let frame = crsf::encode_battery(crsf::BatteryTelemetry {
            voltage_dv: battery.voltage_dv,
            current_da: u16::try_from(battery.current_ca.max(0) / 10).unwrap_or(u16::MAX),
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
