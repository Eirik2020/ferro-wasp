//! STM32F4 hardware endpoints that expand physical resources into task graphs.

/// DMA-backed SPI inertial-sensor endpoint.
pub mod imu_endpoint;

/// Full-duplex DMA-backed serial hardware endpoints.
pub mod serial_endpoint;

pub use imu_endpoint::ImuEndpointDeclaration;
pub use serial_endpoint::SerialEndpointDeclaration;
