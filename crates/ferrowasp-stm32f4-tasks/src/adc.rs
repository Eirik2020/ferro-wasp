//! ADC1, the battery monitor: a 10 Hz conversion trigger, and the DMA
//! completion that turns a sample into pack voltage, cell count and current.
//!
//! The transfer is bounded rather than named, because boards give ADC1
//! different DMA streams; the calibration is the board's, so it is
//! configuration.

use crate::prelude::*;
use crate::snapshots::*;

/// Start an ADC1 conversion every 100 ms.
#[ferroforge::task(
    bounds = [adc1_transfer: stm32_adc::Adc1ObservationDma],
    shared = [adc1_transfer],
    monotonic = Mono,
)]
pub async fn adc1_polling(mut cx: adc1_polling::Context) {
    loop {
        cx.shared.adc1_transfer.lock(|transfer| {
            transfer.start(|adc| {
                adc.start_conversion();
            });
        });

        Mono::delay(100.millis()).await;
    }
}

/// ADC1 DMA complete: publish pack voltage, detected cell count and current.
/// The cell detector starts from the board's cell limits, so the firmware
/// supplies it.
#[ferroforge::task(
    bounds = [adc1_transfer: stm32_adc::Adc1ObservationDma],
    shared = [
        adc1_transfer,
        battery_voltage_v10: u8,
        battery_cell_count: u8,
        battery_cell_voltage_v100: u16,
        battery_current_ca: i16,
    ],
    local = [
        adc1_buffer: Option<stm32_adc::Adc1SampleBuffer>,
        adc1_planner: stm32_adc::AdcDmaIrqPlanner = stm32_adc::AdcDmaIrqPlanner::new(),
        battery_cell_detector: osd::BatteryCellDetector,
        battery_voltage_init_logged: bool = false,
    ],
    config = [
        adc_vbat_divider_ratio: f32,
        adc_current_betaflight_scale: u32,
        adc_current_offset_ma: i32,
    ],
)]
pub fn dma_adc1(mut cx: dma_adc1::Context) {
    let sample = match cx.shared.adc1_transfer.lock(|transfer| {
        transfer.take_completed_sample(cx.local.adc1_buffer, cx.local.adc1_planner)
    }) {
        Ok(Some(sample)) => sample,
        Ok(None) => return,
        Err(stm32_adc::AdcDmaDeliveryError::DmaFault) => {
            warn!("ADC1 DMA error");
            return;
        }
        Err(stm32_adc::AdcDmaDeliveryError::NoSpareBuffer) => {
            panic!("ADC1 spare buffer missing");
        }
        Err(stm32_adc::AdcDmaDeliveryError::TransferNotReady) => {
            warn!("ADC1 DMA next_transfer failed");
            return;
        }
    };

    // Pull the ADC data out of the buffer that the DMA transfer gave us
    let raw_temp = sample.buffer[0];

    // Now that we're finished with this buffer, put it back in `local.buffer` so it's ready for the next transfer
    // If we don't do this before the next transfer, we'll get a panic
    *cx.local.adc1_buffer = Some(sample.buffer);

    let cal30 = VtempCal30::get().read() as f32;
    let cal110 = VtempCal110::get().read() as f32;

    let _temperature = (110.0 - 30.0) * ((raw_temp as f32) - cal30) / (cal110 - cal30) + 30.0;
    let pack_mv = ((sample.voltage_mv as f32) * CONFIG::ADC_VBAT_DIVIDER_RATIO) as u32;
    let cell_count = cx.local.battery_cell_detector.update(pack_mv);
    let cell_voltage_v100 = osd::pack_millivolts_to_cell_centivolts(pack_mv, cell_count);
    let current_ca = if cell_count == 0 {
        0
    } else {
        osd::current_sample_to_centiamps_with_offset(
            sample.current_mv as u32,
            CONFIG::ADC_CURRENT_BETAFLIGHT_SCALE,
            CONFIG::ADC_CURRENT_OFFSET_MA,
        )
    };

    let battery_voltage_v10 = ((pack_mv + 50) / 100).min(u8::MAX as u32) as u8;
    BATTERY_VOLTAGE_V10_SNAPSHOT.store(u32::from(battery_voltage_v10), Ordering::Relaxed);
    BATTERY_CURRENT_CA_SNAPSHOT.store(i32::from(current_ca), Ordering::Relaxed);
    ADC_VOLTAGE_MV_SNAPSHOT.store(sample.voltage_mv as u32, Ordering::Relaxed);
    ADC_CURRENT_MV_SNAPSHOT.store(sample.current_mv as u32, Ordering::Relaxed);
    cx.shared
        .battery_voltage_v10
        .lock(|value| *value = battery_voltage_v10);
    cx.shared
        .battery_cell_count
        .lock(|battery_cell_count| *battery_cell_count = cell_count);
    cx.shared
        .battery_cell_voltage_v100
        .lock(|battery_cell_voltage_v100| *battery_cell_voltage_v100 = cell_voltage_v100);
    cx.shared
        .battery_current_ca
        .lock(|battery_current_ca| *battery_current_ca = current_ca);

    if !*cx.local.battery_voltage_init_logged {
        let pack_v10 = (pack_mv + 50) / 100;
        info!(
            "Initial battery voltage: {}.{}V",
            pack_v10 / 10,
            pack_v10 % 10
        );
        *cx.local.battery_voltage_init_logged = true;
    }

    let _ = (cell_count, current_ca);
}
