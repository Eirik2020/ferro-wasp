#![forbid(unsafe_code)]
//! Bluetooth Low Energy transports for FerroConfigurator.
//!
//! Some boards carry a Bluetooth module that bridges their UART to BLE GATT
//! characteristics - the SpeedyBee F405 V5 is the one this was built
//! against, and its link is documented in the repository's internal book.
//! This crate turns that pipe into a [`LineTransport`]: the same line
//! protocol the CLI already speaks over USB, without a cable.
//!
//! The bridge authenticates nobody - no pairing, no PIN, no encryption - so
//! the disarmed-only, read-back-verified discipline the core enforces over
//! USB applies unchanged over Bluetooth. Nothing here adds arming or
//! actuator authority; [`open`] opens the pipe and resynchronises, exactly
//! as the USB path does.
//!
//! The tests are software-only: [`BleTransport`] is driven over plain
//! channels, and the adapter glue that would need real hardware stays thin.

use std::{
    io,
    sync::mpsc::{Receiver, Sender, channel},
    thread,
    time::{Duration, Instant},
};

use btleplug::api::{
    Central, Manager as _, Peripheral as _, ScanFilter, WriteType, bleuuid::uuid_from_u16,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use ferro_configurator_core::{FerroClient, FerroError, LineReader, LineTransport, Result};
use futures::StreamExt;
use uuid::Uuid;

/// The bridge's UART-write characteristic (short UUID `ABF1`): what the host
/// writes here reaches the flight controller's UART unchanged.
const BRIDGE_UART_WRITE: Uuid = uuid_from_u16(0xABF1);
/// The bridge's UART-notify characteristic (short UUID `ABF2`): the UART's
/// bytes, relayed to the host as notifications, in order, unchanged.
const BRIDGE_UART_NOTIFY: Uuid = uuid_from_u16(0xABF2);

/// How long a write waits for the Bluetooth stack to accept it.
const WRITE_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// A Bluetooth device as a scan sees it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct BleBridgeInfo {
    pub address: String,
    pub name: String,
}

/// Lists nearby Low Energy devices for `duration`, with names where they
/// offer one. No connection is made; nothing is sent to anything.
pub fn scan(duration: Duration) -> Result<Vec<BleBridgeInfo>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| bluetooth_error("the host", error))?;
    runtime.block_on(scan_once(duration))
}

async fn scan_once(duration: Duration) -> Result<Vec<BleBridgeInfo>> {
    let adapter = adapter()
        .await
        .map_err(|reason| bluetooth_error("the host", reason))?;
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(|error| bluetooth_error("the host", error))?;
    tokio::time::sleep(duration).await;
    let mut found = Vec::new();
    for peripheral in adapter.peripherals().await.unwrap_or_default() {
        if let Ok(Some(properties)) = peripheral.properties().await {
            found.push(BleBridgeInfo {
                address: properties.address.to_string(),
                name: properties.local_name.unwrap_or_default(),
            });
        }
    }
    let _ = adapter.stop_scan().await;
    found.sort();
    Ok(found)
}

async fn adapter() -> std::result::Result<Adapter, String> {
    let manager = Manager::new().await.map_err(|error| error.to_string())?;
    manager
        .adapters()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "no Bluetooth adapter was found".to_owned())
}

fn bluetooth_error(target: &str, reason: impl std::fmt::Display) -> FerroError {
    FerroError::OpenBluetooth {
        target: target.to_owned(),
        reason: reason.to_string(),
    }
}

/// An address-like target (it contains a colon) must match the device's
/// address exactly, ignoring case; any other target matches a device whose
/// advertised name contains it, so `F405 V5` finds the board by name.
fn matches_target(target: &str, address: &str, name: &str) -> bool {
    if target.contains(':') {
        address.eq_ignore_ascii_case(target)
    } else {
        name.to_ascii_lowercase()
            .contains(&target.to_ascii_lowercase())
    }
}

enum SessionCommand {
    Write(Vec<u8>, Sender<io::Result<()>>),
    Shutdown,
}

/// Connects the bridge named by `target` and hands back a client over its
/// UART pipe, resynchronised the same way the USB path does. `target` is a
/// MAC address or a fragment of the advertised name.
pub fn open(target: &str, timeout: Duration) -> Result<FerroClient<Box<dyn LineTransport>>> {
    let transport = connect_transport(target, timeout)?;
    let transport: Box<dyn LineTransport> = Box::new(transport);
    let mut client = FerroClient::new(transport, timeout);
    client.resynchronize()?;
    Ok(client)
}

fn connect_transport(target: &str, timeout: Duration) -> Result<BleTransport> {
    let (chunk_tx, chunk_rx) = channel::<Vec<u8>>();
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel::<SessionCommand>();
    let (ready_tx, ready_rx) = channel::<std::result::Result<(), String>>();
    let owned_target = target.to_owned();
    let session = thread::Builder::new()
        .name("ferro-configurator-ble".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    return;
                }
            };
            runtime.block_on(run_session(
                &owned_target,
                timeout,
                chunk_tx,
                command_rx,
                ready_tx,
            ));
        });
    if session.is_err() {
        return Err(bluetooth_error(
            target,
            "could not start the session thread",
        ));
    }
    let settled = ready_rx.recv_timeout(timeout + Duration::from_secs(2));
    match settled {
        Ok(Ok(())) => Ok(BleTransport::from_parts(
            command_tx,
            chunk_rx,
            format!("bluetooth {target}"),
        )),
        Ok(Err(reason)) => Err(bluetooth_error(target, reason)),
        Err(_) => Err(bluetooth_error(
            target,
            format!("no bridge came up within {} s", timeout.as_secs().max(2)),
        )),
    }
}

/// Connects, finds the two UART characteristics, subscribes, then serves
/// writes until shutdown. Every failure before the subscription lands on
/// `ready`; after it, failures surface through each write's acknowledgement
/// or as a closed pipe on the read side.
async fn run_session(
    target: &str,
    timeout: Duration,
    chunk_tx: Sender<Vec<u8>>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<SessionCommand>,
    ready: Sender<std::result::Result<(), String>>,
) {
    let connected = async {
        let adapter = adapter().await?;
        let peripheral = find_bridge(&adapter, target, timeout).await?;
        peripheral
            .connect()
            .await
            .map_err(|error| error.to_string())?;
        peripheral
            .discover_services()
            .await
            .map_err(|error| error.to_string())?;
        let characteristics = peripheral.characteristics();
        let write = characteristics
            .iter()
            .find(|characteristic| characteristic.uuid == BRIDGE_UART_WRITE)
            .ok_or_else(|| {
                "the bridge's UART write characteristic (ABF1) was not found".to_owned()
            })?
            .clone();
        let notify = characteristics
            .iter()
            .find(|characteristic| characteristic.uuid == BRIDGE_UART_NOTIFY)
            .ok_or_else(|| {
                "the bridge's UART notify characteristic (ABF2) was not found".to_owned()
            })?
            .clone();
        peripheral
            .subscribe(&notify)
            .await
            .map_err(|error| error.to_string())?;
        let notifications = peripheral
            .notifications()
            .await
            .map_err(|error| error.to_string())?;
        std::result::Result::<(Peripheral, btleplug::api::Characteristic, _), String>::Ok((
            peripheral,
            write,
            notifications,
        ))
    };
    match connected.await {
        Ok((peripheral, write, mut notifications)) => {
            let forwarder = tokio::spawn(async move {
                while let Some(notification) = notifications.next().await {
                    let _ = chunk_tx.send(notification.value);
                }
                // The stream ends with the link; the sender drops with this
                // task, so the transport sees a closed pipe, not a mute one.
            });
            let _ = ready.send(Ok(()));
            while let Some(command) = commands.recv().await {
                match command {
                    SessionCommand::Write(bytes, acknowledge) => {
                        let result = peripheral
                            .write(&write, &bytes, WriteType::WithResponse)
                            .await;
                        let _ = acknowledge
                            .send(result.map_err(|error| io::Error::other(error.to_string())));
                    }
                    SessionCommand::Shutdown => break,
                }
            }
            forwarder.abort();
            let _ = peripheral.disconnect().await;
        }
        Err(reason) => {
            let _ = ready.send(Err(reason));
        }
    }
}

async fn find_bridge(
    adapter: &Adapter,
    target: &str,
    timeout: Duration,
) -> std::result::Result<Peripheral, String> {
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(|error| error.to_string())?;
    tokio::time::sleep(timeout).await;
    let mut found = None;
    for peripheral in adapter
        .peripherals()
        .await
        .map_err(|error| error.to_string())?
    {
        let Ok(Some(properties)) = peripheral.properties().await else {
            continue;
        };
        let address = properties.address.to_string();
        let name = properties.local_name.unwrap_or_default();
        if matches_target(target, &address, &name) {
            found = Some(peripheral);
            break;
        }
    }
    let _ = adapter.stop_scan().await;
    found.ok_or_else(|| format!("no Bluetooth device matched {target:?}"))
}

/// The bridge's UART pipe as a line transport, over the session's channels.
///
/// One thread owns the Bluetooth stack; this half only talks to it through
/// the command channel and the notification channel, which is why the whole
/// transport is testable with no adapter at all.
pub struct BleTransport {
    commands: tokio::sync::mpsc::UnboundedSender<SessionCommand>,
    chunks: Receiver<Vec<u8>>,
    reader: LineReader,
    name: String,
}

impl BleTransport {
    fn from_parts(
        commands: tokio::sync::mpsc::UnboundedSender<SessionCommand>,
        chunks: Receiver<Vec<u8>>,
        name: String,
    ) -> Self {
        Self {
            commands,
            chunks,
            reader: LineReader::with_capacity(512),
            name,
        }
    }
}

impl Drop for BleTransport {
    fn drop(&mut self) {
        // The session thread exits with the link; leaving it running would
        // hold the Bluetooth connection open past the client's lifetime.
        let _ = self.commands.send(SessionCommand::Shutdown);
    }
}

impl LineTransport for BleTransport {
    fn write_line(&mut self, line: &str) -> Result<()> {
        let mut bytes = Vec::with_capacity(line.len() + 2);
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\r\n");
        let (acknowledge_tx, acknowledge_rx) = channel();
        self.commands
            .send(SessionCommand::Write(bytes, acknowledge_tx))
            .map_err(|_| closed(&self.name))?;
        match acknowledge_rx.recv_timeout(WRITE_ACK_TIMEOUT) {
            Ok(result) => result.map_err(|error| FerroError::Transport {
                port: self.name.clone(),
                reason: error.to_string(),
            }),
            Err(_) => Err(closed(&self.name)),
        }
    }

    fn read_line(&mut self, timeout: Duration) -> Result<Option<String>> {
        let deadline = Instant::now() + timeout;
        loop {
            while let Ok(chunk) = self.chunks.try_recv() {
                self.reader.push(&chunk, &self.name)?;
                if let Some(line) = self.reader.next_line() {
                    return Ok(Some(line));
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            let wait = (deadline - now).min(Duration::from_millis(20));
            match self.chunks.recv_timeout(wait) {
                Ok(chunk) => {
                    self.reader.push(&chunk, &self.name)?;
                    if let Some(line) = self.reader.next_line() {
                        return Ok(Some(line));
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(closed(&self.name));
                }
            }
        }
    }

    fn description(&self) -> &str {
        &self.name
    }
}

fn closed(name: &str) -> FerroError {
    FerroError::Transport {
        port: name.to_owned(),
        reason: "the Bluetooth session is closed".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transport wired to channels pretending to be a session, so no
    /// test needs an adapter or a device.
    fn fake_session() -> (
        BleTransport,
        tokio::sync::mpsc::UnboundedReceiver<SessionCommand>,
        Sender<Vec<u8>>,
    ) {
        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (chunk_tx, chunk_rx) = channel();
        (
            BleTransport::from_parts(command_tx, chunk_rx, "test bridge".to_owned()),
            command_rx,
            chunk_tx,
        )
    }

    #[test]
    fn a_line_arriving_in_pieces_is_read_whole() {
        let (mut transport, _commands, chunks) = fake_session();
        chunks.send(b"OK live ar".to_vec()).unwrap();
        assert_eq!(
            transport.read_line(Duration::from_millis(10)).unwrap(),
            None
        );
        chunks.send(b"med=0\r\n".to_vec()).unwrap();
        assert_eq!(
            transport.read_line(Duration::from_millis(100)).unwrap(),
            Some("OK live armed=0".to_owned())
        );
    }

    #[test]
    fn write_line_frames_with_crlf_and_takes_the_reply() {
        let (mut transport, mut commands, _chunks) = fake_session();
        // The session acknowledges from its own thread, as the real one does;
        // write_line waits for that reply.
        let responder = thread::spawn(move || {
            let command = commands.blocking_recv().expect("the session saw the write");
            let SessionCommand::Write(bytes, acknowledge) = command else {
                panic!("expected a write command");
            };
            assert_eq!(bytes, b"status\r\n".to_vec());
            acknowledge.send(Ok(())).unwrap();
        });
        transport.write_line("status").unwrap();
        responder.join().unwrap();
    }

    #[test]
    fn a_failed_write_surfaces_the_reason() {
        let (mut transport, mut commands, _chunks) = fake_session();
        let responder = thread::spawn(move || {
            let command = commands.blocking_recv().expect("the session saw the write");
            match command {
                SessionCommand::Write(_, acknowledge) => {
                    acknowledge
                        .send(Err(io::Error::other("link dropped")))
                        .unwrap();
                }
                SessionCommand::Shutdown => panic!("expected a write command"),
            }
        });
        let error = transport.write_line("status").unwrap_err();
        responder.join().unwrap();
        let FerroError::Transport { port, reason } = &error else {
            panic!("expected a transport error, got {error:?}");
        };
        assert_eq!(port, "test bridge");
        assert!(reason.contains("link dropped"));
    }

    #[test]
    fn a_closed_session_fails_loudly() {
        let (mut transport, commands, chunks) = fake_session();
        drop(commands);
        drop(chunks);
        assert!(transport.write_line("status").is_err());
        assert!(transport.read_line(Duration::from_millis(10)).is_err());
    }

    #[test]
    fn dropping_the_transport_shuts_the_session_down() {
        let (transport, mut commands, _chunks) = fake_session();
        drop(transport);
        let command = commands.try_recv().expect("the session was told to stop");
        assert!(matches!(command, SessionCommand::Shutdown));
    }

    #[test]
    fn a_target_matches_by_address_or_by_name_fragment() {
        assert!(matches_target(
            "F0:16:1D:C9:ED:56",
            "f0:16:1d:c9:ed:56",
            "F405 V5",
        ));
        assert!(!matches_target(
            "F0:16:1D:C9:ED:56",
            "11:22:33:44:55:66",
            "F405 V5",
        ));
        assert!(matches_target("405 v5", "11:22:33:44:55:66", "F405 V5"));
        assert!(!matches_target("SpeedyBee", "11:22:33:44:55:66", "F405 V5"));
    }
}
