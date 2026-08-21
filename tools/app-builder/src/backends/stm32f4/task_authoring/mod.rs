//! Host-only stand-ins used to type-check target task bodies.

/// Physical DShot and legacy-telemetry authoring types.
pub mod dshot;
/// ADC, SPI NOR, USB, and watchdog authoring types.
pub mod golden_services;
/// SPI IMU endpoint authoring aliases.
pub mod spi_imu;
