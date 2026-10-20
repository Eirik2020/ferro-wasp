#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialRoute {
    pub logical: super::LogicalSerialPort,
    pub peripheral: &'static str,
    pub tx_pin: &'static str,
    pub rx_pin: &'static str,
    pub profile: super::SerialProfile,
    pub rx_dma: &'static str,
    pub tx_dma: Option<&'static str>,
    pub capabilities: SerialCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialCapabilities {
    pub sbus: bool,
    pub crsf: bool,
    pub mavlink: bool,
    pub msp: bool,
    pub esc_telemetry: bool,
    /// The text command line; it answers, so it also needs `tx`.
    pub cli: bool,
    pub tx: bool,
}

impl SerialCapabilities {
    pub const fn accepts(self, profile: super::SerialProfile) -> bool {
        match profile.protocol {
            super::SerialProtocol::Disabled => true,
            super::SerialProtocol::Sbus => self.sbus,
            super::SerialProtocol::Crsf => self.crsf,
            super::SerialProtocol::Mavlink => self.mavlink,
            super::SerialProtocol::Msp => self.msp,
            super::SerialProtocol::EscTelemetry => self.esc_telemetry,
            super::SerialProtocol::Cli => self.cli,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialRouteError {
    UnsupportedProtocol,
    MissingTxDma,
}

impl SerialRoute {
    pub fn validate_profile(self, profile: super::SerialProfile) -> Result<(), SerialRouteError> {
        if !self.capabilities.accepts(profile) {
            return Err(SerialRouteError::UnsupportedProtocol);
        }
        if profile.protocol.needs_tx() && !self.capabilities.tx {
            return Err(SerialRouteError::MissingTxDma);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LogicalSerialPort, SerialProfile, SerialProtocol};

    fn route(capabilities: SerialCapabilities) -> SerialRoute {
        SerialRoute {
            logical: LogicalSerialPort::Uart2,
            peripheral: "USART2",
            tx_pin: "PA2",
            rx_pin: "PA3",
            profile: SerialProfile::disabled(),
            rx_dma: "DMA1_STREAM5",
            tx_dma: None,
            capabilities,
        }
    }

    const NONE: SerialCapabilities = SerialCapabilities {
        sbus: false,
        crsf: false,
        mavlink: false,
        msp: false,
        esc_telemetry: false,
        cli: false,
        tx: false,
    };

    #[test]
    fn each_protocol_uses_its_line_settings() {
        let line = |p: SerialProfile| (p.protocol, p.baud, p.word_bits, p.stop_bits, p.parity_even);
        assert_eq!(
            line(SerialProfile::sbus()),
            (SerialProtocol::Sbus, 100_000, 9, 2, true),
            "SBUS is 8 data bits plus even parity, two stop bits"
        );
        assert_eq!(
            line(SerialProfile::crsf()),
            (SerialProtocol::Crsf, 420_000, 8, 1, false)
        );
        for (profile, protocol) in [
            (SerialProfile::msp(), SerialProtocol::Msp),
            (SerialProfile::cli(), SerialProtocol::Cli),
            (SerialProfile::esc_telemetry(), SerialProtocol::EscTelemetry),
        ] {
            assert_eq!(line(profile), (protocol, 115_200, 8, 1, false));
        }
    }

    #[test]
    fn a_port_refuses_what_its_wiring_cannot_carry() {
        let sbus_only = route(SerialCapabilities { sbus: true, ..NONE });
        assert_eq!(sbus_only.validate_profile(SerialProfile::sbus()), Ok(()));
        assert_eq!(
            sbus_only.validate_profile(SerialProfile::disabled()),
            Ok(())
        );
        assert_eq!(
            sbus_only.validate_profile(SerialProfile::crsf()),
            Err(SerialRouteError::UnsupportedProtocol)
        );

        let crsf_receive_only = route(SerialCapabilities { crsf: true, ..NONE });
        assert_eq!(
            crsf_receive_only.validate_profile(SerialProfile::crsf()),
            Err(SerialRouteError::MissingTxDma),
            "CRSF answers with telemetry, so it needs a transmit path"
        );
        let crsf = route(SerialCapabilities {
            crsf: true,
            tx: true,
            ..NONE
        });
        assert_eq!(crsf.validate_profile(SerialProfile::crsf()), Ok(()));
    }

    #[test]
    fn only_protocols_that_answer_need_a_transmit_path() {
        assert!(SerialProtocol::Crsf.needs_tx());
        assert!(SerialProtocol::Msp.needs_tx());
        assert!(SerialProtocol::Cli.needs_tx());
        assert!(!SerialProtocol::Sbus.needs_tx());
        assert!(!SerialProtocol::EscTelemetry.needs_tx());
        assert!(!SerialProtocol::Disabled.needs_tx());
    }
}
