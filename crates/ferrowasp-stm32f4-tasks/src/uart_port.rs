//! The interrupt tasks of each UART port, by logical port number. Receive
//! tasks move bytes into the port's owned stream and record discontinuities
//! there, whatever function config bound to the port; transmit tasks drain
//! the port's queue into its TX DMA when its function talks back. A port left
//! unbound was never started, so its interrupts never fire and its worker
//! ends at once. A board binds the tasks of the ports it routes. All UART
//! transports run at one priority, chosen by the firmware, since any port may
//! serve any function.

use crate::prelude::*;
use ferrowasp_stm32f4::uart_port::{UartRxEvent, UartRxPortService};

macro_rules! uart_port_tasks {
    (
        $name:literal,
        rx: $rx:ident,
        rx_dma: $rx_dma:ident,
        rx_idle: $rx_idle:ident,
        tx_dma: $tx_dma:ident,
        tx_owner: $tx_owner:ident,
        tx_completion: $tx_completion:ident,
        tx_worker: $tx_worker:ident,
        tx_dma_irq: $tx_dma_irq:ident $(,)?
    ) => {
        #[doc = concat!($name, " RX DMA transfer complete: hand the filled buffer to the port's stream.")]
        #[ferroforge::task(
            bounds = [$rx: UartRxPortService],
            shared = [#[lock_free] $rx],
            monotonic = Mono,
        )]
        pub fn $rx_dma(cx: $rx_dma::Context) {
            let now = TimestampMicros(Mono::now().duration_since_epoch().to_micros());
            if cx.shared.$rx.service(UartRxEvent::DmaComplete, now).is_some() {
                warn!("{=str} RX DMA discontinuity", $name);
            }
        }

        #[doc = concat!($name, " idle line: hand the partly filled buffer to the port's stream.")]
        #[ferroforge::task(
            bounds = [$rx: UartRxPortService],
            shared = [#[lock_free] $rx],
            monotonic = Mono,
        )]
        pub fn $rx_idle(cx: $rx_idle::Context) {
            let now = TimestampMicros(Mono::now().duration_since_epoch().to_micros());
            if cx.shared.$rx.service(UartRxEvent::IdleLine, now).is_some() {
                warn!("{=str} RX idle discontinuity", $name);
            }
        }

        #[doc = concat!("Hand each chunk queued for ", $name, " to its TX DMA and wait for it to finish.")]
        /// Stops, with a warning, when the queue closes or DMA refuses a
        /// chunk, and at once when the port has no transmit path.
        #[ferroforge::task(
            local = [$tx_owner: Option<stm32_memory::UartOwnedTxOwner<'static>>],
            bounds = [$tx_dma: stm32_uart::UartTxDmaService],
            shared = [$tx_dma],
        )]
        pub async fn $tx_worker(mut cx: $tx_worker::Context) {
            let Some(owner) = cx.local.$tx_owner.as_mut() else {
                return;
            };
            loop {
                let chunk = match owner.next_chunk().await {
                    Ok(chunk) => chunk,
                    Err(_error) => {
                        warn!("{=str} TX worker stopped before DMA start", $name);
                        return;
                    }
                };

                let start_result = cx.shared.$tx_dma.lock(|tx_dma| tx_dma.start_chunk(&chunk));
                if let Err(error) = start_result {
                    let fault = match error {
                        stm32_uart::UartTxStartError::InvalidChunk => SerialFault::InvalidChunk,
                        stm32_uart::UartTxStartError::Busy
                        | stm32_uart::UartTxStartError::TransferMissing => {
                            SerialFault::InvalidState
                        }
                    };
                    owner.fail(fault);
                    warn!("{=str} TX DMA start failed", $name);
                    return;
                }

                if owner.wait_completion().await.is_err() {
                    warn!("{=str} TX worker stopped after DMA start", $name);
                    return;
                }
            }
        }

        #[doc = concat!($name, " TX DMA complete: finish or fail the in-flight chunk.")]
        #[ferroforge::task(
            local = [$tx_completion: Option<stm32_memory::UartOwnedTxCompletion<'static>>],
            bounds = [$tx_dma: stm32_uart::UartTxDmaService],
            shared = [$tx_dma],
        )]
        pub fn $tx_dma_irq(mut cx: $tx_dma_irq::Context) {
            let outcome = cx.shared.$tx_dma.lock(|tx_dma| tx_dma.service_irq());
            let Some(completion) = cx.local.$tx_completion.as_mut() else {
                return;
            };

            match outcome {
                stm32_uart::UartTxIrqOutcome::Ignored => {}
                stm32_uart::UartTxIrqOutcome::Completed => {
                    if completion.complete().is_err() {
                        warn!("{=str} TX completion arrived without an in-flight chunk", $name);
                    }
                }
                stm32_uart::UartTxIrqOutcome::DmaError(error) => {
                    completion.fail(SerialFault::DmaTransfer);
                    match error {
                        stm32_uart::UartTxDmaError::Transfer => {
                            warn!("{=str} TX DMA transfer error", $name)
                        }
                        stm32_uart::UartTxDmaError::DirectMode => {
                            warn!("{=str} TX DMA direct-mode error", $name)
                        }
                    }
                }
            }
        }
    };
}

uart_port_tasks!(
    "UART1",
    rx: uart1_rx,
    rx_dma: uart1_rx_dma,
    rx_idle: uart1_rx_idle,
    tx_dma: uart1_tx_dma,
    tx_owner: uart1_tx_owner,
    tx_completion: uart1_tx_completion,
    tx_worker: uart1_tx_worker,
    tx_dma_irq: uart1_tx_dma_complete,
);
uart_port_tasks!(
    "UART2",
    rx: uart2_rx,
    rx_dma: uart2_rx_dma,
    rx_idle: uart2_rx_idle,
    tx_dma: uart2_tx_dma,
    tx_owner: uart2_tx_owner,
    tx_completion: uart2_tx_completion,
    tx_worker: uart2_tx_worker,
    tx_dma_irq: uart2_tx_dma_complete,
);
uart_port_tasks!(
    "UART3",
    rx: uart3_rx,
    rx_dma: uart3_rx_dma,
    rx_idle: uart3_rx_idle,
    tx_dma: uart3_tx_dma,
    tx_owner: uart3_tx_owner,
    tx_completion: uart3_tx_completion,
    tx_worker: uart3_tx_worker,
    tx_dma_irq: uart3_tx_dma_complete,
);
uart_port_tasks!(
    "UART4",
    rx: uart4_rx,
    rx_dma: uart4_rx_dma,
    rx_idle: uart4_rx_idle,
    tx_dma: uart4_tx_dma,
    tx_owner: uart4_tx_owner,
    tx_completion: uart4_tx_completion,
    tx_worker: uart4_tx_worker,
    tx_dma_irq: uart4_tx_dma_complete,
);
uart_port_tasks!(
    "UART5",
    rx: uart5_rx,
    rx_dma: uart5_rx_dma,
    rx_idle: uart5_rx_idle,
    tx_dma: uart5_tx_dma,
    tx_owner: uart5_tx_owner,
    tx_completion: uart5_tx_completion,
    tx_worker: uart5_tx_worker,
    tx_dma_irq: uart5_tx_dma_complete,
);
uart_port_tasks!(
    "UART6",
    rx: uart6_rx,
    rx_dma: uart6_rx_dma,
    rx_idle: uart6_rx_idle,
    tx_dma: uart6_tx_dma,
    tx_owner: uart6_tx_owner,
    tx_completion: uart6_tx_completion,
    tx_worker: uart6_tx_worker,
    tx_dma_irq: uart6_tx_dma_complete,
);
uart_port_tasks!(
    "UART7",
    rx: uart7_rx,
    rx_dma: uart7_rx_dma,
    rx_idle: uart7_rx_idle,
    tx_dma: uart7_tx_dma,
    tx_owner: uart7_tx_owner,
    tx_completion: uart7_tx_completion,
    tx_worker: uart7_tx_worker,
    tx_dma_irq: uart7_tx_dma_complete,
);
uart_port_tasks!(
    "UART8",
    rx: uart8_rx,
    rx_dma: uart8_rx_dma,
    rx_idle: uart8_rx_idle,
    tx_dma: uart8_tx_dma,
    tx_owner: uart8_tx_owner,
    tx_completion: uart8_tx_completion,
    tx_worker: uart8_tx_worker,
    tx_dma_irq: uart8_tx_dma_complete,
);
