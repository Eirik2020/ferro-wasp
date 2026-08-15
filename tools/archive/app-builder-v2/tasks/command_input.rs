use crate::task::{TaskDefinition, rc_input_observer_publisher, sbus_consumer_state, serial_rx};

crate::app_task! {
    pub const COMMAND_INPUT: TaskDefinition = TaskDefinition::asynchronous("command_input")
        .with_local(&[
            sbus_consumer_state("decoder"),
            rc_input_observer_publisher("rc_observer"),
        ])
        .with_shared(&[serial_rx("rx")]);

    async fn command_input(mut cx: command_input::Context) {
        let mut bytes = [0_u8; UART_RX_BUFFER_SIZE];

        loop {
            match cx.shared.rx.lock(|rx| rx.read_chunk(&mut bytes)) {
                UartRxReadOutcome::NoChunk => {}
                UartRxReadOutcome::RecycleError => {
                    defmt::warn!("serial RX buffer recycle error");
                }
                UartRxReadOutcome::Chunk(len) => {
                    cx.local.decoder.consume(&bytes[..len], |snapshot| {
                        cx.local.rc_observer.publish(snapshot);
                    });
                }
            }

            Mono::delay(1.millis()).await;
        }
    }
}
