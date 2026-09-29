//! USART1, the ESC telemetry return line: DMA and idle-line receive. A
//! receive fault marks a wire discontinuity for the ESC manager.

use crate::prelude::*;
use crate::snapshots::*;

/// USART1 RX DMA transfer complete. `uart1_rx` is lock-free, shared with the
/// idle-line handler at one priority.
#[ferroforge::task(shared = [#[lock_free] uart1_rx: stm32_uart::Uart1RxIrq])]
pub fn usart1_rx_dma_transfer(cx: usart1_rx_dma_transfer::Context) {
    match cx.shared.uart1_rx.service_dma_irq() {
        stm32_uart::UartRxIrqOutcome::Delivered
        | stm32_uart::UartRxIrqOutcome::Ignored
        | stm32_uart::UartRxIrqOutcome::NoChunk => {}
        stm32_uart::UartRxIrqOutcome::DmaError | stm32_uart::UartRxIrqOutcome::DeliveryError(_) => {
            ESC_TELEMETRY_DISCONTINUITY.store(true, Ordering::Relaxed);
            warn!("Foxeer USART1 ESC telemetry RX DMA discontinuity");
        }
    }
}

/// USART1 idle line: deliver the partly filled buffer.
#[ferroforge::task(shared = [#[lock_free] uart1_rx: stm32_uart::Uart1RxIrq])]
pub fn usart1_rx_peripheral(cx: usart1_rx_peripheral::Context) {
    match cx.shared.uart1_rx.service_idle_irq() {
        stm32_uart::UartRxIrqOutcome::Delivered
        | stm32_uart::UartRxIrqOutcome::Ignored
        | stm32_uart::UartRxIrqOutcome::NoChunk => {}
        stm32_uart::UartRxIrqOutcome::DmaError | stm32_uart::UartRxIrqOutcome::DeliveryError(_) => {
            ESC_TELEMETRY_DISCONTINUITY.store(true, Ordering::Relaxed);
            warn!("Foxeer USART1 ESC telemetry RX IDLE discontinuity");
        }
    }
}
