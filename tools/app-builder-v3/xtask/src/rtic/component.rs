//! Application-level declarations for reusable hardware components.

use crate::hardware_definitions::stm32f4::{
    board_declaration::BoardDeclaration,
    components::serial_endpoint::{self, SerialEndpointDeclaration},
};

/// One reusable component instance selected by application composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentDeclaration {
    /// DMA-backed STM32F4 serial endpoint.
    SerialEndpoint(SerialEndpointDeclaration),
}

impl ComponentDeclaration {
    /// Wraps a serial endpoint for inclusion in an application composition.
    pub const fn serial_endpoint(endpoint: SerialEndpointDeclaration) -> Self {
        Self::SerialEndpoint(endpoint)
    }

    /// Returns the application-local component identifier.
    pub const fn id(self) -> &'static str {
        match self {
            Self::SerialEndpoint(endpoint) => endpoint.id,
        }
    }

    pub(crate) fn hardware_id(self) -> &'static str {
        match self {
            Self::SerialEndpoint(endpoint) => endpoint.hardware_id,
        }
    }

    pub(crate) fn validate(self, board: &BoardDeclaration) -> Result<(), String> {
        match self {
            Self::SerialEndpoint(endpoint) => serial_endpoint::validate(endpoint, board),
        }
    }
}
