//! UART4, the OSD link: the transmit worker that feeds queued chunks to DMA.

use crate::prelude::*;

/// Hand each queued chunk to the UART4 TX DMA and wait for it to finish.
/// Stops, with a warning, when the queue closes or DMA refuses a chunk.
#[ferroforge::task(
    local = [uart4_tx_owner: stm32_memory::UartOwnedTxOwner<'static>],
    shared = [uart4_tx_dma: stm32_uart::Uart4TxDmaSide],
)]
pub async fn uart4_tx_worker(mut cx: uart4_tx_worker::Context) {
    loop {
        let chunk = match cx.local.uart4_tx_owner.next_chunk().await {
            Ok(chunk) => chunk,
            Err(_error) => {
                warn!("UART4 TX worker stopped before DMA start");
                return;
            }
        };

        let start_result = cx
            .shared
            .uart4_tx_dma
            .lock(|tx_dma| tx_dma.start_chunk(&chunk));
        if let Err(error) = start_result {
            let fault = match error {
                stm32_uart::UartTxStartError::InvalidChunk => SerialFault::InvalidChunk,
                stm32_uart::UartTxStartError::Busy
                | stm32_uart::UartTxStartError::TransferMissing => SerialFault::InvalidState,
            };
            cx.local.uart4_tx_owner.fail(fault);
            warn!("UART4 TX DMA start failed");
            return;
        }

        if cx.local.uart4_tx_owner.wait_completion().await.is_err() {
            warn!("UART4 TX worker stopped after DMA start");
            return;
        }
    }
}
