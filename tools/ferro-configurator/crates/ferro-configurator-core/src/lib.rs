#![forbid(unsafe_code)]
//! Host-side configuration support for FerroWasp controllers.
//!
//! The current firmware exposes a bounded ASCII storage protocol over USB CDC.
//! This crate keeps that transport behind a small client API and does not give
//! the host any arming or actuator authority.

pub mod blackbox;
pub mod client;
pub mod config;
pub mod csv_export;
pub mod device;
pub mod error;
pub mod firmware;
pub mod prearm;
pub mod profiles;
pub mod transport;
pub mod ulog;

pub use blackbox::{
    CatalogEntry, DownloadSummary, FlightCatalog, FlightSelector, FlightSpan, SyncedFlight,
    catalog_device, download_flight, resolve_device_flight, sync_file_name, sync_flights,
};
pub use client::{
    AccelLevel, FerroClient, FlashInfo, LiveSnapshot, LogInfo, SERIAL_FUNCTIONS, SerialBindings,
    SerialPortBinding, StatusSnapshot, UnsyncedFlight, UnsyncedFlights,
};
pub use config::{AxisPid, ConfigKey, ConfigValidationError, FerroConfig, RcProtocol};
pub use csv_export::{CsvSummary, convert_fwbb_to_csv};
pub use device::{DeviceSelector, PortInfo, discover_ports, open_device};
pub use error::{FerroError, Result};
pub use firmware::{
    BoardProfile, BundledFirmware, DFU_ENTRY_TIMEOUT, DfuDetection, FirmwareArtifact,
    FlashProgress, FlashResult, PreparedImage, RESTART_TIMEOUT, ReleaseManifest, detect_dfu,
    enter_rom_bootloader, find_board_port, find_bundled_firmware, flash_firmware,
    load_bundled_firmware, prepare_elf, sha256_file, wait_for_board_port, wait_for_dfu_device,
};
pub use prearm::{CheckState, PrearmCheck, prearm_checks};
pub use profiles::{ProfileInfo, ProfileStore};
pub use transport::{LineTransport, MockTransport, SerialTransport};
pub use ulog::{ConversionSummary, convert_fwbb_to_ulog};
