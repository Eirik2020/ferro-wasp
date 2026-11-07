#[cfg(test)]
use ferrowasp_serial::SerialRouteError;
use ferrowasp_serial::{
    LogicalSerialPort, SerialBindings, SerialCapabilities, SerialFunction, SerialProfile,
    SerialRoute,
};

pub const USART1_CONFIGURATOR: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart1,
    peripheral: "USART1",
    tx_pin: "PA9 AF7",
    rx_pin: "PA10 AF7",
    profile: SerialProfile::cli(),
    rx_dma: "DMA2 Stream 5 Channel 4",
    tx_dma: Some("DMA2 Stream 7 Channel 4"),
    // The T1/R1 pads, plain 8N1 both ways with no inverter, so no SBUS. The
    // configurator line protocol rides here: a Bluetooth serial module or a
    // wire, for log sync and configuration.
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

pub const USART6_SBUS: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart6,
    peripheral: "USART6",
    tx_pin: "unused",
    rx_pin: "PC7 AF8",
    profile: SerialProfile::sbus(),
    rx_dma: "DMA2 Stream 2 Channel 5",
    tx_dma: None,
    // The DJI O4 connector's receive line, straight into PC7 with no inverter.
    // A receiver that emits SBUS non-inverted (ExpressLRS and companions can)
    // binds here; an O4 unit's CRSF needs the return line this route does not
    // wire, so CRSF stays off until it does.
    capabilities: SerialCapabilities {
        sbus: true,
        crsf: false,
        mavlink: false,
        msp: false,
        esc_telemetry: false,
        cli: false,
        tx: false,
    },
};

pub const UART5_ESC_TELEMETRY: SerialRoute = SerialRoute {
    logical: LogicalSerialPort::Uart5,
    peripheral: "UART5",
    tx_pin: "unused",
    rx_pin: "PD2 AF8",
    profile: SerialProfile::esc_telemetry(),
    rx_dma: "DMA1 Stream 0 Channel 4",
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

/// Every UART the bring-up routes; config may bind each to any function its
/// capabilities accept. The board's other pads (UART2, UART3, UART4) stay
/// unrouted until a driver or a need claims them.
pub const SERIAL_ROUTES: &[SerialRoute] = &[USART1_CONFIGURATOR, USART6_SBUS, UART5_ESC_TELEMETRY];

/// The wiring the board shipped with, used until a pilot saves bindings.
pub const DEFAULT_SERIAL_BINDINGS: SerialBindings = SerialBindings::none()
    .with(LogicalSerialPort::Uart1, SerialFunction::Configurator)
    .with(LogicalSerialPort::Uart5, SerialFunction::EscTelemetry)
    .with(LogicalSerialPort::Uart6, SerialFunction::RcInput);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_resolve_cleanly() {
        use ferrowasp_serial::{RcProtocol, resolve_bindings};

        let defaults = resolve_bindings(DEFAULT_SERIAL_BINDINGS, SERIAL_ROUTES, RcProtocol::Sbus);
        assert_eq!(defaults.issues(), &[]);
        assert_eq!(
            defaults.profile(LogicalSerialPort::Uart6),
            Some(SerialProfile::sbus())
        );
        assert_eq!(
            defaults.profile(LogicalSerialPort::Uart1),
            Some(SerialProfile::cli())
        );
        assert_eq!(
            defaults.profile(LogicalSerialPort::Uart5),
            Some(SerialProfile::esc_telemetry())
        );
    }

    #[test]
    fn crsf_rc_needs_a_port_that_transmits_and_none_does_yet() {
        use ferrowasp_serial::{RcProtocol, resolve_bindings};

        // UART6 receives SBUS but transmits nothing, so CRSF RC is refused.
        let refused = resolve_bindings(DEFAULT_SERIAL_BINDINGS, SERIAL_ROUTES, RcProtocol::Crsf);
        assert_eq!(
            refused.function(LogicalSerialPort::Uart6),
            SerialFunction::None
        );
    }

    #[test]
    fn rc_on_a_port_without_an_inverter_is_refused() {
        use ferrowasp_serial::{RcProtocol, resolve_bindings};

        // SBUS is only accepted on the O4 connector's receive line; moving RC
        // to the configurator port is refused, so it stays unbound and the
        // resolver reports the one issue.
        let refused = resolve_bindings(
            DEFAULT_SERIAL_BINDINGS
                .with(LogicalSerialPort::Uart6, SerialFunction::None)
                .with(LogicalSerialPort::Uart1, SerialFunction::RcInput),
            SERIAL_ROUTES,
            RcProtocol::Sbus,
        );
        assert_eq!(refused.issues().len(), 1);
        assert_eq!(
            refused.function(LogicalSerialPort::Uart1),
            SerialFunction::None
        );
    }

    #[test]
    fn active_profiles_match_the_supported_ferrowasp_protocols() {
        assert_eq!(USART6_SBUS.validate_profile(SerialProfile::sbus()), Ok(()));
        assert_eq!(
            USART1_CONFIGURATOR.validate_profile(SerialProfile::cli()),
            Ok(())
        );
        assert_eq!(
            UART5_ESC_TELEMETRY.validate_profile(SerialProfile::esc_telemetry()),
            Ok(())
        );
        assert_eq!(
            USART6_SBUS.validate_profile(SerialProfile::msp()),
            Err(SerialRouteError::UnsupportedProtocol)
        );
    }
}
