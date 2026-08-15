use crate::task::{TaskDefinition, line_consumer_state, serial_rx};

crate::app_task! {
    pub const COMPORT: TaskDefinition = TaskDefinition::asynchronous("comport")
        .with_local(&[line_consumer_state("decoder")])
        .with_shared(&[serial_rx("rx")]);

    async fn comport(mut cx: comport::Context) {
        let mut bytes = [0_u8; UART_RX_BUFFER_SIZE];

        loop {
            match cx.shared.rx.lock(|rx| rx.read_chunk(&mut bytes)) {
                UartRxReadOutcome::NoChunk => {}
                UartRxReadOutcome::RecycleError => {
                    defmt::warn!("serial RX buffer recycle error");
                }
                UartRxReadOutcome::Chunk(len) => {
                    cx.local.decoder.consume(&bytes[..len], |event| match event {
                        LineConsumerEvent::Line(line) => {
                            match core::str::from_utf8(line.as_slice()) {
                                Ok(line) => defmt::info!("COMPORT: {}", line),
                                Err(_) => defmt::info!("COMPORT bytes: {=[u8]}", line.as_slice()),
                            }
                        }
                        LineConsumerEvent::Overflow => {
                            defmt::warn!("COMPORT line exceeded 64 bytes; discarded");
                        }
                    });
                }
            }

            Mono::delay(1.millis()).await;
        }
    }
}
