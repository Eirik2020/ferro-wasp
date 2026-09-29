#![forbid(unsafe_code)]
//! A front-end-agnostic session over [`ferro_configurator_core`].
//!
//! The CLI is one front-end over the core; this crate is what a second one
//! needs and the CLI does not: a long-lived connection, every result shaped
//! for serialization, and the disarmed precondition enforced in one place
//! rather than at each call site.
//!
//! It deliberately holds no Tauri, HTTP or UI dependency, so the whole session
//! can be driven against [`ferro_configurator_core::MockTransport`] in tests
//! with no device and no webview.
//!
//! # Authority
//!
//! The core states that it gives the host no arming or actuator authority, and
//! this crate does not add any. The protocol has no disarm command, so
//! "forcing disarm" here means refusing to write while the controller reports
//! itself armed - see [`Session::require_disarmed`]. Nothing in this crate can
//! arm a controller, spin a motor, or clear a safety latch.

use std::{path::Path, time::Duration};

use ferro_configurator_core::{
    CatalogEntry, DeviceSelector, DownloadSummary, FerroClient, FerroConfig, FerroError, FlashInfo,
    FlightCatalog, FlightSelector, LineTransport, LogInfo, PortInfo, SerialTransport,
    StatusSnapshot, catalog_device, discover_ports, download_flight, open_device,
    resolve_device_flight,
};
use serde::Serialize;

/// How long to wait for one line of device response.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// Every way a session call can fail, in a shape a front-end can render.
///
/// `Refused` is the interesting one: it is not a device or transport failure
/// but this crate declining to act, and a front-end should present it as a
/// safety state rather than an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BridgeError {
    /// No device is open. The front-end should offer to connect.
    NotConnected,
    /// The controller reports itself armed, so a write was refused.
    Refused { reason: String },
    /// The device, transport or host filesystem failed.
    Device { message: String },
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => f.write_str("not connected to a controller"),
            Self::Refused { reason } => write!(f, "refused: {reason}"),
            Self::Device { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<FerroError> for BridgeError {
    fn from(error: FerroError) -> Self {
        Self::Device {
            message: error.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, BridgeError>;

/// What the front-end shows before it lets anyone change anything.
///
/// Serialized snake_case like the core types it carries, so the whole wire
/// format is one convention and the front-end needs no field mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Safety {
    /// The controller's own report.
    pub status: StatusSnapshot,
    /// Whether this session would currently accept a write.
    pub writes_allowed: bool,
}

/// One connected controller, or none.
///
/// Generic over the transport so tests drive the same code as the application.
/// Only [`Session<SerialTransport>`] can open a real port.
pub struct Session<T: LineTransport = SerialTransport> {
    client: Option<FerroClient<T>>,
}

impl<T: LineTransport> Default for Session<T> {
    fn default() -> Self {
        Self { client: None }
    }
}

impl<T: LineTransport> Session<T> {
    /// An empty session. Call `connect` before anything else.
    pub fn new() -> Self {
        Self::default()
    }

    /// A session over an already-open client, which is how tests inject a
    /// [`ferro_configurator_core::MockTransport`].
    pub fn from_client(client: FerroClient<T>) -> Self {
        Self {
            client: Some(client),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// Drops the client, which closes the port.
    pub fn disconnect(&mut self) {
        self.client = None;
    }

    /// How the open device describes itself, for the title bar.
    pub fn description(&self) -> Option<&str> {
        self.client.as_ref().map(FerroClient::description)
    }

    fn client_mut(&mut self) -> Result<&mut FerroClient<T>> {
        self.client.as_mut().ok_or(BridgeError::NotConnected)
    }

    /// The controller's live safety status, and whether this session would
    /// accept a write right now.
    pub fn safety(&mut self) -> Result<Safety> {
        let status = self.client_mut()?.read_status_fresh()?;
        Ok(Safety {
            writes_allowed: !status.armed,
            status,
        })
    }

    /// Refuses unless the controller reports itself disarmed.
    ///
    /// Read fresh every time rather than cached: the arm switch can move
    /// between a front-end's last poll and the moment a write is submitted,
    /// and a stale `disarmed` is exactly the wrong thing to trust.
    pub fn require_disarmed(&mut self) -> Result<StatusSnapshot> {
        // Fresh, never cached: FerroClient::read_status answers from its cache
        // when it has one, and a snapshot taken before the operator touched the
        // arm switch is exactly what must not authorise a write.
        let status = self.client_mut()?.read_status_fresh()?;
        if status.armed {
            return Err(BridgeError::Refused {
                reason: "the controller is armed; disarm it before changing configuration"
                    .to_owned(),
            });
        }
        Ok(status)
    }

    pub fn flash_info(&mut self) -> Result<FlashInfo> {
        Ok(self.client_mut()?.flash_info()?)
    }

    pub fn log_info(&mut self) -> Result<LogInfo> {
        Ok(self.client_mut()?.log_info()?)
    }

    /// The complete active configuration. A read, so it is allowed while armed.
    pub fn read_config(&mut self) -> Result<FerroConfig> {
        Ok(self.client_mut()?.read_config()?)
    }

    /// Stages, persists and verifies a configuration.
    ///
    /// Gated on [`Session::require_disarmed`]. The returned configuration is
    /// what the controller read back, not what was sent.
    pub fn apply_config(&mut self, config: &FerroConfig) -> Result<FerroConfig> {
        self.require_disarmed()?;
        Ok(self.client_mut()?.apply_config(config)?)
    }

    /// The stored flights, grouped by recorded boot session.
    pub fn flights(&mut self) -> Result<FlightCatalog> {
        Ok(catalog_device(self.client_mut()?)?)
    }

    /// Downloads one flight, reporting `(page, total)` as it goes.
    ///
    /// A read of the log store, so it is not gated on the disarmed state; it
    /// is merely slow, which is why it reports progress.
    pub fn download_flight(
        &mut self,
        selector: FlightSelector,
        output: &Path,
        resume: bool,
        progress: impl FnMut(u32, u32),
    ) -> Result<DownloadSummary> {
        let client = self.client_mut()?;
        // The resolver also hands back the storage summary it had to read; the
        // download only needs the span.
        let (_storage, span) = resolve_device_flight(client, selector)?;
        Ok(download_flight(client, span, output, resume, progress)?)
    }
}

impl Session<SerialTransport> {
    /// Opens a controller, replacing any currently open one.
    pub fn connect(&mut self, selector: DeviceSelector, timeout: Duration) -> Result<String> {
        // Drop the old client first so its port is closed before the new open,
        // which matters when reconnecting to the same port.
        self.client = None;
        let client = open_device(selector, timeout)?;
        let description = client.description().to_owned();
        self.client = Some(client);
        Ok(description)
    }
}

/// Serial ports the host can see, FerroWasp or not.
///
/// Free of any session, because a front-end needs it before connecting.
pub fn ports() -> Result<Vec<PortInfo>> {
    Ok(discover_ports()?)
}

/// The flights in a catalog, for a front-end that renders a list.
pub fn catalog_entries(catalog: &FlightCatalog) -> &[CatalogEntry] {
    &catalog.flights
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferro_configurator_core::MockTransport;

    /// The firmware answers `status` with one bounded ASCII line. These are
    /// the two states the gate turns on.
    fn status_line(armed: bool) -> String {
        format!(
            "FWDBG1 ms=1000 imu=icm42688p ready=1 rc=1 armable=1 thr=0 arm_sw=0 armed={} vbat_dV=251",
            u8::from(armed)
        )
    }

    fn session_answering(lines: &[String]) -> Session<MockTransport> {
        let transport = MockTransport::with_lines(lines.iter().cloned());
        Session::from_client(FerroClient::new(transport, DEFAULT_TIMEOUT))
    }

    #[test]
    fn a_session_starts_disconnected_and_refuses_work() {
        let mut session: Session<MockTransport> = Session::new();
        assert!(!session.is_connected());
        assert_eq!(session.safety().unwrap_err(), BridgeError::NotConnected);
    }

    #[test]
    fn an_armed_controller_refuses_a_write() {
        let mut session = session_answering(&[status_line(true)]);
        let error = session.require_disarmed().unwrap_err();
        assert!(
            matches!(error, BridgeError::Refused { .. }),
            "an armed controller must refuse, got {error:?}"
        );
    }

    #[test]
    fn a_disarmed_controller_allows_a_write() {
        let mut session = session_answering(&[status_line(false)]);
        let status = session.require_disarmed().expect("disarmed must pass");
        assert!(!status.armed);
    }

    #[test]
    fn safety_reports_whether_writes_are_allowed() {
        let mut session = session_answering(&[status_line(true)]);
        let safety = session.safety().expect("status must parse");
        assert!(safety.status.armed);
        assert!(
            !safety.writes_allowed,
            "an armed controller must not report writes allowed"
        );
    }

    /// The gate reads status fresh for every write. If it cached the first
    /// answer, a controller armed after connecting would still be written to.
    #[test]
    fn the_gate_reads_status_fresh_for_every_write() {
        let mut session = session_answering(&[status_line(false), status_line(true)]);
        session
            .require_disarmed()
            .expect("the first read is disarmed");
        let error = session
            .require_disarmed()
            .expect_err("the second read is armed and must refuse");
        assert!(matches!(error, BridgeError::Refused { .. }));
    }

    #[test]
    fn disconnecting_closes_the_session() {
        let mut session = session_answering(&[status_line(false)]);
        assert!(session.is_connected());
        session.disconnect();
        assert!(!session.is_connected());
        assert_eq!(session.safety().unwrap_err(), BridgeError::NotConnected);
    }
}
