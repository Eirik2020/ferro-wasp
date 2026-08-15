use core::fmt::Write as _;

use embedded_io_async::Write;
use ferrowasp_drivers::mpu6500::ImuData;
use ferrowasp_stm32f4::memory::UartOwnedWriter;
use fugit::MillisDurationU32;

use crate::rtic::task::Mono;

crate::reusable_task! {
    contract {
        local {
            /// Owned byte writer exported by the MSP OSD serial endpoint.
            writer: UartOwnedWriter<'static>,
        }
        shared {
            /// Latest decoded gyroscope sample from the IMU endpoint.
            imu: ImuData,
            /// Latest healthy SBUS channel 3 value.
            channel3: u32,
        }
        config {
            /// Delay between bounded DisplayPort update frames.
            interval: MillisDurationU32,
        }
        spawns {}
    }

    /// Displays gyroscope axes and SBUS channel 3 using MSP V1 DisplayPort.
    pub async fn simple_osd(mut cx: simple_osd::Context<'_>) {
        let mut responder = ferrowasp_mspv1::MspResponder::new();
        let mut output = [0; ferrowasp_mspv1::OSD_TX_BUFFER_LEN];
        let mut step = 0_u8;

        loop {
            let gyro = cx.shared.imu.lock(|imu| imu.gyro);
            let channel3 = cx.shared.channel3.lock(|value| *value);
            let mut text = heapless::String::<32>::new();
            let frame_len = match step {
                0 => responder.clear_screen(&mut output),
                1 => {
                    let _ = write!(
                        text,
                        "GYRO {} {} {}",
                        gyro[0] as i16,
                        gyro[1] as i16,
                        gyro[2] as i16
                    );
                    responder.write_string(3, 2, 0, text.as_bytes(), &mut output)
                }
                2 => {
                    let _ = write!(text, "SBUS CH3 {}", channel3);
                    responder.write_string(5, 2, 0, text.as_bytes(), &mut output)
                }
                3 => responder.draw_screen(&mut output),
                _ => responder.heartbeat(&mut output),
            };

            if let Some(frame_len) = frame_len
                && cx.local.writer.write_all(&output[..frame_len]).await.is_err()
            {
                defmt::warn!("MSP V1 OSD writer stopped after a transport fault");
                return;
            }

            step = (step + 1) % 5;
            Mono::delay(cx.config.interval).await;
        }
    }
}
