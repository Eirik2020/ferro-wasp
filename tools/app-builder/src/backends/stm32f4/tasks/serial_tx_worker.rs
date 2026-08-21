use ferrowasp_io_core::serial::SerialFault;
use ferrowasp_stm32f4::{
    memory::{UART_RX_BUFFER_BYTES, UartOwnedTxOwner},
    serial::{UartTxDmaService, UartTxStartError},
};

/// UART TX queue owner stored locally by the endpoint worker.
pub type UartTxOwner = UartOwnedTxOwner<'static>;

/// STM32F4 UART transmit-DMA service shared by the endpoint's TX tasks.
pub type UartTxDma = dyn UartTxDmaService<UART_RX_BUFFER_BYTES> + 'static;

crate::reusable_task! {
    contract {
        local {
            /// Exclusive owner of queued transmit chunks and completion waits.
            owner: UartTxOwner,
        }
        shared {
            /// UART transmit DMA state shared with its completion interrupt task.
            tx_dma: UartTxDma,
        }
        config {}
        spawns {}
    }

    /// Drains queued UART chunks through DMA and awaits each IRQ completion.
    pub async fn serial_tx_worker(mut cx: serial_tx_worker::Context<'_>) {
        loop {
            let chunk = match cx.local.owner.next_chunk().await {
                Ok(chunk) => chunk,
                Err(_) => {
                    defmt::warn!("UART TX worker stopped before DMA start");
                    return;
                }
            };

            let start = cx
                .shared
                .tx_dma
                .lock(|tx_dma| tx_dma.start_chunk(&chunk));
            if let Err(error) = start {
                let fault = match error {
                    UartTxStartError::InvalidChunk => SerialFault::InvalidChunk,
                    UartTxStartError::Busy | UartTxStartError::TransferMissing => {
                        SerialFault::InvalidState
                    }
                };
                cx.local.owner.fail(fault);
                defmt::warn!("UART TX DMA start failed");
                return;
            }

            if cx.local.owner.wait_completion().await.is_err() {
                defmt::warn!("UART TX worker stopped after DMA start");
                return;
            }
        }
    }
}
