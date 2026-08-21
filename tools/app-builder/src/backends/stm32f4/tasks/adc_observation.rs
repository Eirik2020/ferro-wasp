use ferrowasp_stm32f4::adc::AdcDmaIrqPlanner;
use ferrowasp_tasks::{osd::BatteryCellDetector, service_telemetry::FlightServiceTelemetry};
use fugit::MillisDurationU32;

use crate::{
    backends::stm32f4::task_authoring::golden_services::{
        Adc1ObservationTransfer, AdcDmaDeliveryError, take_completed_adc1_sample_for,
    },
    rtic::task::Mono,
};

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Exclusive ADC1 DMA transfer state shared with its completion interrupt.
            transfer: Adc1ObservationTransfer,
        }
        config {
            /// Delay between bounded ADC conversion sequences.
            interval: MillisDurationU32,
        }
        spawns {}
    }

    /// Starts one ADC1 voltage/current observation at each bounded interval.
    pub async fn adc_observation_poll(mut cx: adc_observation_poll::Context<'_>) {
        loop {
            cx.shared
                .transfer
                .lock(|transfer| transfer.start(|adc| adc.start_conversion()));
            Mono::delay(cx.config.interval).await;
        }
    }
}

crate::reusable_task! {
    contract {
        local {
            /// Spare buffer rotated into the next ADC1 DMA transfer.
            buffer: Option<&'static mut [u16; 3]>,
            /// Explicit DMA completion/error acknowledgement planner.
            planner: AdcDmaIrqPlanner,
            /// Latched, bounded cell-count detector.
            cell_detector: BatteryCellDetector,
        }
        shared {
            /// Exclusive ADC1 DMA transfer state.
            transfer: Adc1ObservationTransfer,
            /// Observation-only flight-service telemetry.
            telemetry: FlightServiceTelemetry,
        }
        config {
            /// Voltage divider ratio multiplied by ten.
            vbat_divider_x10: u32,
            /// Betaflight-compatible current-sense scale.
            current_scale: u32,
            /// Current-sense offset in milliamps.
            current_offset_ma: u32,
        }
        spawns {}
    }

    /// Accepts one complete ADC1 DMA sample and publishes freshness-tracked telemetry.
    pub fn adc_observation_dma(mut cx: adc_observation_dma::Context<'_>) {
        let sample = cx.shared.transfer.lock(|transfer| {
            take_completed_adc1_sample_for(transfer, cx.local.buffer, cx.local.planner)
        });
        let sample = match sample {
            Ok(Some(sample)) => sample,
            Ok(None) => return,
            Err(
                AdcDmaDeliveryError::DmaFault
                | AdcDmaDeliveryError::NoSpareBuffer
                | AdcDmaDeliveryError::TransferNotReady,
            ) => {
                cx.shared
                    .telemetry
                    .lock(|telemetry| telemetry.battery.record_fault());
                defmt::warn!("ADC1 observation rejected after DMA delivery fault");
                return;
            }
        };

        let pack_mv = u32::from(sample.voltage_mv)
            .saturating_mul(cx.config.vbat_divider_x10)
            / 10;
        let cell_count = cx.local.cell_detector.update(pack_mv);
        let cell_voltage_centivolts =
            ferrowasp_tasks::osd::pack_millivolts_to_cell_centivolts(pack_mv, cell_count);
        let current_centiamps = if cell_count == 0 {
            0
        } else {
            ferrowasp_tasks::osd::current_sample_to_centiamps_with_offset(
                u32::from(sample.current_mv),
                cx.config.current_scale,
                cx.config.current_offset_ma as i32,
            )
        };
        let voltage_decivolts = ((pack_mv + 50) / 100).min(u32::from(u8::MAX)) as u8;
        let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;

        cx.shared.telemetry.lock(|telemetry| {
            telemetry.battery.observe(
                voltage_decivolts,
                cell_count,
                cell_voltage_centivolts,
                current_centiamps,
                sample.voltage_mv,
                sample.current_mv,
                now_ms,
            );
        });

        // Return the completed buffer before the next polling conversion.
        *cx.local.buffer = Some(sample.buffer);
    }
}
