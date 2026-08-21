use ferrowasp_stm32f4::serial::{UartRxIrqOutcome, UartRxIrqService};

/// STM32F4 UART receive service shared by the endpoint's RX interrupt tasks.
pub type UartRxService = dyn UartRxIrqService + 'static;

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Receive endpoint shared with the UART peripheral interrupt task.
            rx: UartRxService,
        }
        config {}
        spawns {
            /// Drains completed DMA buffers into the owned receive channel.
            bridge(),
        }
    }

    /// Services one STM32F4 UART receive-DMA interrupt.
    pub fn serial_rx_dma_irq(mut cx: serial_rx_dma_irq::Context<'_>) {
        match cx.shared.rx.lock(UartRxIrqService::service_dma_irq) {
            UartRxIrqOutcome::Ignored | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::Delivered => {
                let _ = bridge::spawn();
            }
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX DMA error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX DMA buffer delivery error")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeRx {
        dma_calls: u8,
    }

    impl UartRxIrqService for FakeRx {
        fn service_dma_irq(&mut self) -> UartRxIrqOutcome {
            self.dma_calls += 1;
            UartRxIrqOutcome::Delivered
        }

        fn service_idle_irq(&mut self) -> UartRxIrqOutcome {
            UartRxIrqOutcome::Ignored
        }
    }

    #[test]
    fn services_the_bound_rx_dma_endpoint_once() {
        let mut rx = FakeRx { dma_calls: 0 };
        let context = serial_rx_dma_irq::Context::new(
            serial_rx_dma_irq::Local::new(),
            serial_rx_dma_irq::Shared::new(&mut rx),
            serial_rx_dma_irq::Config::new(),
        );

        serial_rx_dma_irq(context);

        assert_eq!(rx.dma_calls, 1);
    }
}
