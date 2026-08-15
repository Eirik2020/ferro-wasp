use crate::task::{TaskDefinition, uart_rx_dma};

crate::app_task! {
    pub const UART_RX_IDLE_IRQ: TaskDefinition = TaskDefinition::synchronous("uart_rx_idle_irq")
        .with_shared(&[uart_rx_dma("rx")]);

    fn uart_rx_idle_irq(mut cx: uart_rx_idle_irq::Context) {
        match cx.shared.rx.lock(|rx| rx.service_idle_irq()) {
            UartRxIrqOutcome::Ignored
            | UartRxIrqOutcome::Delivered
            | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX peripheral error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX IDLE buffer delivery error")
            }
        }
    }
}
