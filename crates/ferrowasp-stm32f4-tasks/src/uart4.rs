//! UART4, the OSD link: receive by DMA and idle line, and transmit queued
//! chunks by DMA.

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

/// UART4 RX DMA transfer complete: hand the filled buffer on and wake the OSD.
/// `uart4_rx` is lock-free, shared with the idle-line handler at one priority.
#[ferroforge::task(
    shared = [#[lock_free] uart4_rx: stm32_uart::Uart4RxIrq],
    spawn = [osd_refresh()],
)]
pub fn uart4_rx_dma_transfer(cx: uart4_rx_dma_transfer::Context) {
    let uart = cx.shared.uart4_rx;
    let delivered = match uart.service_dma_irq() {
        stm32_uart::UartRxIrqOutcome::Delivered => true,
        stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
        stm32_uart::UartRxIrqOutcome::DmaError => {
            warn!("UART4 RX DMA error");
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::NoFreshBuffer,
        ) => {
            panic!("UART4 RX free-buffer pool exhausted");
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::TransferNotReady,
        ) => {
            warn!("UART4 DMA next_transfer failed");
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::FilledQueueFull,
        ) => {
            panic!("UART4 filled queue full; RX buffer ownership would be lost");
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::PlannerRejected,
        ) => false,
    };

    if delivered {
        let _ = cx.spawn.osd_refresh();
    }
}

/// UART4 idle line: deliver the partly filled buffer and wake the OSD.
#[ferroforge::task(
    shared = [#[lock_free] uart4_rx: stm32_uart::Uart4RxIrq],
    spawn = [osd_refresh()],
)]
pub fn uart4_rx_peripheral(cx: uart4_rx_peripheral::Context) {
    let uart = cx.shared.uart4_rx;

    let delivered = match uart.service_idle_irq() {
        stm32_uart::UartRxIrqOutcome::Delivered => true,
        stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
        stm32_uart::UartRxIrqOutcome::DmaError => false,
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::NoFreshBuffer,
        ) => {
            warn!("UART4 RX free-buffer pool exhausted on IDLE");
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::TransferNotReady,
        ) => {
            warn!("UART4 IDLE next_transfer failed");
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::FilledQueueFull,
        ) => {
            panic!("UART4 filled queue full; RX buffer ownership would be lost");
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::PlannerRejected,
        ) => {
            panic!("UART4 RX IDLE planner did not deliver a non-empty buffer");
        }
    };

    if delivered {
        let _ = cx.spawn.osd_refresh();
    }
}

/// UART4 TX DMA complete: finish or fail the in-flight chunk.
#[ferroforge::task(
    local = [uart4_tx_completion: stm32_memory::UartOwnedTxCompletion<'static>],
    shared = [uart4_tx_dma: stm32_uart::Uart4TxDmaSide],
)]
pub fn uart4_tx_dma_transfer(mut cx: uart4_tx_dma_transfer::Context) {
    let outcome = cx
        .shared
        .uart4_tx_dma
        .lock(stm32_uart::Uart4TxDmaSide::service_irq);

    match outcome {
        stm32_uart::UartTxIrqOutcome::Ignored => {}
        stm32_uart::UartTxIrqOutcome::Completed => {
            if cx.local.uart4_tx_completion.complete().is_err() {
                warn!("UART4 TX completion arrived without an in-flight chunk");
            }
        }
        stm32_uart::UartTxIrqOutcome::DmaError(error) => {
            cx.local.uart4_tx_completion.fail(SerialFault::DmaTransfer);
            match error {
                stm32_uart::UartTxDmaError::Transfer => {
                    warn!("UART4 TX DMA transfer error")
                }
                stm32_uart::UartTxDmaError::DirectMode => {
                    warn!("UART4 TX DMA direct-mode error")
                }
            }
        }
    }
}
