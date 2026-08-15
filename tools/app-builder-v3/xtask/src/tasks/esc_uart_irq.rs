use ferrowasp_stm32f4::serial::UartRxIrqOutcome;

use crate::hardware_definitions::stm32f4::dshot_authoring::Uart1RxIrq;

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// USART1 receive DMA/IDLE owner.
            uart: Uart1RxIrq,
            /// Sticky parser discontinuity observation.
            discontinuity: bool,
        }
        config {}
        spawns {}
    }

    /// Services the exact USART1 RX DMA stream.
    pub fn esc_uart_rx_dma(mut cx: esc_uart_rx_dma::Context<'_>) {
        let outcome = cx.shared.uart.lock(|uart| uart.service_dma_irq());
        if matches!(
            outcome,
            UartRxIrqOutcome::DmaError | UartRxIrqOutcome::DeliveryError(_)
        ) {
            cx.shared.discontinuity.lock(|flag| *flag = true);
            defmt::warn!("USART1 ESC telemetry RX DMA discontinuity");
        }
    }
}

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// USART1 receive DMA/IDLE owner.
            uart: Uart1RxIrq,
            /// Sticky parser discontinuity observation.
            discontinuity: bool,
        }
        config {}
        spawns {}
    }

    /// Services USART1 IDLE chunk completion.
    pub fn esc_uart_rx_idle(mut cx: esc_uart_rx_idle::Context<'_>) {
        let outcome = cx.shared.uart.lock(|uart| uart.service_idle_irq());
        if matches!(
            outcome,
            UartRxIrqOutcome::DmaError | UartRxIrqOutcome::DeliveryError(_)
        ) {
            cx.shared.discontinuity.lock(|flag| *flag = true);
            defmt::warn!("USART1 ESC telemetry RX IDLE discontinuity");
        }
    }
}
