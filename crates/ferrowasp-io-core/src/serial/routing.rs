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
