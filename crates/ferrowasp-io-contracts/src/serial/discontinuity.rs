use crate::time::TimestampMicros;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StreamGeneration(pub u32);

impl StreamGeneration {
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Discontinuity {
    QueueOverflow,
    DmaError,
    FramingError,
    PortReconfigured,
    TransportReset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscontinuityRecord {
    pub sequence: u32,
    pub cause: Discontinuity,
    pub generation: StreamGeneration,
    pub observed_at: TimestampMicros,
}
