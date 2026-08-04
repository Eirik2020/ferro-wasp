use crate::task::{TaskDefinition, uart_rx_dma};

crate::app_task! {
    pub const UART_RX_DMA_IRQ: TaskDefinition = TaskDefinition::synchronous("uart_rx_dma_irq")
        .with_shared(&[uart_rx_dma("rx")]);

    fn uart_rx_dma_irq(mut cx: uart_rx_dma_irq::Context) {
        match cx.shared.rx.lock(|rx| rx.service_dma_irq()) {
            UartRxIrqOutcome::Ignored
            | UartRxIrqOutcome::Delivered
            | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::DmaError => defmt::warn!("SBUS RX DMA error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("SBUS RX DMA buffer delivery error")
            }
        }
    }
}
