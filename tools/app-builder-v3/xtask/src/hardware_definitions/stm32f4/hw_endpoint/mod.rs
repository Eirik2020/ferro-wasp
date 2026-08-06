//! STM32F4 hardware endpoints that expand physical resources into task graphs.

/// Full-duplex DMA-backed serial hardware endpoints.
pub mod serial_endpoint;

pub use serial_endpoint::SerialEndpointDeclaration;
