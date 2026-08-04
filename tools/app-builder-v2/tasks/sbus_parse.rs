use crate::task::{TaskDefinition, uart_rx_dma};

crate::app_task! {
    pub const SBUS_PARSE: TaskDefinition = TaskDefinition::asynchronous("sbus_parse")
        .with_shared(&[uart_rx_dma("rx")]);

    async fn sbus_parse(mut cx: sbus_parse::Context) {
        let mut parser = StreamingParser::new();
        let mut bytes = [0_u8; UART_RX_BUFFER_SIZE];

        loop {
            match cx.shared.rx.lock(|rx| rx.read_chunk(&mut bytes)) {
                UartRxReadOutcome::NoChunk => {}
                UartRxReadOutcome::RecycleError => {
                    defmt::warn!("SBUS RX buffer recycle error");
                }
                UartRxReadOutcome::Chunk(len) => {
                    for packet in parser.push_bytes(&bytes[..len]) {
                        match packet {
                            Ok(packet) => defmt::info!(
                                "SBUS channels: [{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}], d1={}, d2={}, frame_lost={}, failsafe={}",
                                packet.channels[0], packet.channels[1], packet.channels[2],
                                packet.channels[3], packet.channels[4], packet.channels[5],
                                packet.channels[6], packet.channels[7], packet.channels[8],
                                packet.channels[9], packet.channels[10], packet.channels[11],
                                packet.channels[12], packet.channels[13], packet.channels[14],
                                packet.channels[15], packet.flags.d1, packet.flags.d2,
                                packet.flags.frame_lost, packet.flags.failsafe,
                            ),
                            Err(_) => defmt::warn!("Invalid SBUS frame"),
                        }
                    }
                }
            }

            Mono::delay(1.millis()).await;
        }
    }
}
