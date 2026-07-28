#[cfg(test)]
use ferrowasp_io_core::serial::SerialRouteError;
use ferrowasp_io_core::serial::{
    LogicalSerialPort, SerialCapabilities, SerialProfile, SerialRoute,
};

pub const USART2_SBUS: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart2,
    peripheral: "USART2",
    tx_pin: "PA2 AF7",
    rx_pin: "PA3 AF7",
    profile: SerialProfile::sbus(),
    rx_dma: "DMA1 Stream 5 Channel 4",
    tx_dma: None,
    capabilities: SerialCapabilities {
        sbus: true,
        crsf: false,
        mavlink: false,
        msp: false,
        esc_telemetry: false,
        tx: false,
    },
};

pub const UART4_MSP: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart4,
    peripheral: "UART4",
    tx_pin: "PA0 AF8",
    rx_pin: "PA1 AF8",
    profile: SerialProfile::msp(),
    rx_dma: "DMA1 Stream 2 Channel 4",
    tx_dma: Some("DMA1 Stream 4 Channel 4"),
    capabilities: SerialCapabilities {
        sbus: false,
        crsf: false,
        mavlink: false,
        msp: true,
        esc_telemetry: false,
        tx: true,
    },
};

pub const USART1_ESC_TELEMETRY: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart1,
    peripheral: "USART1",
    tx_pin: "unused",
    rx_pin: "PA10 AF7",
    profile: SerialProfile::esc_telemetry(),
    rx_dma: "DMA2 Stream 5 Channel 4",
    tx_dma: None,
    capabilities: SerialCapabilities {
        sbus: false,
        crsf: false,
        mavlink: false,
        msp: false,
        esc_telemetry: true,
        tx: false,
    },
};

pub const ACTIVE_SERIAL_ROUTES: &[SerialRoute] = &[USART2_SBUS, UART4_MSP];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_profiles_match_the_supported_ferrowasp_protocols() {
        assert_eq!(USART2_SBUS.validate_profile(SerialProfile::sbus()), Ok(()));
        assert_eq!(UART4_MSP.validate_profile(SerialProfile::msp()), Ok(()));
        assert_eq!(
            USART1_ESC_TELEMETRY.validate_profile(SerialProfile::esc_telemetry()),
            Ok(())
        );
        assert_eq!(
            USART2_SBUS.validate_profile(SerialProfile::msp()),
            Err(SerialRouteError::UnsupportedProtocol)
        );
    }
}
