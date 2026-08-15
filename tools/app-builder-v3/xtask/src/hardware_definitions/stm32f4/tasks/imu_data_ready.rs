use stm32f4xx_hal::gpio::ExtiPin;

use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// Rising-edge data-ready input owned by this interrupt task.
            data_ready: dyn ExtiPin,
        }
        shared {
            /// Active sensor discriminant; zero means bring-up failed.
            kind: u8,
        }
        config {}
        spawns {
            /// Starts one bounded asynchronous sensor transaction.
            poll(observed_at_us: u64),
        }
    }

    /// Acknowledges one IMU data-ready edge and requests a sample.
    pub fn imu_data_ready(mut cx: imu_data_ready::Context<'_>) {
        if !cx.local.data_ready.check_interrupt() {
            return;
        }
        cx.local.data_ready.clear_interrupt_pending_bit();

        let kind = cx.shared.kind.lock(|kind| *kind);
        if kind == 0 {
            return;
        }
        let observed_at_us = Mono::now().duration_since_epoch().to_micros();
        if poll::spawn(observed_at_us).is_err() {
            defmt::warn!("IMU sample request rejected while the poll task is busy");
        }
    }
}
