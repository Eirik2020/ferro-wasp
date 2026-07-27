use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, FerroError>;

#[derive(Debug, Error)]
pub enum FerroError {
    #[error("no FerroWasp USB CDC device was found")]
    DeviceNotFound,

    #[error("multiple FerroWasp devices were found: {ports:?}; select one with --port")]
    MultipleDevicesFound { ports: Vec<String> },

    #[error("could not enumerate serial ports: {0}")]
    Enumeration(String),

    #[error("could not open serial port {port}: {reason}")]
    OpenPort { port: String, reason: String },

    #[error("transport error on {port}: {reason}")]
    Transport { port: String, reason: String },

    #[error(
        "timed out waiting for FerroWasp response to `{operation}` after {attempts} attempt(s)"
    )]
    Timeout { operation: String, attempts: u8 },

    #[error("device rejected `{operation}`: {message}")]
    DeviceRejected { operation: String, message: String },

    #[error("unexpected response to `{operation}`: {response}")]
    UnexpectedResponse { operation: String, response: String },

    #[error("configuration is invalid: {0}")]
    InvalidConfiguration(String),

    #[error("could not read configuration file {path}: {reason}")]
    ReadConfig { path: PathBuf, reason: String },

    #[error("could not write configuration file {path}: {reason}")]
    WriteConfig { path: PathBuf, reason: String },

    #[error("configuration readback did not match the requested values: {details}")]
    VerificationFailed { details: String },

    #[error("firmware image is invalid: {reason}")]
    InvalidFirmware { reason: String },

    #[error("bundled dfu-util was not found beside FerroConfigurator")]
    DfuUtilityMissing,

    #[error("no STM32 ROM DFU device (0483:df11) was found")]
    DfuDeviceNotFound,

    #[error("multiple STM32 ROM DFU devices were found: {devices:?}")]
    MultipleDfuDevices { devices: Vec<String> },

    #[error("DFU operation failed during {operation}: {reason}")]
    Dfu { operation: String, reason: String },

    #[error("configuration profile error: {0}")]
    Profile(String),

    #[error("blackbox operation failed: {0}")]
    Blackbox(String),

    #[error("could not {operation} {path}: {reason}")]
    FileIo {
        operation: &'static str,
        path: PathBuf,
        reason: String,
    },

    #[error("ULog conversion failed: {0}")]
    Ulog(String),
}
