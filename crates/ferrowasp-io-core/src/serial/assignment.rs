/// Radio-control protocol selected for a serial-port consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RcProtocol {
    /// Futaba SBUS framing and packets.
    Sbus,
}

/// Fixed startup assignment selected for a configurable serial port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialPortAssignment {
    /// Leave the serial endpoint uninitialized.
    Disabled,

    /// Decode radio-control input using the selected protocol.
    Rc(RcProtocol),

    /// Receive raw 115200-baud, 8-N-1, line-oriented input.
    ComPort,
}

impl SerialPortAssignment {
    /// Returns the serial profile required by this assignment.
    pub const fn profile(self) -> super::SerialProfile {
        match self {
            Self::Disabled => super::SerialProfile::disabled(),
            Self::Rc(RcProtocol::Sbus) => super::SerialProfile::sbus(),
            Self::ComPort => super::SerialProfile::raw(),
        }
    }
}
