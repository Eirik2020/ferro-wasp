#![forbid(unsafe_code)]
//! Host-side configuration support for FerroWasp controllers.
//!
//! The current firmware exposes a bounded ASCII storage protocol over USB CDC.
//! This crate keeps that transport behind a small client API and does not give
//! the host any arming or actuator authority.

pub mod blackbox;
pub mod client;
pub mod config;
pub mod device;
pub mod error;
pub mod firmware;
pub mod profiles;
pub mod transport;
pub mod ulog;

pub use blackbox::{
    CatalogEntry, DownloadSummary, FlightCatalog, FlightSelector, FlightSpan, catalog_device,
    download_flight, resolve_device_flight,
};
pub use client::{FerroClient, FlashInfo, LogInfo, StatusSnapshot};
pub use config::{AxisPid, ConfigKey, ConfigValidationError, FerroConfig};
pub use device::{DeviceSelector, PortInfo, discover_ports, open_device};
pub use error::{FerroError, Result};
pub use firmware::{
    BoardProfile, BundledFirmware, DfuDetection, FirmwareArtifact, FlashProgress, FlashResult,
    PreparedImage, ReleaseManifest, detect_dfu, find_bundled_firmware, flash_firmware,
    load_bundled_firmware, prepare_elf, sha256_file,
};
pub use profiles::{ProfileInfo, ProfileStore};
pub use transport::{LineTransport, MockTransport, SerialTransport};
pub use ulog::{ConversionSummary, convert_fwbb_to_ulog};
