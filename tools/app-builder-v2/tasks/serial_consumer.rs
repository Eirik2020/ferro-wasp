use crate::task::{TaskDefinition, rc_input_snapshot, serial_consumer_state, uart_rx_dma};

crate::app_task! {
    pub const SERIAL_CONSUMER: TaskDefinition = TaskDefinition::asynchronous("serial_consumer")
        .with_local(&[serial_consumer_state("consumer")])
        .with_shared(&[uart_rx_dma("endpoint"), rc_input_snapshot("rc_input")]);

    async fn serial_consumer(mut cx: serial_consumer::Context) {
        let mut bytes = [0_u8; UART_RX_BUFFER_SIZE];

        loop {
            match cx.shared.endpoint.lock(|endpoint| endpoint.read_chunk(&mut bytes)) {
                UartRxReadOutcome::NoChunk => {}
                UartRxReadOutcome::RecycleError => {
                    defmt::warn!("UART RX buffer recycle error");
                }
                UartRxReadOutcome::Chunk(len) => {
                    cx.local.consumer.consume(&bytes[..len], |event| match event {
                        SerialConsumerEvent::RcSnapshot(snapshot) => {
                            cx.shared.rc_input.lock(|output| *output = snapshot);
                        }
                        SerialConsumerEvent::ComPortLine(line) => {
                            match core::str::from_utf8(line.as_slice()) {
                                Ok(line) => defmt::info!("COMPORT: {}", line),
                                Err(_) => defmt::info!("COMPORT bytes: {=[u8]}", line.as_slice()),
                            }
                        }
                        SerialConsumerEvent::ComPortOverflow => {
                            defmt::warn!("COMPORT line exceeded 64 bytes; discarded");
                        }
                    });
                }
            }

            Mono::delay(1.millis()).await;
        }
    }
}
