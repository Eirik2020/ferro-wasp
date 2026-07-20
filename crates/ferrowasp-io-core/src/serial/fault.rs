#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialFault {
    Disabled,
    UnsupportedProtocol,
    DmaTransfer,
    QueueOverflow,
    Timeout,
    InvalidChunk,
    InvalidState,
}

impl core::fmt::Display for SerialFault {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Disabled => "serial stream disabled",
            Self::UnsupportedProtocol => "serial protocol unsupported",
            Self::DmaTransfer => "serial DMA transfer failed",
            Self::QueueOverflow => "serial queue overflowed",
            Self::Timeout => "serial operation timed out",
            Self::InvalidChunk => "serial chunk is invalid",
            Self::InvalidState => "serial transport state is invalid",
        })
    }
}

impl core::error::Error for SerialFault {}

impl embedded_io::Error for SerialFault {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Self::Disabled => embedded_io::ErrorKind::NotConnected,
            Self::QueueOverflow => embedded_io::ErrorKind::OutOfMemory,
            Self::Timeout => embedded_io::ErrorKind::TimedOut,
            Self::InvalidChunk | Self::InvalidState => embedded_io::ErrorKind::InvalidData,
            Self::UnsupportedProtocol | Self::DmaTransfer => embedded_io::ErrorKind::Other,
        }
    }
}
