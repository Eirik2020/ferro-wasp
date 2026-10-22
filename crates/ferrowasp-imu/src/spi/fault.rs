//! Requirements: IMU-004, IMU-009, IMU-010, IMU-011, IMU-012, IMU-013
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
