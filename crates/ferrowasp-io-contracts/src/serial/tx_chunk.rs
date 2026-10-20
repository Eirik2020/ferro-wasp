#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxChunkError {
    Empty,
    CapacityExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TxChunk<const N: usize> {
    pub bytes: [u8; N],
    pub len: u16,
}

impl<const N: usize> TxChunk<N> {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, TxChunkError> {
        if bytes.is_empty() {
            return Err(TxChunkError::Empty);
        }
        if bytes.len() > N || bytes.len() > usize::from(u16::MAX) {
            return Err(TxChunkError::CapacityExceeded);
        }

        let mut owned = [0; N];
        owned[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: owned,
            len: bytes.len() as u16,
        })
    }

    pub fn as_slice(&self) -> Result<&[u8], TxChunkError> {
        let len = usize::from(self.len);
        if len == 0 {
            return Err(TxChunkError::Empty);
        }
        self.bytes.get(..len).ok_or(TxChunkError::CapacityExceeded)
    }
}
