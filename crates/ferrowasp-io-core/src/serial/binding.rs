//! Which firmware function each serial port serves, chosen at boot.
//!
//! The board fixes what a port is: pins, DMA streams, interrupt and what the
//! wiring can carry ([`SerialRoute`]). Saved configuration says what each
//! port is for ([`SerialBindings`]). Init resolves the two once, before any
//! port starts, so a binding only changes across a reboot. A binding the
//! port cannot carry is dropped and reported; the rest still apply. A
//! function claimed by two ports is dropped from both, so RC input is either
//! on exactly one port or on none, and with none the RC link never becomes
//! valid and the craft cannot arm.

use super::{LogicalSerialPort, SerialProfile, SerialRoute, SerialRouteError};

/// One slot per UART the logical port numbering can name.
pub const SERIAL_PORT_SLOTS: usize = LogicalSerialPort::ALL.len();

/// Which receiver protocol RC input speaks. It decides the line settings of
/// whichever port is bound to RC input, so it applies at boot like a binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RcProtocol {
    Sbus = 0,
    Crsf = 1,
}

impl RcProtocol {
    pub const fn profile(self) -> SerialProfile {
        match self {
            Self::Sbus => SerialProfile::sbus(),
            Self::Crsf => SerialProfile::crsf(),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Sbus => "sbus",
            Self::Crsf => "crsf",
        }
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Sbus),
            1 => Some(Self::Crsf),
            _ => None,
        }
    }
}

/// What a port is used for. RC input's protocol is its own setting,
/// [`RcProtocol`]; every other function speaks one protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SerialFunction {
    None = 0,
    RcInput = 1,
    MspDisplayPort = 2,
    EscTelemetry = 3,
}

impl SerialFunction {
    pub const ALL: [Self; 4] = [
        Self::None,
        Self::RcInput,
        Self::MspDisplayPort,
        Self::EscTelemetry,
    ];

    /// The line settings this function needs, RC input in `rc`'s protocol.
    pub const fn profile(self, rc: RcProtocol) -> SerialProfile {
        match self {
            Self::None => SerialProfile::disabled(),
            Self::RcInput => rc.profile(),
            Self::MspDisplayPort => SerialProfile::msp(),
            Self::EscTelemetry => SerialProfile::esc_telemetry(),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::RcInput => "rc",
            Self::MspDisplayPort => "osd",
            Self::EscTelemetry => "esc_telemetry",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|function| function.name() == name)
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::RcInput),
            2 => Some(Self::MspDisplayPort),
            3 => Some(Self::EscTelemetry),
            _ => None,
        }
    }
}

/// The saved port-to-function table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialBindings {
    functions: [SerialFunction; SERIAL_PORT_SLOTS],
}

impl SerialBindings {
    pub const fn none() -> Self {
        Self {
            functions: [SerialFunction::None; SERIAL_PORT_SLOTS],
        }
    }

    pub const fn with(mut self, port: LogicalSerialPort, function: SerialFunction) -> Self {
        self.functions[port.index()] = function;
        self
    }

    pub const fn function(&self, port: LogicalSerialPort) -> SerialFunction {
        self.functions[port.index()]
    }

    pub fn set(&mut self, port: LogicalSerialPort, function: SerialFunction) {
        self.functions[port.index()] = function;
    }

    pub fn encode(&self) -> [u8; SERIAL_PORT_SLOTS] {
        self.functions.map(|function| function as u8)
    }

    /// `None` when any slot holds a function this firmware does not know, so
    /// a table written by newer firmware is not half-applied.
    pub fn decode(bytes: &[u8; SERIAL_PORT_SLOTS]) -> Option<Self> {
        let mut functions = [SerialFunction::None; SERIAL_PORT_SLOTS];
        for (slot, byte) in functions.iter_mut().zip(bytes) {
            *slot = SerialFunction::from_u8(*byte)?;
        }
        Some(Self { functions })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingFault {
    /// The board has no usable route for this port.
    NoSuchPort,
    /// The port's wiring cannot carry the function's protocol.
    Unsupported(SerialRouteError),
    /// Another port claims the same function.
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingIssue {
    pub port: LogicalSerialPort,
    pub function: SerialFunction,
    pub fault: BindingFault,
}

/// The bindings init may apply, and the ones it dropped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedBindings {
    functions: [SerialFunction; SERIAL_PORT_SLOTS],
    rc_protocol: RcProtocol,
    issues: heapless::Vec<BindingIssue, SERIAL_PORT_SLOTS>,
}

impl ResolvedBindings {
    /// The function this port serves, `None` when unbound or dropped.
    pub const fn function(&self, port: LogicalSerialPort) -> SerialFunction {
        self.functions[port.index()]
    }

    /// The line settings to start this port with, `None` to leave it off.
    pub const fn profile(&self, port: LogicalSerialPort) -> Option<SerialProfile> {
        match self.function(port) {
            SerialFunction::None => None,
            function => Some(function.profile(self.rc_protocol)),
        }
    }

    /// The protocol RC input was resolved with.
    pub const fn rc_protocol(&self) -> RcProtocol {
        self.rc_protocol
    }

    pub fn issues(&self) -> &[BindingIssue] {
        &self.issues
    }
}

/// Check every binding against the board's routes, RC input in
/// `rc_protocol`.
pub fn resolve_bindings(
    bindings: SerialBindings,
    routes: &[SerialRoute],
    rc_protocol: RcProtocol,
) -> ResolvedBindings {
    let mut resolved = ResolvedBindings {
        functions: [SerialFunction::None; SERIAL_PORT_SLOTS],
        rc_protocol,
        issues: heapless::Vec::new(),
    };
    let mut report = |port, function, fault| {
        // One issue per port at most, so the vector cannot fill.
        let _ = resolved.issues.push(BindingIssue {
            port,
            function,
            fault,
        });
    };

    for port in LogicalSerialPort::ALL {
        let function = bindings.function(port);
        if function == SerialFunction::None {
            continue;
        }
        let claims = LogicalSerialPort::ALL
            .into_iter()
            .filter(|other| bindings.function(*other) == function)
            .count();
        if claims > 1 {
            report(port, function, BindingFault::Duplicate);
            continue;
        }
        let Some(route) = routes.iter().find(|route| route.logical == port) else {
            report(port, function, BindingFault::NoSuchPort);
            continue;
        };
        if let Err(error) = route.validate_profile(function.profile(rc_protocol)) {
            report(port, function, BindingFault::Unsupported(error));
            continue;
        }
        resolved.functions[port.index()] = function;
    }
    resolved
}

/// One endpoint per function, filled from the resolved bindings at init and
/// moved into the function tasks' local resources.
pub struct SerialFunctionSlots<E> {
    pub rc_input: Option<E>,
    pub msp_display_port: Option<E>,
    pub esc_telemetry: Option<E>,
}

impl<E> SerialFunctionSlots<E> {
    pub const fn empty() -> Self {
        Self {
            rc_input: None,
            msp_display_port: None,
            esc_telemetry: None,
        }
    }

    /// Give `function` its port. Hands the endpoint back if the function is
    /// `None` or already has one, which resolved bindings never produce.
    pub fn place(&mut self, function: SerialFunction, endpoint: E) -> Result<(), E> {
        let slot = match function {
            SerialFunction::None => return Err(endpoint),
            SerialFunction::RcInput => &mut self.rc_input,
            SerialFunction::MspDisplayPort => &mut self.msp_display_port,
            SerialFunction::EscTelemetry => &mut self.esc_telemetry,
        };
        if slot.is_some() {
            return Err(endpoint);
        }
        *slot = Some(endpoint);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial::SerialCapabilities;

    const RX_ONLY: SerialCapabilities = SerialCapabilities {
        sbus: false,
        crsf: false,
        mavlink: false,
        msp: false,
        esc_telemetry: true,
        tx: false,
    };

    const fn route(logical: LogicalSerialPort, capabilities: SerialCapabilities) -> SerialRoute {
        SerialRoute {
            logical,
            peripheral: "test",
            tx_pin: "test",
            rx_pin: "test",
            profile: SerialProfile::disabled(),
            rx_dma: "test",
            tx_dma: None,
            capabilities,
        }
    }

    const ROUTES: [SerialRoute; 3] = [
        route(LogicalSerialPort::Uart1, RX_ONLY),
        route(
            LogicalSerialPort::Uart2,
            SerialCapabilities {
                sbus: true,
                ..RX_ONLY
            },
        ),
        route(
            LogicalSerialPort::Uart4,
            SerialCapabilities {
                msp: true,
                tx: true,
                ..RX_ONLY
            },
        ),
    ];

    #[test]
    fn rc_input_takes_the_configured_protocol_and_crsf_needs_a_transmit_path() {
        let mut both_ways = RX_ONLY;
        both_ways.crsf = true;
        both_ways.tx = true;
        let routes = [
            route(LogicalSerialPort::Uart1, RX_ONLY),
            route(LogicalSerialPort::Uart2, both_ways),
        ];
        let bindings =
            SerialBindings::none().with(LogicalSerialPort::Uart2, SerialFunction::RcInput);

        let crsf = resolve_bindings(bindings, &routes, RcProtocol::Crsf);
        assert_eq!(crsf.issues(), &[]);
        assert_eq!(
            crsf.profile(LogicalSerialPort::Uart2),
            Some(SerialProfile::crsf())
        );

        let mut rx_only_crsf = RX_ONLY;
        rx_only_crsf.crsf = true;
        let refused = resolve_bindings(
            SerialBindings::none().with(LogicalSerialPort::Uart1, SerialFunction::RcInput),
            &[route(LogicalSerialPort::Uart1, rx_only_crsf)],
            RcProtocol::Crsf,
        );
        assert_eq!(
            refused.issues()[0].fault,
            BindingFault::Unsupported(SerialRouteError::MissingTxDma)
        );
        assert_eq!(
            RcProtocol::from_u8(RcProtocol::Crsf as u8),
            Some(RcProtocol::Crsf)
        );
        assert_eq!(RcProtocol::from_u8(2), None);
    }

    #[test]
    fn valid_bindings_resolve_to_their_profiles() {
        let bindings = SerialBindings::none()
            .with(LogicalSerialPort::Uart2, SerialFunction::RcInput)
            .with(LogicalSerialPort::Uart4, SerialFunction::EscTelemetry);
        let resolved = resolve_bindings(bindings, &ROUTES, RcProtocol::Sbus);
        assert_eq!(resolved.issues(), &[]);
        assert_eq!(
            resolved.profile(LogicalSerialPort::Uart2),
            Some(SerialProfile::sbus())
        );
        assert_eq!(
            resolved.profile(LogicalSerialPort::Uart4),
            Some(SerialProfile::esc_telemetry())
        );
        assert_eq!(resolved.profile(LogicalSerialPort::Uart1), None);
    }

    #[test]
    fn a_port_that_cannot_carry_the_protocol_is_dropped() {
        let bindings = SerialBindings::none()
            .with(LogicalSerialPort::Uart1, SerialFunction::RcInput)
            .with(LogicalSerialPort::Uart2, SerialFunction::MspDisplayPort)
            .with(LogicalSerialPort::Uart3, SerialFunction::EscTelemetry);
        let resolved = resolve_bindings(bindings, &ROUTES, RcProtocol::Sbus);
        assert_eq!(
            resolved.issues(),
            &[
                BindingIssue {
                    port: LogicalSerialPort::Uart1,
                    function: SerialFunction::RcInput,
                    fault: BindingFault::Unsupported(SerialRouteError::UnsupportedProtocol),
                },
                BindingIssue {
                    port: LogicalSerialPort::Uart2,
                    function: SerialFunction::MspDisplayPort,
                    fault: BindingFault::Unsupported(SerialRouteError::UnsupportedProtocol),
                },
                BindingIssue {
                    port: LogicalSerialPort::Uart3,
                    function: SerialFunction::EscTelemetry,
                    fault: BindingFault::NoSuchPort,
                },
            ]
        );
        for port in LogicalSerialPort::ALL {
            assert_eq!(resolved.function(port), SerialFunction::None);
        }
    }

    #[test]
    fn a_function_on_two_ports_is_on_neither() {
        let bindings = SerialBindings::none()
            .with(LogicalSerialPort::Uart1, SerialFunction::EscTelemetry)
            .with(LogicalSerialPort::Uart4, SerialFunction::EscTelemetry);
        let resolved = resolve_bindings(bindings, &ROUTES, RcProtocol::Sbus);
        assert_eq!(resolved.issues().len(), 2);
        assert!(
            resolved
                .issues()
                .iter()
                .all(|issue| issue.fault == BindingFault::Duplicate)
        );
        assert_eq!(
            resolved.function(LogicalSerialPort::Uart1),
            SerialFunction::None
        );
        assert_eq!(
            resolved.function(LogicalSerialPort::Uart4),
            SerialFunction::None
        );
    }

    #[test]
    fn bindings_round_trip_and_reject_unknown_functions() {
        let bindings = SerialBindings::none()
            .with(LogicalSerialPort::Uart2, SerialFunction::RcInput)
            .with(LogicalSerialPort::Uart6, SerialFunction::MspDisplayPort);
        assert_eq!(SerialBindings::decode(&bindings.encode()), Some(bindings));
        assert_eq!(SerialBindings::decode(&[0, 9, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn names_parse_back() {
        for function in SerialFunction::ALL {
            assert_eq!(SerialFunction::parse(function.name()), Some(function));
        }
        for port in LogicalSerialPort::ALL {
            assert_eq!(LogicalSerialPort::parse(port.name()), Some(port));
        }
    }

    #[test]
    fn slots_take_one_endpoint_per_function() {
        let mut slots = SerialFunctionSlots::empty();
        assert_eq!(slots.place(SerialFunction::RcInput, 2), Ok(()));
        assert_eq!(slots.place(SerialFunction::RcInput, 3), Err(3));
        assert_eq!(slots.place(SerialFunction::None, 4), Err(4));
        assert_eq!(slots.rc_input, Some(2));
        assert_eq!(slots.esc_telemetry, None);
    }
}
