// GENERATED FILE — DO NOT EDIT DIRECTLY
// Lowered from xtask/src/target/platform_config.rs.

//! Boot-time service assignments for the generated firmware.

use ferrowasp_io_core::platform_config::{
    ImuInstallationId, RuntimePlatformConfig, SerialService, SpiService,
};

pub(crate) fn load_platform_config() -> RuntimePlatformConfig {
    RuntimePlatformConfig {
        serial1: Some(SerialService::RcSbus),
        serial2: Some(SerialService::MspV1Osd),
        spi1: Some(SpiService::Imu(ImuInstallationId::new(1))),
    }
}
