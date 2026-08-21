//! Boot-loaded service assignments for the selected Foxeer platform.

use crate::rtic::platform_config::{ImuInstallationId, PlatformConfig, SerialService, SpiService};

/// Returns the service routing selected when the generated application boots.
///
/// This is a function, rather than an application-composition constant, so a
/// later persistent configuration provider can replace the defaults without
/// changing the RTIC task graph.
pub fn platform_config() -> PlatformConfig {
    PlatformConfig::empty()
        .serial1(SerialService::RcSbus)
        .serial2(SerialService::MspV1Osd)
        .spi1(SpiService::Imu(ImuInstallationId::new(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_routes_match_the_foxeer_vertical_slice() {
        let config = platform_config();
        assert_eq!(
            config.serial_port(SerialService::RcSbus),
            Some(crate::rtic::platform_config::SerialPort::Serial1)
        );
        assert_eq!(
            config.serial_port(SerialService::MspV1Osd),
            Some(crate::rtic::platform_config::SerialPort::Serial2)
        );
        assert_eq!(
            config.spi_port(SpiService::Imu(ImuInstallationId::new(1))),
            Some(crate::rtic::platform_config::SpiPort::Spi1)
        );
    }
}
