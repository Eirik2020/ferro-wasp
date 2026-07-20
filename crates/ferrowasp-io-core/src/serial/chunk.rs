use super::StreamGeneration;
use crate::time::TimestampMicros;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxCompletion {
    Idle,
    DmaFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxChunkError {
    Empty,
    CapacityExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RxChunk<const N: usize> {
    pub bytes: [u8; N],
    pub len: u16,
    pub timestamp: TimestampMicros,
    pub completion: RxCompletion,
    pub generation: StreamGeneration,
    pub discontinuity_before: bool,
    pub uart_error_seen: bool,
}

impl<const N: usize> RxChunk<N> {
    pub fn from_slice(
        bytes: &[u8],
        timestamp: TimestampMicros,
        completion: RxCompletion,
        generation: StreamGeneration,
        uart_error_seen: bool,
    ) -> Result<Self, RxChunkError> {
        if bytes.is_empty() {
            return Err(RxChunkError::Empty);
        }
        if bytes.len() > N || bytes.len() > usize::from(u16::MAX) {
            return Err(RxChunkError::CapacityExceeded);
        }

        let mut owned = [0; N];
        owned[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: owned,
            len: bytes.len() as u16,
            timestamp,
            completion,
            generation,
            discontinuity_before: false,
            uart_error_seen,
        })
    }

    pub fn as_slice(&self) -> Result<&[u8], RxChunkError> {
        let len = usize::from(self.len);
        if len == 0 {
            return Err(RxChunkError::Empty);
        }
        self.bytes.get(..len).ok_or(RxChunkError::CapacityExceeded)
    }
}
