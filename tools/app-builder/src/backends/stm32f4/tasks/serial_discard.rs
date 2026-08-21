use embedded_io_async::Read;
use ferrowasp_stm32f4::memory::UartOwnedReader;

crate::reusable_task! {
    contract {
        local {
            /// Owned byte reader whose inbound traffic is intentionally ignored.
            reader: UartOwnedReader<'static>,
        }
        shared {}
        config {}
        spawns {}
    }

    /// Keeps an unused bidirectional endpoint's bounded RX queue drained.
    pub async fn serial_discard(cx: serial_discard::Context<'_>) {
        loop {
            let mut bytes = [0; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES];
            if cx.local.reader.read(&mut bytes).await.is_err() {
                defmt::warn!("discarded serial reader stopped after a transport fault");
                return;
            }
        }
    }
}
