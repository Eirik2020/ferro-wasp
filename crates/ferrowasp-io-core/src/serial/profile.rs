#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalSerialPort {
    Uart1,
    Uart2,
    Uart3,
    Uart4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialProtocol {
    Disabled,
    Raw,
    Sbus,
    Crsf,
    Mavlink,
    Msp,
    EscTelemetry,
}

pub const SBUS_FRAME_LEN: usize = 25;
pub const CRSF_FRAME_LEN: usize = 64;
pub const MAVLINK_MIN_FRAME_LEN: usize = 25;
pub const MSP_V1_MAX_PAYLOAD_LEN: usize = 64;
pub const MSP_V1_MAX_FRAME_LEN: usize = MSP_V1_MAX_PAYLOAD_LEN + 6;
pub const ESC_TELEMETRY_FRAME_LEN: usize = 10;
pub const RAW_LINE_MAX_LEN: usize = 64;

impl SerialProtocol {
    pub const fn frame_size(self) -> usize {
        match self {
            SerialProtocol::Disabled => 0,
            SerialProtocol::Raw => RAW_LINE_MAX_LEN,
            SerialProtocol::Sbus => SBUS_FRAME_LEN,
            SerialProtocol::Crsf => CRSF_FRAME_LEN,
            SerialProtocol::Mavlink => MAVLINK_MIN_FRAME_LEN,
            SerialProtocol::Msp => MSP_V1_MAX_FRAME_LEN,
            SerialProtocol::EscTelemetry => ESC_TELEMETRY_FRAME_LEN,
        }
    }

    pub const fn max_frame_size() -> usize {
        let sizes = [
            SerialProtocol::Raw.frame_size(),
            SerialProtocol::Sbus.frame_size(),
            SerialProtocol::Crsf.frame_size(),
            SerialProtocol::Mavlink.frame_size(),
            SerialProtocol::Msp.frame_size(),
            SerialProtocol::EscTelemetry.frame_size(),
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

    pub const fn raw() -> Self {
        Self {
            protocol: SerialProtocol::Raw,
            baud: 115_200,
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
