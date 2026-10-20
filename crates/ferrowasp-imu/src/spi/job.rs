use super::TransactionId;
use heapless::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnedSpiOperation {
    Read {
        rx_offset: usize,
        len: usize,
    },
    Write {
        tx_offset: usize,
        len: usize,
    },
    Transfer {
        tx_offset: usize,
        tx_len: usize,
        rx_offset: usize,
        rx_len: usize,
        wire_len: usize,
    },
    TransferInPlace {
        tx_offset: usize,
        rx_offset: usize,
        len: usize,
    },
    DelayNs {
        ns: u32,
    },
}

pub enum SpiOperationRef<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
    Transfer { read: &'a mut [u8], write: &'a [u8] },
    TransferInPlace(&'a mut [u8]),
    DelayNs(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiJobError {
    TooManyOperations,
    TxCapacityExceeded,
    RxCapacityExceeded,
    CopybackShapeMismatch,
}

pub struct OwnedSpiJob<const MAX_OPS: usize, const MAX_BYTES: usize> {
    id: Option<TransactionId>,
    operations: Vec<OwnedSpiOperation, MAX_OPS>,
    tx: [u8; MAX_BYTES],
    rx: [u8; MAX_BYTES],
    tx_len: usize,
    rx_len: usize,
}

impl<const MAX_OPS: usize, const MAX_BYTES: usize> OwnedSpiJob<MAX_OPS, MAX_BYTES> {
    pub const fn new() -> Self {
        Self {
            id: None,
            operations: Vec::new(),
            tx: [0; MAX_BYTES],
            rx: [0; MAX_BYTES],
            tx_len: 0,
            rx_len: 0,
        }
    }

    pub fn pack(
        &mut self,
        id: TransactionId,
        operations: &[SpiOperationRef<'_>],
    ) -> Result<(), SpiJobError> {
        let mut staged = Self::new();
        staged.id = Some(id);
        staged.pack_operations(operations)?;
        *self = staged;
        Ok(())
    }

    pub const fn id(&self) -> Option<TransactionId> {
        self.id
    }

    pub fn operations(&self) -> &[OwnedSpiOperation] {
        self.operations.as_slice()
    }

    pub fn tx_bytes(&self) -> &[u8] {
        &self.tx[..self.tx_len]
    }

    pub fn rx_bytes(&self) -> &[u8] {
        &self.rx[..self.rx_len]
    }

    pub fn rx_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.rx[..self.rx_len]
    }

    pub fn copy_results(&self, operations: &mut [SpiOperationRef<'_>]) -> Result<(), SpiJobError> {
        self.validate_copyback_shapes(operations)?;

        for (owned, operation) in self.operations.iter().zip(operations.iter_mut()) {
            match (owned, operation) {
                (OwnedSpiOperation::Read { rx_offset, len }, SpiOperationRef::Read(read)) => {
                    read.copy_from_slice(&self.rx[*rx_offset..*rx_offset + *len]);
                }
                (
                    OwnedSpiOperation::Transfer {
                        rx_offset, rx_len, ..
                    },
                    SpiOperationRef::Transfer { read, .. },
                ) => {
                    read.copy_from_slice(&self.rx[*rx_offset..*rx_offset + *rx_len]);
                }
                (
                    OwnedSpiOperation::TransferInPlace { rx_offset, len, .. },
                    SpiOperationRef::TransferInPlace(data),
                ) => {
                    data.copy_from_slice(&self.rx[*rx_offset..*rx_offset + *len]);
                }
                (OwnedSpiOperation::Write { .. }, SpiOperationRef::Write(_))
                | (OwnedSpiOperation::DelayNs { .. }, SpiOperationRef::DelayNs(_)) => {}
                _ => return Err(SpiJobError::CopybackShapeMismatch),
            }
        }

        Ok(())
    }

    fn pack_operations(&mut self, operations: &[SpiOperationRef<'_>]) -> Result<(), SpiJobError> {
        for operation in operations {
            let owned = match operation {
                SpiOperationRef::Read(read) => {
                    let rx_offset = self.reserve_rx(read.len())?;
                    OwnedSpiOperation::Read {
                        rx_offset,
                        len: read.len(),
                    }
                }
                SpiOperationRef::Write(write) => {
                    let tx_offset = self.append_tx(write)?;
                    OwnedSpiOperation::Write {
                        tx_offset,
                        len: write.len(),
                    }
                }
                SpiOperationRef::Transfer { read, write } => {
                    let tx_offset = self.append_tx(write)?;
                    let rx_offset = self.reserve_rx(read.len())?;
                    OwnedSpiOperation::Transfer {
                        tx_offset,
                        tx_len: write.len(),
                        rx_offset,
                        rx_len: read.len(),
                        wire_len: write.len().max(read.len()),
                    }
                }
                SpiOperationRef::TransferInPlace(data) => {
                    let tx_offset = self.append_tx(data)?;
                    let rx_offset = self.reserve_rx(data.len())?;
                    OwnedSpiOperation::TransferInPlace {
                        tx_offset,
                        rx_offset,
                        len: data.len(),
                    }
                }
                SpiOperationRef::DelayNs(ns) => OwnedSpiOperation::DelayNs { ns: *ns },
            };

            self.operations
                .push(owned)
                .map_err(|_| SpiJobError::TooManyOperations)?;
        }

        Ok(())
    }

    fn append_tx(&mut self, bytes: &[u8]) -> Result<usize, SpiJobError> {
        let end = self
            .tx_len
            .checked_add(bytes.len())
            .filter(|end| *end <= MAX_BYTES)
            .ok_or(SpiJobError::TxCapacityExceeded)?;
        let offset = self.tx_len;
        self.tx[offset..end].copy_from_slice(bytes);
        self.tx_len = end;
        Ok(offset)
    }

    fn reserve_rx(&mut self, len: usize) -> Result<usize, SpiJobError> {
        let end = self
            .rx_len
            .checked_add(len)
            .filter(|end| *end <= MAX_BYTES)
            .ok_or(SpiJobError::RxCapacityExceeded)?;
        let offset = self.rx_len;
        self.rx_len = end;
        Ok(offset)
    }

    fn validate_copyback_shapes(
        &self,
        operations: &[SpiOperationRef<'_>],
    ) -> Result<(), SpiJobError> {
        if self.operations.len() != operations.len() {
            return Err(SpiJobError::CopybackShapeMismatch);
        }

        for (owned, operation) in self.operations.iter().zip(operations) {
            let matches = match (owned, operation) {
                (OwnedSpiOperation::Read { len, .. }, SpiOperationRef::Read(read)) => {
                    *len == read.len()
                }
                (OwnedSpiOperation::Write { len, .. }, SpiOperationRef::Write(write)) => {
                    *len == write.len()
                }
                (
                    OwnedSpiOperation::Transfer { tx_len, rx_len, .. },
                    SpiOperationRef::Transfer { read, write },
                ) => *tx_len == write.len() && *rx_len == read.len(),
                (
                    OwnedSpiOperation::TransferInPlace { len, .. },
                    SpiOperationRef::TransferInPlace(data),
                ) => *len == data.len(),
                (OwnedSpiOperation::DelayNs { ns }, SpiOperationRef::DelayNs(actual)) => {
                    *ns == *actual
                }
                _ => false,
            };

            if !matches {
                return Err(SpiJobError::CopybackShapeMismatch);
            }
        }

        Ok(())
    }
}

impl<const MAX_OPS: usize, const MAX_BYTES: usize> Default for OwnedSpiJob<MAX_OPS, MAX_BYTES> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestJob = OwnedSpiJob<8, 16>;

    #[test]
    fn packs_mixed_operations_without_retaining_borrows() {
        let mut read = [0; 3];
        let write = [0x10, 0x20];
        let transfer_write = [0x80, 0, 0, 0];
        let mut transfer_read = [0; 3];
        let mut in_place = [0xaa, 0xbb];
        let operations = [
            SpiOperationRef::Write(&write),
            SpiOperationRef::Read(&mut read),
            SpiOperationRef::Transfer {
                read: &mut transfer_read,
                write: &transfer_write,
            },
            SpiOperationRef::TransferInPlace(&mut in_place),
            SpiOperationRef::DelayNs(500),
        ];
        let mut job = TestJob::new();

        job.pack(TransactionId(7), &operations).unwrap();

        assert_eq!(job.id(), Some(TransactionId(7)));
        assert_eq!(job.tx_bytes(), &[0x10, 0x20, 0x80, 0, 0, 0, 0xaa, 0xbb]);
        assert_eq!(job.rx_bytes().len(), 8);
        assert_eq!(
            job.operations()[2],
            OwnedSpiOperation::Transfer {
                tx_offset: 2,
                tx_len: 4,
                rx_offset: 3,
                rx_len: 3,
                wire_len: 4,
            }
        );
    }

    #[test]
    fn copies_only_read_results_back_to_matching_operations() {
        let mut initial_read = [0; 2];
        let write = [1, 2];
        let mut initial_transfer = [0; 3];
        let transfer_write = [0x80, 0, 0];
        let mut initial_in_place = [9, 8];
        let packed = [
            SpiOperationRef::Read(&mut initial_read),
            SpiOperationRef::Write(&write),
            SpiOperationRef::Transfer {
                read: &mut initial_transfer,
                write: &transfer_write,
            },
            SpiOperationRef::TransferInPlace(&mut initial_in_place),
        ];
        let mut job = TestJob::new();
        job.pack(TransactionId(1), &packed).unwrap();
        job.rx_bytes_mut()
            .copy_from_slice(&[10, 11, 20, 21, 22, 30, 31]);

        let mut read = [0; 2];
        let mut transfer = [0; 3];
        let mut in_place = [9, 8];
        let mut copyback = [
            SpiOperationRef::Read(&mut read),
            SpiOperationRef::Write(&write),
            SpiOperationRef::Transfer {
                read: &mut transfer,
                write: &transfer_write,
            },
            SpiOperationRef::TransferInPlace(&mut in_place),
        ];

        job.copy_results(&mut copyback).unwrap();

        assert_eq!(read, [10, 11]);
        assert_eq!(transfer, [20, 21, 22]);
        assert_eq!(in_place, [30, 31]);
        assert_eq!(write, [1, 2]);
    }

    #[test]
    fn capacity_error_does_not_replace_previous_job() {
        let mut job = OwnedSpiJob::<2, 4>::new();
        let first_write = [1, 2];
        job.pack(TransactionId(3), &[SpiOperationRef::Write(&first_write)])
            .unwrap();

        let oversized = [0; 5];
        assert_eq!(
            job.pack(TransactionId(4), &[SpiOperationRef::Write(&oversized)]),
            Err(SpiJobError::TxCapacityExceeded)
        );
        assert_eq!(job.id(), Some(TransactionId(3)));
        assert_eq!(job.tx_bytes(), &[1, 2]);
    }

    #[test]
    fn operation_limit_is_checked_before_replacing_job() {
        let mut job = OwnedSpiJob::<1, 4>::new();
        let write = [1];
        let operations = [SpiOperationRef::Write(&write), SpiOperationRef::DelayNs(10)];

        assert_eq!(
            job.pack(TransactionId(0), &operations),
            Err(SpiJobError::TooManyOperations)
        );
        assert_eq!(job.id(), None);
    }

    #[test]
    fn copyback_shape_mismatch_is_rejected_before_any_write() {
        let mut packed_read = [0; 2];
        let packed = [SpiOperationRef::Read(&mut packed_read)];
        let mut job = TestJob::new();
        job.pack(TransactionId(0), &packed).unwrap();
        job.rx_bytes_mut().copy_from_slice(&[7, 8]);

        let mut wrong_len = [99; 1];
        let mut copyback = [SpiOperationRef::Read(&mut wrong_len)];
        assert_eq!(
            job.copy_results(&mut copyback),
            Err(SpiJobError::CopybackShapeMismatch)
        );
        assert_eq!(wrong_len, [99]);
    }
}
