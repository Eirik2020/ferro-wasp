//! Serial routes. Each logical port is the UART's number on the chip, the
//! name a pilot sees on the board and types in `serial <port> <function>`:
//! USART3 is `uart3`, USART6 `uart6` and UART8 `uart8`.
//!
//! Every STM32H743 UART inverts its receive line in hardware, so any port can
//! take SBUS without an external inverter, and any port can take receive-only
//! ESC telemetry. MSP DisplayPort and CRSF talk back, so they need a transmit
//! DMA stream, which UART3 and UART6 have.

#[cfg(test)]
use ferrowasp_io_core::serial::SerialRouteError;
use ferrowasp_io_core::serial::{
    LogicalSerialPort, SerialBindings, SerialCapabilities, SerialFunction, SerialProfile,
    SerialRoute,
};

pub const USART6_SBUS: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart6,
    peripheral: "USART6",
    tx_pin: "PC6 AF7",
    rx_pin: "PC7 AF7",
    profile: SerialProfile::sbus(),
    rx_dma: "DMA1 Stream 0 DMAMUX request 71",
    tx_dma: Some("DMA1 Stream 6 DMAMUX request 72"),
    capabilities: SerialCapabilities {
        sbus: true,
        crsf: true,
        mavlink: false,
        msp: true,
        esc_telemetry: true,
        cli: false,
        tx: true,
    },
};

pub const USART3_MSP: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart3,
    peripheral: "USART3",
    tx_pin: "PD8 AF7",
    rx_pin: "PD9 AF7",
    profile: SerialProfile::msp(),
    rx_dma: "DMA1 Stream 1 DMAMUX request 45",
    tx_dma: Some("DMA1 Stream 3 DMAMUX request 46"),
    capabilities: SerialCapabilities {
        sbus: true,
        crsf: true,
        mavlink: false,
        msp: true,
        esc_telemetry: true,
        cli: false,
        tx: true,
    },
};

pub const UART8_ESC_TELEMETRY: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart8,
    peripheral: "UART8",
    tx_pin: "unused",
    rx_pin: "PE0 AF8",
    profile: SerialProfile::esc_telemetry(),
    rx_dma: "DMA1 Stream 2 DMAMUX request 81",
    tx_dma: None,
    capabilities: SerialCapabilities {
        sbus: true,
        crsf: false,
        mavlink: false,
        msp: false,
        esc_telemetry: true,
        cli: false,
        tx: false,
    },
};

/// Every UART the board routes; config may bind each to any function its
/// capabilities accept.
pub const SERIAL_ROUTES: &[SerialRoute] = &[USART3_MSP, USART6_SBUS, UART8_ESC_TELEMETRY];

/// The wiring the board shipped with, used until a pilot saves bindings.
pub const DEFAULT_SERIAL_BINDINGS: SerialBindings = SerialBindings::none()
    .with(LogicalSerialPort::Uart3, SerialFunction::MspDisplayPort)
    .with(LogicalSerialPort::Uart6, SerialFunction::RcInput)
    .with(LogicalSerialPort::Uart8, SerialFunction::EscTelemetry);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_follow_each_ports_capabilities() {
        use ferrowasp_io_core::serial::{RcProtocol, resolve_bindings};

        let defaults = resolve_bindings(DEFAULT_SERIAL_BINDINGS, SERIAL_ROUTES, RcProtocol::Sbus);
        assert_eq!(defaults.issues(), &[]);
        assert_eq!(
            defaults.profile(LogicalSerialPort::Uart6),
            Some(SerialProfile::sbus())
        );
        // A port the board does not route cannot be bound.
        let unrouted = resolve_bindings(
            DEFAULT_SERIAL_BINDINGS
                .with(LogicalSerialPort::Uart6, SerialFunction::None)
                .with(LogicalSerialPort::Uart1, SerialFunction::RcInput),
            SERIAL_ROUTES,
            RcProtocol::Sbus,
        );
        assert_eq!(unrouted.issues().len(), 1);
        assert_eq!(
            unrouted.function(LogicalSerialPort::Uart1),
            SerialFunction::None
        );
        // Every port inverts in hardware, so RC moves to UART3 or UART8.
        for port in [LogicalSerialPort::Uart3, LogicalSerialPort::Uart8] {
            let moved = resolve_bindings(
                SerialBindings::none().with(port, SerialFunction::RcInput),
                SERIAL_ROUTES,
                RcProtocol::Sbus,
            );
            assert_eq!(moved.issues(), &[]);
            assert_eq!(moved.profile(port), Some(SerialProfile::sbus()));
        }
        // MSP needs transmit DMA, which UART8 does not have, so it is refused.
        let refused = resolve_bindings(
            SerialBindings::none().with(LogicalSerialPort::Uart8, SerialFunction::MspDisplayPort),
            SERIAL_ROUTES,
            RcProtocol::Sbus,
        );
        assert_eq!(refused.issues().len(), 1);
        assert_eq!(
            refused.function(LogicalSerialPort::Uart8),
            SerialFunction::None
        );
        // CRSF RC on UART6 starts at CRSF's line settings; UART8 cannot
        // transmit, so CRSF RC there is refused.
        let crsf = resolve_bindings(DEFAULT_SERIAL_BINDINGS, SERIAL_ROUTES, RcProtocol::Crsf);
        assert_eq!(crsf.issues(), &[]);
        assert_eq!(
            crsf.profile(LogicalSerialPort::Uart6),
            Some(SerialProfile::crsf())
        );
        let no_tx = resolve_bindings(
            SerialBindings::none().with(LogicalSerialPort::Uart8, SerialFunction::RcInput),
            SERIAL_ROUTES,
            RcProtocol::Crsf,
        );
        assert_eq!(
            no_tx.function(LogicalSerialPort::Uart8),
            SerialFunction::None
        );
    }

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
