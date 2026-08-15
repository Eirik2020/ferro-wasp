use ferrowasp_stm32f4::serial::{UartOwnedRxBridgeOutcome, UartOwnedRxBridgeService};

/// Host-checkable receive bridge capability implemented by the STM32 backend.
pub type UartRxBridge = dyn UartOwnedRxBridgeService + 'static;

crate::reusable_task! {
    contract {
        local {
            /// Exclusive DMA-parser to owned-channel bridge.
            bridge: UartRxBridge,
        }
        shared {}
        config {}
        spawns {}
    }

    /// Drains every completed DMA buffer without unbounded waiting.
    pub async fn serial_rx_bridge(cx: serial_rx_bridge::Context<'_>) {
        loop {
            match cx.local.bridge.publish_next_untimed() {
                UartOwnedRxBridgeOutcome::Published => {}
                UartOwnedRxBridgeOutcome::NoChunk => return,
                UartOwnedRxBridgeOutcome::InvalidChunk => {
                    defmt::warn!("UART RX bridge rejected an invalid chunk")
                }
                UartOwnedRxBridgeOutcome::QueueOverflow => {
                    defmt::warn!("UART RX owned queue overflowed")
                }
                UartOwnedRxBridgeOutcome::Disabled => return,
                UartOwnedRxBridgeOutcome::RecycleFailed => {
                    defmt::warn!("UART RX bridge could not recycle its DMA buffer");
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use core::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    use super::*;

    struct FakeBridge {
        outcomes: [UartOwnedRxBridgeOutcome; 3],
        calls: usize,
    }

    impl UartOwnedRxBridgeService for FakeBridge {
        fn publish_next_untimed(&mut self) -> UartOwnedRxBridgeOutcome {
            let outcome = self.outcomes[self.calls];
            self.calls += 1;
            outcome
        }
    }

    fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        future.as_mut().poll(&mut context)
    }

    #[test]
    fn drains_all_waiting_chunks_before_returning() {
        let mut bridge = FakeBridge {
            outcomes: [
                UartOwnedRxBridgeOutcome::Published,
                UartOwnedRxBridgeOutcome::Published,
                UartOwnedRxBridgeOutcome::NoChunk,
            ],
            calls: 0,
        };
        let context = serial_rx_bridge::Context::new(
            serial_rx_bridge::Local::new(&mut bridge),
            serial_rx_bridge::Shared::new(),
            serial_rx_bridge::Config::new(),
        );

        assert!(poll_once(serial_rx_bridge(context)).is_ready());
        assert_eq!(bridge.calls, 3);
    }
}
