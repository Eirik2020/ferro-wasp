#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveformFault {
    Busy,
    Underrun,
    DmaTransfer,
    UnsafeEndpointUnavailable,
}
