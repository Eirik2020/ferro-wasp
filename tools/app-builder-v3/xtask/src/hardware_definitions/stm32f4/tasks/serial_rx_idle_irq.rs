use ferrowasp_stm32f4::serial::{UartRxIrqOutcome, UartRxIrqService};

/// STM32F4 UART receive service shared by the endpoint's RX interrupt tasks.
pub type UartRxService = dyn UartRxIrqService + 'static;

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Receive endpoint shared with the RX DMA interrupt task.
            rx: UartRxService,
        }
        config {}
        spawns {}
    }

    /// Services one STM32F4 UART peripheral IDLE interrupt.
    pub fn serial_rx_idle_irq(mut cx: serial_rx_idle_irq::Context<'_>) {
        match cx.shared.rx.lock(UartRxIrqService::service_idle_irq) {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeRx {
        idle_calls: u8,
    }

    impl UartRxIrqService for FakeRx {
        fn service_dma_irq(&mut self) -> UartRxIrqOutcome {
            UartRxIrqOutcome::Ignored
        }

        fn service_idle_irq(&mut self) -> UartRxIrqOutcome {
            self.idle_calls += 1;
            UartRxIrqOutcome::NoChunk
        }
    }

    #[test]
    fn services_the_bound_uart_idle_endpoint_once() {
        let mut rx = FakeRx { idle_calls: 0 };
        let context = serial_rx_idle_irq::Context::new(
            serial_rx_idle_irq::Local::new(),
            serial_rx_idle_irq::Shared::new(&mut rx),
            serial_rx_idle_irq::Config::new(),
        );

        serial_rx_idle_irq(context);

        assert_eq!(rx.idle_calls, 1);
    }
}
