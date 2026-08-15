//! Application-level declarations for reusable hardware components.

use crate::hardware_definitions::stm32f4::{
    board_declaration::BoardDeclaration,
    dshot_actuator::{self, DshotActuatorDeclaration},
    golden_services::{self, GoldenServicesDeclaration},
    hw_endpoint::imu_endpoint::{self, ImuEndpointDeclaration},
    hw_endpoint::serial_endpoint::{self, SerialEndpointDeclaration},
    periodic_control::{self, PeriodicControlDeclaration},
};

/// One reusable component instance selected by application composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentDeclaration {
    /// Four-lane Foxeer DShot bank with receive-only legacy ESC telemetry.
    DshotActuator(DshotActuatorDeclaration),

    /// Mandatory bounded Foxeer observation, persistence, and diagnostic services.
    GoldenServices(GoldenServicesDeclaration),

    /// DMA-backed STM32F4 SPI IMU endpoint.
    ImuEndpoint(ImuEndpointDeclaration),

    /// Timer-backed synchronous periodic control scheduler.
    PeriodicControl(PeriodicControlDeclaration),

    /// DMA-backed STM32F4 serial endpoint.
    SerialEndpoint(SerialEndpointDeclaration),
}

impl ComponentDeclaration {
    /// Wraps a reviewed physical DShot actuator component.
    pub const fn dshot_actuator(actuator: DshotActuatorDeclaration) -> Self {
        Self::DshotActuator(actuator)
    }

    /// Wraps the mandatory Foxeer service suite.
    pub const fn golden_services(services: GoldenServicesDeclaration) -> Self {
        Self::GoldenServices(services)
    }

    /// Wraps an SPI IMU endpoint for inclusion in an application composition.
    pub const fn imu_endpoint(endpoint: ImuEndpointDeclaration) -> Self {
        Self::ImuEndpoint(endpoint)
    }

    /// Wraps a hardware-neutral SPI endpoint for application composition.
    pub const fn spi_endpoint(endpoint: ImuEndpointDeclaration) -> Self {
        Self::ImuEndpoint(endpoint)
    }

    /// Wraps a periodic control timer for application composition.
    pub const fn periodic_control(timer: PeriodicControlDeclaration) -> Self {
        Self::PeriodicControl(timer)
    }

    /// Wraps a serial endpoint for inclusion in an application composition.
    pub const fn serial_endpoint(endpoint: SerialEndpointDeclaration) -> Self {
        Self::SerialEndpoint(endpoint)
    }

    /// Returns the application-local component identifier.
    pub const fn id(self) -> &'static str {
        match self {
            Self::DshotActuator(actuator) => actuator.id,
            Self::GoldenServices(services) => services.id,
            Self::ImuEndpoint(endpoint) => endpoint.id,
            Self::PeriodicControl(timer) => timer.id,
            Self::SerialEndpoint(endpoint) => endpoint.id,
        }
    }

    pub(crate) const fn hardware_id(self) -> &'static str {
        match self {
            Self::DshotActuator(actuator) => actuator.hardware_id,
            Self::GoldenServices(services) => services.adc_hardware_id,
            Self::ImuEndpoint(endpoint) => endpoint.hardware_id,
            Self::PeriodicControl(timer) => timer.hardware_id,
            Self::SerialEndpoint(endpoint) => endpoint.hardware_id,
        }
    }

    pub(crate) fn validate(self, board: &BoardDeclaration) -> Result<(), String> {
        match self {
            Self::DshotActuator(actuator) => dshot_actuator::validate(actuator, board),
            Self::GoldenServices(services) => golden_services::validate(services, board),
            Self::ImuEndpoint(endpoint) => imu_endpoint::validate(endpoint, board),
            Self::PeriodicControl(timer) => periodic_control::validate(timer, board),
            Self::SerialEndpoint(endpoint) => serial_endpoint::validate(endpoint, board),
        }
    }
}
