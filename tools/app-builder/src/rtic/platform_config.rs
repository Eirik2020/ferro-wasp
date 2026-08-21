//! Boot-time assignment of application services to hardware-neutral connectors.
//!
//! Endpoint declarations establish the physical RTIC interrupt topology. This
//! configuration is loaded during `init` and selects the protocol/service that
//! owns each endpoint's portable handles for that boot.

use std::collections::BTreeSet;

pub use ferrowasp_io_core::platform_config::{ImuInstallationId, SerialService, SpiService};

/// Application-facing serial connector identifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SerialPort {
    /// First configurable serial connector.
    Serial1,
    /// Second configurable serial connector.
    Serial2,
}

/// Application-facing SPI connector identifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SpiPort {
    /// First configurable SPI connector.
    Spi1,
}

/// One boot-time serial service-to-connector assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialAssignment {
    /// Application-facing connector.
    pub port: SerialPort,
    /// Service activated on the connector at boot.
    pub service: SerialService,
}

impl SerialAssignment {
    /// Creates one boot-time serial assignment.
    pub const fn new(port: SerialPort, service: SerialService) -> Self {
        Self { port, service }
    }
}

/// One boot-time SPI service-to-connector assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiAssignment {
    /// Application-facing connector.
    pub port: SpiPort,
    /// Service activated on the connector at boot.
    pub service: SpiService,
}

impl SpiAssignment {
    /// Creates one boot-time SPI assignment.
    pub const fn new(port: SpiPort, service: SpiService) -> Self {
        Self { port, service }
    }
}

/// Complete set of service assignments loaded by generated RTIC `init`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformConfig {
    /// Serial connector assignments.
    pub serial: [Option<SerialAssignment>; 2],
    /// SPI connector assignments.
    pub spi: [Option<SpiAssignment>; 1],
}

impl PlatformConfig {
    /// Creates a platform configuration with every connector unassigned.
    pub const fn empty() -> Self {
        Self::new([None, None], [None])
    }

    /// Creates a platform configuration without performing hardware access.
    pub const fn new(
        serial: [Option<SerialAssignment>; 2],
        spi: [Option<SpiAssignment>; 1],
    ) -> Self {
        Self { serial, spi }
    }

    /// Assigns the first serial connector.
    pub const fn serial1(mut self, service: SerialService) -> Self {
        self.serial[0] = Some(SerialAssignment::new(SerialPort::Serial1, service));
        self
    }

    /// Assigns the second serial connector.
    pub const fn serial2(mut self, service: SerialService) -> Self {
        self.serial[1] = Some(SerialAssignment::new(SerialPort::Serial2, service));
        self
    }

    /// Assigns the first SPI connector.
    pub const fn spi1(mut self, service: SpiService) -> Self {
        self.spi[0] = Some(SpiAssignment::new(SpiPort::Spi1, service));
        self
    }

    /// Returns the logical port carrying a serial service.
    pub fn serial_port(self, service: SerialService) -> Option<SerialPort> {
        self.serial
            .iter()
            .flatten()
            .find(|assignment| assignment.service == service)
            .map(|assignment| assignment.port)
    }

    /// Returns the logical port carrying a SPI service.
    pub fn spi_port(self, service: SpiService) -> Option<SpiPort> {
        self.spi
            .iter()
            .flatten()
            .find(|assignment| assignment.service == service)
            .map(|assignment| assignment.port)
    }
}

/// Empty configuration used by compositions without configurable endpoints.
pub const fn empty_platform_config() -> PlatformConfig {
    PlatformConfig::empty()
}

/// Validates that each runtime service is assigned to at most one connector.
///
/// Physical routes are deliberately absent here. Endpoint declarations own
/// those facts; this configuration only wires portable services at boot.
pub fn validate(config: PlatformConfig) -> Result<(), String> {
    let mut serial_ports = BTreeSet::new();
    let mut serial_services = BTreeSet::new();
    for assignment in config.serial.into_iter().flatten() {
        if !serial_ports.insert(assignment.port) {
            return Err(format!(
                "platform config assigns serial port `{:?}` more than once",
                assignment.port
            ));
        }
        if !serial_services.insert(assignment.service) {
            return Err(format!(
                "platform config assigns serial service `{:?}` more than once",
                assignment.service
            ));
        }
    }

    let mut spi_ports = BTreeSet::new();
    let mut spi_services = BTreeSet::new();
    for assignment in config.spi.into_iter().flatten() {
        if !spi_ports.insert(assignment.port) {
            return Err(format!(
                "platform config assigns SPI port `{:?}` more than once",
                assignment.port
            ));
        }
        if !spi_services.insert(assignment.service) {
            return Err(format!(
                "platform config assigns SPI service `{:?}` more than once",
                assignment.service
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_catalog::foxeer_f405_v2::platform_config::platform_config;

    #[test]
    fn foxeer_boot_assignments_are_valid() {
        validate(platform_config()).unwrap();
    }

    #[test]
    fn duplicate_logical_serial_port_is_rejected() {
        let config = PlatformConfig::new(
            [
                Some(SerialAssignment::new(
                    SerialPort::Serial1,
                    SerialService::RcSbus,
                )),
                Some(SerialAssignment::new(
                    SerialPort::Serial1,
                    SerialService::MspV1Osd,
                )),
            ],
            [None],
        );

        let error = validate(config).unwrap_err();
        assert!(error.contains("assigns serial port `Serial1` more than once"));
    }

    #[test]
    fn duplicate_serial_service_is_rejected() {
        let config = PlatformConfig::new(
            [
                Some(SerialAssignment::new(
                    SerialPort::Serial1,
                    SerialService::MspV1Osd,
                )),
                Some(SerialAssignment::new(
                    SerialPort::Serial2,
                    SerialService::MspV1Osd,
                )),
            ],
            [None],
        );

        let error = validate(config).unwrap_err();
        assert!(error.contains("assigns serial service `MspV1Osd` more than once"));
    }
}
