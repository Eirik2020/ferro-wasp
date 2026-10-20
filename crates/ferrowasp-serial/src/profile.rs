/// A UART by its number on the chip, the name a pilot sees on the board:
/// USART3 is `Uart3` whichever function it serves. Eight covers the UARTs of
/// every supported family; the STM32H743 numbers its eighth UART8.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalSerialPort {
    Uart1,
    Uart2,
    Uart3,
    Uart4,
    Uart5,
    Uart6,
    Uart7,
    Uart8,
}

impl LogicalSerialPort {
    pub const ALL: [Self; 8] = [
        Self::Uart1,
        Self::Uart2,
        Self::Uart3,
        Self::Uart4,
        Self::Uart5,
        Self::Uart6,
        Self::Uart7,
        Self::Uart8,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Uart1 => "uart1",
            Self::Uart2 => "uart2",
            Self::Uart3 => "uart3",
            Self::Uart4 => "uart4",
            Self::Uart5 => "uart5",
            Self::Uart6 => "uart6",
            Self::Uart7 => "uart7",
            Self::Uart8 => "uart8",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|port| port.name() == name)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialProtocol {
    Disabled,
    Sbus,
    Crsf,
    Mavlink,
    Msp,
    EscTelemetry,
    /// FerroWasp's text command line, as on USB: a configurator over a
    /// serial or Bluetooth link.
    Cli,
}

pub const SBUS_FRAME_LEN: usize = 25;
pub const CRSF_FRAME_LEN: usize = 64;
pub const MAVLINK_MIN_FRAME_LEN: usize = 25;
pub const MSP_V1_MAX_PAYLOAD_LEN: usize = 64;
pub const MSP_V1_MAX_FRAME_LEN: usize = MSP_V1_MAX_PAYLOAD_LEN + 6;
pub const ESC_TELEMETRY_FRAME_LEN: usize = 10;
/// The command line has no frames; this is the most one receive chunk holds.
pub const CLI_CHUNK_LEN: usize = 64;

impl SerialProtocol {
    /// The protocol talks back, so its port needs a transmit path.
    pub const fn needs_tx(self) -> bool {
        matches!(
            self,
            SerialProtocol::Crsf | SerialProtocol::Msp | SerialProtocol::Cli
        )
    }

    pub const fn frame_size(self) -> usize {
        match self {
            SerialProtocol::Disabled => 0,
            SerialProtocol::Sbus => SBUS_FRAME_LEN,
            SerialProtocol::Crsf => CRSF_FRAME_LEN,
            SerialProtocol::Mavlink => MAVLINK_MIN_FRAME_LEN,
            SerialProtocol::Msp => MSP_V1_MAX_FRAME_LEN,
            SerialProtocol::EscTelemetry => ESC_TELEMETRY_FRAME_LEN,
            SerialProtocol::Cli => CLI_CHUNK_LEN,
        }
    }

    pub const fn max_frame_size() -> usize {
        let sizes = [
            SerialProtocol::Sbus.frame_size(),
            SerialProtocol::Crsf.frame_size(),
            SerialProtocol::Mavlink.frame_size(),
            SerialProtocol::Msp.frame_size(),
            SerialProtocol::EscTelemetry.frame_size(),
            SerialProtocol::Cli.frame_size(),
        ];

        let mut max = 0;
        let mut i = 0;

        while i < sizes.len() {
            if sizes[i] > max {
                max = sizes[i];
            }

            i += 1;
        }

        max
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialProfile {
    pub protocol: SerialProtocol,
    pub baud: u32,
    pub word_bits: u8,
    pub stop_bits: u8,
    pub parity_even: bool,
}

impl SerialProfile {
    pub const fn disabled() -> Self {
        Self {
            protocol: SerialProtocol::Disabled,
            baud: 0,
            word_bits: 8,
            stop_bits: 1,
            parity_even: false,
        }
    }

    pub const fn sbus() -> Self {
        Self {
            protocol: SerialProtocol::Sbus,
            baud: 100_000,
            word_bits: 9,
            stop_bits: 2,
            parity_even: true,
        }
    }

    /// CRSF at the 420 kbaud receivers default to, 8N1, not inverted.
    pub const fn crsf() -> Self {
        Self {
            protocol: SerialProtocol::Crsf,
            baud: 420_000,
            word_bits: 8,
            stop_bits: 1,
            parity_even: false,
        }
    }

    pub const fn msp() -> Self {
        Self {
            protocol: SerialProtocol::Msp,
            baud: 115_200,
            word_bits: 8,
            stop_bits: 1,
            parity_even: false,
        }
    }

    /// The command line at 115200 8N1, the rate Bluetooth serial modules
    /// default to.
    pub const fn cli() -> Self {
        Self {
            protocol: SerialProtocol::Cli,
            baud: 115_200,
            word_bits: 8,
            stop_bits: 1,
            parity_even: false,
        }
    }

    pub const fn esc_telemetry() -> Self {
        Self {
            protocol: SerialProtocol::EscTelemetry,
            baud: 115_200,
            word_bits: 8,
            stop_bits: 1,
            parity_even: false,
        }
    }
}
