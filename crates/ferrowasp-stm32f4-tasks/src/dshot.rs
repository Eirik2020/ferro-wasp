//! DShot motor output: the DMA completion of each motor's frame.

use crate::prelude::*;
use ferrowasp_stm32f4::dshot;

pub fn service_dshot_dma_irq(
    bank: &mut impl rtic::Mutex<T = dshot::DshotMotorBank>,
    motor: dshot::DshotMotor,
    stream: u8,
) {
    let event = bank.lock(|dshot| dshot.on_dma_interrupt(motor));
    if event == dshot::DshotInterruptEvent::Spurious {
        warn!(
            "Foxeer DShot received spurious DMA2 Stream{} interrupt",
            stream
        );
    }
}

/// One motor's DShot DMA transfer complete. Selected once per motor: which
/// motor, and which DMA2 stream carries it for the log, are the board's.
#[ferroforge::task(
    shared = [dshot_motors: dshot::DshotMotorBank],
    config = [motor: dshot::DshotMotor, stream: u8],
)]
pub fn dshot_dma_complete(mut cx: dshot_dma_complete::Context) {
    service_dshot_dma_irq(&mut cx.shared.dshot_motors, CONFIG::MOTOR, CONFIG::STREAM);
}
