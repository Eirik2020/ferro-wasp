//! Hardware-neutral service assignments loaded during platform initialization.

/// Service selected for one serial connector.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SerialService {
    /// Receive SBUS frames and publish radio-control input.
    RcSbus,
    /// Provide an MSP V1 DisplayPort transport for the OSD.
    MspV1Osd,
}

impl SerialService {
    /// UART electrical/protocol profile required by the service.
    pub const fn profile(self) -> crate::serial::SerialProfile {
        match self {
            Self::RcSbus => crate::serial::SerialProfile::sbus(),
            Self::MspV1Osd => crate::serial::SerialProfile::msp(),
        }
    }

    /// Whether the service requires a transmit route.
    pub const fn requires_tx(self) -> bool {
        matches!(self, Self::MspV1Osd)
    }

    /// Stable RTIC local-resource name for this service's receive side.
    pub const fn reader_resource(self) -> &'static str {
        match self {
            Self::RcSbus => "rc_sbus_reader",
            Self::MspV1Osd => "msp_v1_osd_reader",
        }
    }

    /// Stable RTIC local-resource name for receive continuity observations.
    pub const fn discontinuities_resource(self) -> &'static str {
        match self {
            Self::RcSbus => "rc_sbus_rx_discontinuities",
            Self::MspV1Osd => "msp_v1_osd_rx_discontinuities",
        }
    }

    /// Stable RTIC local-resource name for this service's transmit side.
    pub const fn writer_resource(self) -> Option<&'static str> {
        match self {
            Self::RcSbus => None,
            Self::MspV1Osd => Some("msp_v1_osd_writer"),
        }
    }
}

/// Stable board-local identifier for one installed IMU.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ImuInstallationId(u8);

impl ImuInstallationId {
    /// Creates an IMU installation identifier.
    pub const fn new(id: u8) -> Self {
        Self(id)
    }

    /// Returns the numeric board-local identifier.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Service selected for one SPI connector.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SpiService {
    /// Inertial-sensor service using the selected physical installation.
    Imu(ImuInstallationId),
}

impl SpiService {
    /// Stable RTIC shared-resource name published by this service.
    pub const fn sample_resource(self) -> &'static str {
        match self {
            Self::Imu(_) => "imu_sample",
        }
    }
}

/// Runtime service selection consumed by platform initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimePlatformConfig {
    /// Service assigned to the first serial connector.
    pub serial1: Option<SerialService>,
    /// Service assigned to the second serial connector.
    pub serial2: Option<SerialService>,
    /// Service assigned to the first SPI connector.
    pub spi1: Option<SpiService>,
}

impl RuntimePlatformConfig {
    /// Creates a configuration with every connector disabled.
    pub const fn empty() -> Self {
        Self {
            serial1: None,
            serial2: None,
            spi1: None,
        }
    }
}

impl Default for RuntimePlatformConfig {
    fn default() -> Self {
        Self::empty()
    }
}
