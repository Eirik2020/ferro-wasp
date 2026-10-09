//! The text command line on the UART config binds to `configurator`: a host
//! on a cable or a Bluetooth module, beside USB.
//!
//! Commands go to the storage owner on this link's own queue, and its answers
//! come back on this link's own queue, so USB and the UART never see each
//! other's replies. Motor commands and the unconditional log erase stay on
//! USB: a Bluetooth module can be paired by anyone in range.

use crate::prelude::*;
use crate::snapshots::*;

/// Answers leave in whole transmit chunks. The last chunk of a burst is
/// padded with NUL bytes, which a host skips.
const CHUNK_LEN: usize = stm32_memory::UART_RX_BUFFER_BYTES;
/// How often the link looks for input and answers when idle.
const POLL_MS: u64 = 2;
/// How long after its last command the link still blocks arming.
const ACTIVE_HOLD_MS: u64 = 2_000;

/// Packs answer lines into transmit chunks.
struct Outbox {
    bytes: [u8; CHUNK_LEN],
    len: usize,
    healthy: bool,
}

impl Outbox {
    async fn push(&mut self, writer: &mut stm32_memory::UartOwnedWriter<'static>, text: &[u8]) {
        let mut text = text;
        while !text.is_empty() {
            let count = (CHUNK_LEN - self.len).min(text.len());
            self.bytes[self.len..self.len + count].copy_from_slice(&text[..count]);
            self.len += count;
            text = &text[count..];
            if self.len == CHUNK_LEN {
                self.flush(writer).await;
            }
        }
    }

    async fn flush(&mut self, writer: &mut stm32_memory::UartOwnedWriter<'static>) {
        use embedded_io_async::Write;

        if self.len == 0 {
            return;
        }
        // A faulted stream stays down rather than retrying into it.
        if self.healthy && writer.write_all(&self.bytes[..self.len]).await.is_err() {
            self.healthy = false;
            warn!("Configurator link TX stopped after a writer fault");
        }
        self.len = 0;
    }
}

/// Reads commands from the configurator port and writes back the answers.
/// With no port bound the task ends at once.
#[ferroforge::task(
    local = [
        configurator_port: Option<ferrowasp_stm32::uart_port::SerialPortEndpoint>,
        configurator_commands: Option<flash_task::CommandProducer>,
        configurator_responses: Option<flash_task::ResponseConsumer>,
    ],
    monotonic = Mono,
)]
pub async fn configurator_link(cx: configurator_link::Context) {
    let (Some(port), Some(commands), Some(responses)) = (
        cx.local.configurator_port.as_mut(),
        cx.local.configurator_commands.as_mut(),
        cx.local.configurator_responses.as_mut(),
    ) else {
        return;
    };
    let Some(mut writer) = port.writer.take() else {
        warn!("Configurator port has no transmit path; link disabled");
        return;
    };
    info!("Configurator command line on {}", port.port.name());
    let mut parser = flash_task::CommandParser::new();
    let mut outbox = Outbox {
        bytes: [0; CHUNK_LEN],
        len: 0,
        healthy: true,
    };
    let mut input = [0u8; 32];
    let mut last_command_ms: Option<u64> = None;
    loop {
        // A break in the stream would splice two half lines into one command.
        if port.discontinuities.take_new().is_some() {
            parser.clear();
        }
        loop {
            let count = match port.reader.try_read(&mut input) {
                Ok(count) => count,
                Err(_) => {
                    parser.clear();
                    0
                }
            };
            if count == 0 {
                break;
            }
            for byte in &input[..count] {
                let Some(parsed) = parser.ingest(*byte) else {
                    continue;
                };
                last_command_ms = Some(Mono::now().duration_since_epoch().to_millis());
                match parsed {
                    Ok(flash_task::StorageCommand::Live) => {
                        let line = ferrowasp_flight::usb_debug::format_live(live_snapshot());
                        outbox.push(&mut writer, line.as_bytes()).await;
                    }
                    Ok(command) if command.usb_only() => {
                        outbox
                            .push(&mut writer, b"ERR USB only for this command\r\n")
                            .await;
                    }
                    Ok(command) => {
                        if commands.enqueue(command).is_err() {
                            outbox
                                .push(&mut writer, b"ERR command queue full\r\n")
                                .await;
                        }
                    }
                    Err(_) => {
                        outbox
                            .push(&mut writer, b"ERR invalid command; type help\r\n")
                            .await;
                    }
                }
            }
        }
        while let Some(frame) = responses.dequeue() {
            outbox.push(&mut writer, frame.as_bytes()).await;
        }
        outbox.flush(&mut writer).await;
        let now_ms = Mono::now().duration_since_epoch().to_millis();
        CONFIGURATOR_LINK_ACTIVE.store(
            last_command_ms.is_some_and(|last| now_ms.saturating_sub(last) < ACTIVE_HOLD_MS),
            Ordering::Release,
        );
        Mono::delay(POLL_MS.millis()).await;
    }
}
