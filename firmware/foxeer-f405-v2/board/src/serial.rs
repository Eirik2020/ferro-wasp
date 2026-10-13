#[cfg(test)]
use ferrowasp_io_core::serial::SerialRouteError;
use ferrowasp_io_core::serial::{
    LogicalSerialPort, SerialBindings, SerialCapabilities, SerialFunction, SerialProfile,
    SerialRoute,
};

pub const USART2_SBUS: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart2,
    peripheral: "USART2",
    tx_pin: "PA2 AF7",
    rx_pin: "PA3 AF7",
    profile: SerialProfile::sbus(),
    rx_dma: "DMA1 Stream 5 Channel 4",
    tx_dma: Some("DMA1 Stream 6 Channel 4"),
    // SBUS reaches PA3 from the board's SBUS pad through its inverter; CRSF
    // uses the plain R2/T2 pads, straight to PA3/PA2, both ways.
    capabilities: SerialCapabilities {
        sbus: true,
        crsf: true,
        mavlink: false,
        msp: false,
        esc_telemetry: false,
        cli: true,
        tx: true,
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
    // A plain 8N1 port with DMA both ways; it has no inverter, so no SBUS.
    capabilities: SerialCapabilities {
        sbus: false,
        crsf: true,
        mavlink: false,
        msp: true,
        esc_telemetry: true,
        cli: true,
        tx: true,
    },
};

pub const USART3_CONFIGURATOR: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart3,
    peripheral: "USART3",
    tx_pin: "PC10 AF7",
    rx_pin: "PC11 AF7",
    profile: SerialProfile::cli(),
    rx_dma: "DMA1 Stream 1 Channel 4",
    tx_dma: Some("DMA1 Stream 3 Channel 4"),
    // The R3/T3 pads, plain 8N1 both ways with no inverter, so no SBUS. A
    // Bluetooth serial module here carries the configurator for log sync.
    capabilities: SerialCapabilities {
        sbus: false,
        crsf: true,
        mavlink: false,
        msp: true,
        esc_telemetry: true,
        cli: true,
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
        cli: false,
        tx: false,
    },
};

/// Every UART the board routes; config may bind each to any function its
/// capabilities accept.
pub const SERIAL_ROUTES: &[SerialRoute] = &[
    USART1_ESC_TELEMETRY,
    USART2_SBUS,
    USART3_CONFIGURATOR,
    UART4_MSP,
];

/// The wiring the board shipped with, used until a pilot saves bindings.
pub const DEFAULT_SERIAL_BINDINGS: SerialBindings = SerialBindings::none()
    .with(LogicalSerialPort::Uart1, SerialFunction::EscTelemetry)
    .with(LogicalSerialPort::Uart2, SerialFunction::RcInput)
    .with(LogicalSerialPort::Uart3, SerialFunction::Configurator)
    .with(LogicalSerialPort::Uart4, SerialFunction::MspDisplayPort);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_resolve_cleanly_and_uart4_can_take_esc_telemetry() {
        use ferrowasp_io_core::serial::{RcProtocol, resolve_bindings};

        let defaults = resolve_bindings(DEFAULT_SERIAL_BINDINGS, SERIAL_ROUTES, RcProtocol::Sbus);
        assert_eq!(defaults.issues(), &[]);
        let moved = resolve_bindings(
            DEFAULT_SERIAL_BINDINGS
                .with(LogicalSerialPort::Uart1, SerialFunction::None)
                .with(LogicalSerialPort::Uart4, SerialFunction::EscTelemetry),
            SERIAL_ROUTES,
            RcProtocol::Sbus,
        );
        assert_eq!(moved.issues(), &[]);
        assert_eq!(
            moved.profile(LogicalSerialPort::Uart4),
            Some(SerialProfile::esc_telemetry())
        );
        // RC on a port without an inverter is refused, so RC stays unbound.
        let refused = resolve_bindings(
            DEFAULT_SERIAL_BINDINGS
                .with(LogicalSerialPort::Uart2, SerialFunction::None)
                .with(LogicalSerialPort::Uart4, SerialFunction::RcInput),
            SERIAL_ROUTES,
            RcProtocol::Sbus,
        );
        assert_eq!(refused.issues().len(), 1);
        assert_eq!(
            refused.function(LogicalSerialPort::Uart4),
            SerialFunction::None
        );
    }

    #[test]
    fn crsf_rc_input_needs_a_port_that_transmits() {
        use ferrowasp_io_core::serial::{RcProtocol, resolve_bindings};

        let crsf = resolve_bindings(DEFAULT_SERIAL_BINDINGS, SERIAL_ROUTES, RcProtocol::Crsf);
        assert_eq!(crsf.issues(), &[]);
        assert_eq!(
            crsf.profile(LogicalSerialPort::Uart2),
            Some(SerialProfile::crsf())
        );
        // UART1 is receive-only, so CRSF RC there is refused.
        let refused = resolve_bindings(
            DEFAULT_SERIAL_BINDINGS
                .with(LogicalSerialPort::Uart2, SerialFunction::None)
                .with(LogicalSerialPort::Uart1, SerialFunction::RcInput),
            SERIAL_ROUTES,
            RcProtocol::Crsf,
        );
        assert_eq!(
            refused.function(LogicalSerialPort::Uart1),
            SerialFunction::None
        );
    }

    #[test]
    fn the_configurator_defaults_to_uart3_and_needs_a_port_that_transmits() {
        use ferrowasp_io_core::serial::{RcProtocol, resolve_bindings};

        let defaults = resolve_bindings(DEFAULT_SERIAL_BINDINGS, SERIAL_ROUTES, RcProtocol::Sbus);
        assert_eq!(
            defaults.profile(LogicalSerialPort::Uart3),
            Some(SerialProfile::cli())
        );
        let receive_only = resolve_bindings(
            DEFAULT_SERIAL_BINDINGS
                .with(LogicalSerialPort::Uart3, SerialFunction::None)
                .with(LogicalSerialPort::Uart1, SerialFunction::Configurator),
            SERIAL_ROUTES,
            RcProtocol::Sbus,
        );
        assert_eq!(
            receive_only.function(LogicalSerialPort::Uart1),
            SerialFunction::None
        );
    }

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
