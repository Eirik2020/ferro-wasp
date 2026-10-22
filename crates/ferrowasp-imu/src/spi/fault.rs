//! Requirements: IMU-004
//! Safety-critical.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiFault {
    Busy,
    Oversize,
    DmaTransfer,
    Timeout,
    Cancelled,
    Unavailable,
    Backend,
}
