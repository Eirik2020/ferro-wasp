use ferrowasp_tasks::{
    flash_storage::{CommandParser, CommandProducer, ResponseConsumer, ResponseFrame},
    service_telemetry::{FlightServiceTelemetry, StorageStatus},
    usb_debug::{self, ImuKind, StatusSnapshot},
};

use crate::hardware_definitions::stm32f4::golden_service_authoring::{
    BufferedUsbCdcSerial, UsbCdcDevice, UsbDeviceState,
};
use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// Sole OTG_FS USB device owner.
            device: UsbCdcDevice,
            /// Sole buffered CDC serial-class owner.
            serial: BufferedUsbCdcSerial,
            /// Connection-scoped diagnostic header latch.
            header_sent: bool,
            /// Bounded whitelisted command parser.
            parser: CommandParser,
            /// USB-to-flash command producer.
            commands: CommandProducer,
            /// Flash-to-USB response consumer.
            responses: ResponseConsumer,
            /// Partially transmitted response retained across interrupts.
            pending_response: Option<ResponseFrame>,
        }
        shared {
            /// Observation-only flight telemetry.
            telemetry: FlightServiceTelemetry,
            /// Observation-only storage identity and health.
            storage: StorageStatus,
            /// Heartbeat-owned request for one status line.
            status_due: bool,
        }
        config {}
        spawns {}
    }

    /// Services USB CDC diagnostics and forwards only whitelisted storage commands.
    pub fn usb_cdc(mut cx: usb_cdc::Context<'_>) {
        const USB_DEBUG_HEADER: &[u8] =
            b"FerroWasp Foxeer debug/config (disarmed writes only)\r\n";
        let _ = cx.local.device.poll(&mut [cx.local.serial]);

        let mut rx = [0_u8; 64];
        let read_len = cx.local.serial.read(&mut rx).unwrap_or(0);
        for byte in &rx[..read_len] {
            let Some(parsed) = cx.local.parser.ingest(*byte) else {
                continue;
            };
            match parsed {
                Ok(command) if cx.local.commands.enqueue(command).is_ok() => {}
                Ok(_) => {
                    let _ = cx.local.serial.write(b"ERR command queue full\r\n");
                }
                Err(_) => {
                    let _ = cx
                        .local
                        .serial
                        .write(b"ERR invalid command; type help\r\n");
                }
            }
        }

        if cx.local.device.state() != UsbDeviceState::Configured {
            *cx.local.header_sent = false;
            *cx.local.pending_response = None;
            return;
        }
        if cx.local.serial.flush().is_err() {
            return;
        }

        if !*cx.local.header_sent {
            if matches!(
                cx.local.serial.write(USB_DEBUG_HEADER),
                Ok(written) if written == USB_DEBUG_HEADER.len()
            ) {
                *cx.local.header_sent = true;
            }
            return;
        }

        if cx.local.pending_response.is_none() {
            *cx.local.pending_response = cx.local.responses.dequeue();
        }
        if let Some(response) = cx.local.pending_response.as_ref() {
            if matches!(
                cx.local.serial.write(response.as_bytes()),
                Ok(written) if written == response.as_bytes().len()
            ) {
                *cx.local.pending_response = None;
                ferrowasp_stm32f4::usb_serial::pend_usb_irq();
            }
            return;
        }

        let due = cx.shared.status_due.lock(|due| {
            let value = *due;
            *due = false;
            value
        });
        if !due {
            return;
        }

        let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
        let telemetry = cx.shared.telemetry.lock(|telemetry| *telemetry);
        let _storage = cx.shared.storage.lock(|storage| *storage);
        let battery = telemetry.battery.is_fresh(now_ms).then_some(telemetry.battery);
        let snapshot = StatusSnapshot {
            uptime_ms: now_ms,
            imu_kind: ImuKind::None,
            imu_ready: telemetry.imu_sequence != 0,
            imu_sequence: telemetry.imu_sequence,
            gyro_raw: [0; 3],
            imu_stale: telemetry.imu_stale,
            control_sequence: telemetry.control_sequence,
            rc_valid: telemetry.rc_link_valid,
            rc_armable: telemetry.rc_link_valid && !telemetry.armed,
            rc_throttle: telemetry.rc_throttle,
            rc_arm_high: false,
            system_armed: telemetry.armed,
            battery_voltage_decivolts: battery
                .map_or(0, |value| u32::from(value.voltage_decivolts)),
            battery_current_centiamps: battery
                .map_or(0, |value| i32::from(value.current_centiamps)),
            adc_voltage_mv: battery.map_or(0, |value| u32::from(value.adc_voltage_mv)),
            adc_current_mv: battery.map_or(0, |value| u32::from(value.adc_current_mv)),
        };
        match usb_debug::format_status(snapshot) {
            Ok(line)
                if matches!(
                    cx.local.serial.write(line.as_bytes()),
                    Ok(written) if written == line.len()
                ) => {}
            _ => cx.shared.status_due.lock(|due| *due = true),
        }
    }
}
