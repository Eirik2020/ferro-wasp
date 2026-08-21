use ferrowasp_core::safety_channel::SafetyProducer;
use ferrowasp_drivers::mpu6500::ImuData;
use fugit::MillisDurationU32;

use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// Exclusive decoded-sample producer consumed by the control task.
            control: SafetyProducer<'static, ImuData>,
        }
        shared {
            /// Latest sample exported by the selected physical IMU endpoint.
            sample: ImuData,
        }
        config {
            /// Interval used to detect newly decoded endpoint samples.
            poll_interval: MillisDurationU32,
        }
        spawns {}
    }

    /// Forwards each newly decoded IMU sample into the control safety channel.
    pub async fn imu_control_bridge(mut cx: imu_control_bridge::Context<'_>) {
        let mut forwarded_sequence = 0_u32;
        loop {
            let sample = cx.shared.sample.lock(|sample| ImuData {
                acc: sample.acc,
                gyro: sample.gyro,
                gyro_raw: sample.gyro_raw,
                temp: sample.temp,
                sequence: sample.sequence,
            });
            if sample.sequence != forwarded_sequence {
                forwarded_sequence = sample.sequence;
                if cx.local.control.try_send(sample).is_err() {
                    defmt::warn!("IMU-to-control safety channel full; rejected newest sample");
                }
            }

            Mono::delay(cx.config.poll_interval).await;
        }
    }
}
