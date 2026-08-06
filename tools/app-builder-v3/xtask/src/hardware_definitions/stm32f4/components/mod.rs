//! STM32F4 component declarations that expand hardware into task graphs.

/// Full-duplex DMA-backed serial endpoint components.
pub mod serial_endpoint;

pub use serial_endpoint::SerialEndpointDeclaration;
