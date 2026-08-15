use ferrowasp_tasks::service_telemetry::{FlightServiceTelemetry, StorageStatus};
use fugit::MillisDurationU32;

use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Observation-only flight telemetry.
            telemetry: FlightServiceTelemetry,
            /// Observation-only SPI NOR health and identity.
            storage: StorageStatus,
            /// Request for the USB interrupt to emit a fresh status line.
            usb_status_due: bool,
        }
        config {
            /// Golden diagnostic heartbeat cadence.
            interval: MillisDurationU32,
        }
        spawns {}
    }

    /// Emits bounded service identity/health and schedules USB diagnostics.
    pub async fn heartbeat(mut cx: heartbeat::Context<'_>) {
        loop {
            let telemetry = cx.shared.telemetry.lock(|telemetry| *telemetry);
            let storage = cx.shared.storage.lock(|storage| *storage);
            defmt::info!(
                "FerroWasp Foxeer heartbeat ctl={} imu={} armed={} flash={} dropped={}",
                telemetry.control_sequence,
                telemetry.imu_sequence,
                telemetry.armed,
                storage.ready,
                storage.dropped_records,
            );
            cx.shared.usb_status_due.lock(|due| *due = true);
            ferrowasp_stm32f4::usb_serial::pend_usb_irq();
            Mono::delay(cx.config.interval).await;
        }
    }
}
