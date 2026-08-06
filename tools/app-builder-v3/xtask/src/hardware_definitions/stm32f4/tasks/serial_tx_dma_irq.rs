use ferrowasp_io_core::serial::SerialFault;
use ferrowasp_stm32f4::{
    memory::{UART_RX_BUFFER_BYTES, UartOwnedTxCompletion},
    serial::{UartTxDmaService, UartTxIrqOutcome},
};

/// UART TX completion handle stored locally by the DMA interrupt task.
pub type UartTxCompletion = UartOwnedTxCompletion<'static>;

/// STM32F4 UART transmit-DMA service shared by the endpoint's TX tasks.
pub type UartTxDma = dyn UartTxDmaService<UART_RX_BUFFER_BYTES> + 'static;

crate::reusable_task! {
    contract {
        local {
            /// Completion channel used to wake the endpoint TX worker.
            completion: UartTxCompletion,
        }
        shared {
            /// UART transmit DMA state shared with the endpoint TX worker.
            tx_dma: UartTxDma,
        }
        config {}
        spawns {}
    }

    /// Services one STM32F4 UART transmit-DMA interrupt.
    pub fn serial_tx_dma_irq(mut cx: serial_tx_dma_irq::Context<'_>) {
        let outcome = cx.shared.tx_dma.lock(UartTxDmaService::service_irq);

        match outcome {
            UartTxIrqOutcome::Ignored => {}
            UartTxIrqOutcome::Completed => {
                if cx.local.completion.complete().is_err() {
                    defmt::warn!("UART TX completion arrived without an in-flight chunk");
                }
            }
            UartTxIrqOutcome::DmaError(error) => {
                cx.local.completion.fail(SerialFault::DmaTransfer);
                match error {
                    ferrowasp_stm32f4::serial::UartTxDmaError::Transfer => {
                        defmt::warn!("UART TX DMA transfer error")
                    }
                    ferrowasp_stm32f4::serial::UartTxDmaError::DirectMode => {
                        defmt::warn!("UART TX DMA direct-mode error")
                    }
                }
            }
        }
    }
}
