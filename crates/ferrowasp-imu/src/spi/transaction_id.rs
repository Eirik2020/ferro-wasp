//! Requirements: IMU-004, IMU-009, IMU-010, IMU-011, IMU-012, IMU-013
//! Safety-critical.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransactionId(pub u32);

impl TransactionId {
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}
