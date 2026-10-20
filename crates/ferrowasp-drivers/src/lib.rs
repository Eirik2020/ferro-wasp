//! The drivers' former home, now re-exporting the crates they moved to
//! (ADR-0060), so existing paths keep working until this crate dissolves.

#![no_std]
#![forbid(unsafe_code)]

pub use ferrowasp_blheli_telemetry as blheli_telemetry;
pub use ferrowasp_crsf as crsf;
pub use ferrowasp_icm42688p as icm42688p;
pub use ferrowasp_mpu6000 as mpu6000;
pub use ferrowasp_mpu6500 as mpu6500;
pub use ferrowasp_storage::{nor_storage, spi_nor};
