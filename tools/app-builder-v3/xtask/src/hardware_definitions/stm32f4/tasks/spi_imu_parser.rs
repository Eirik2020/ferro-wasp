use crate::hardware_definitions::stm32f4::spi_imu::{
    IMU_KIND_ICM42688P, IMU_KIND_MPU6500, Spi1ImuParser,
};
use ferrowasp_core::frames::FrameRotation;
use ferrowasp_drivers::mpu6500::ImuData;

crate::reusable_task! {
    contract {
        local {
            /// Completed-DMA queue and free-buffer producer.
            parser: Spi1ImuParser,
        }
        shared {
            /// Active sensor discriminant selected during initialization.
            kind: u8,
            /// Latest decoded sample in the declared physical body frame.
            sample: ImuData,
        }
        config {
            /// Sensor-frame to body-frame permutation, applied exactly once here.
            orientation: FrameRotation,
        }
        spawns {}
    }

    /// Decodes DMA buffers and publishes body-frame IMU samples.
    ///
    /// Downstream control code must not repeat the installation rotation. The
    /// controller's explicit body-to-controller compatibility transform remains
    /// a separate operation.
    pub async fn spi_imu_parser(mut cx: spi_imu_parser::Context<'_>) {
        let kind = cx.shared.kind.lock(|kind| *kind);
        while let Some(filled) = cx.local.parser.next_frame() {
            let len = filled.len.min(filled.buffer.len());
            let frame = &filled.buffer[..len];
            let expected_request = match kind {
                IMU_KIND_MPU6500 => Some(ferrowasp_drivers::mpu6500::Register::AccelXoutH as u8),
                IMU_KIND_ICM42688P => {
                    Some(ferrowasp_drivers::icm42688p::Register::TempData1 as u8)
                }
                _ => None,
            };
            let parsed = if expected_request != Some(filled.request) {
                None
            } else {
                match kind {
                IMU_KIND_MPU6500 => ferrowasp_drivers::mpu6500::decode_accel_temp_gyro_burst(frame)
                    .ok()
                    .map(|sample| {
                        (
                            [
                                sample.acc_raw[0] as f32 / 4_096.0,
                                sample.acc_raw[1] as f32 / 4_096.0,
                                sample.acc_raw[2] as f32 / 4_096.0,
                            ],
                            [
                                sample.gyro_raw[0] as f32 / 16.4,
                                sample.gyro_raw[1] as f32 / 16.4,
                                sample.gyro_raw[2] as f32 / 16.4,
                            ],
                            sample.gyro_raw,
                            sample.temp_raw as f32 / 333.87 + 21.0,
                        )
                    }),
                IMU_KIND_ICM42688P => {
                    ferrowasp_drivers::icm42688p::decode_temp_accel_gyro_burst(frame)
                        .ok()
                        .map(|sample| {
                            (
                                sample.accel_g(ferrowasp_drivers::icm42688p::AccelFullScale::G16),
                                sample.gyro_dps(
                                    ferrowasp_drivers::icm42688p::GyroFullScale::Dps2000,
                                ),
                                sample.gyro_raw,
                                sample.temperature_c(),
                            )
                        })
                }
                    _ => None,
                }
            };

            if let Some((acc, gyro, gyro_raw, temp)) = parsed {
                let acc = cx.config.orientation.map_f32(acc);
                let gyro = cx.config.orientation.map_f32(gyro);
                let gyro_raw = cx.config.orientation.map_i16_saturating(gyro_raw);
                cx.shared.sample.lock(|sample| {
                    sample.acc = acc;
                    sample.gyro = gyro;
                    sample.gyro_raw = gyro_raw;
                    sample.temp = temp;
                    sample.sequence = sample.sequence.wrapping_add(1);
                });
            } else {
                defmt::warn!("SPI1 IMU frame could not be decoded");
            }

            if cx.local.parser.return_buffer(filled.buffer).is_err() {
                defmt::warn!("SPI1 IMU free-buffer queue rejected a returned buffer");
            }
        }
    }
}
