//! Serial routes. The logical ports keep the Foxeer F405 V2 numbering, so
//! stored configuration and the app's `uart1`/`uart2`/`uart4` resources mean
//! the same function on both boards: logical UART1 is ESC telemetry on UART8,
//! UART2 is SBUS on USART6, and UART4 is MSP DisplayPort on USART3.

#[cfg(test)]
use ferrowasp_io_core::serial::SerialRouteError;
use ferrowasp_io_core::serial::{
    LogicalSerialPort, SerialCapabilities, SerialProfile, SerialRoute,
};

pub const USART6_SBUS: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart2,
    peripheral: "USART6",
    tx_pin: "PC6 AF7",
    rx_pin: "PC7 AF7",
    profile: SerialProfile::sbus(),
    rx_dma: "DMA1 Stream 0 DMAMUX request 71",
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

pub const USART3_MSP: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart4,
    peripheral: "USART3",
    tx_pin: "PD8 AF7",
    rx_pin: "PD9 AF7",
    profile: SerialProfile::msp(),
    rx_dma: "DMA1 Stream 1 DMAMUX request 45",
    tx_dma: Some("DMA1 Stream 3 DMAMUX request 46"),
    capabilities: SerialCapabilities {
        sbus: false,
        crsf: false,
        mavlink: false,
        msp: true,
        esc_telemetry: false,
        tx: true,
    },
};

pub const UART8_ESC_TELEMETRY: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart1,
    peripheral: "UART8",
    tx_pin: "unused",
    rx_pin: "PE0 AF8",
    profile: SerialProfile::esc_telemetry(),
    rx_dma: "DMA1 Stream 2 DMAMUX request 81",
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

pub const ACTIVE_SERIAL_ROUTES: &[SerialRoute] = &[USART6_SBUS, USART3_MSP];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_profiles_match_the_supported_ferrowasp_protocols() {
        assert_eq!(USART6_SBUS.validate_profile(SerialProfile::sbus()), Ok(()));
        assert_eq!(USART3_MSP.validate_profile(SerialProfile::msp()), Ok(()));
        assert_eq!(
            UART8_ESC_TELEMETRY.validate_profile(SerialProfile::esc_telemetry()),
            Ok(())
        );
        assert_eq!(
            USART6_SBUS.validate_profile(SerialProfile::msp()),
            Err(SerialRouteError::UnsupportedProtocol)
        );
    }
}
