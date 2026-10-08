//! Which recorded flights a host has acknowledged as stored.
//!
//! The ledger lives in the last 4 KiB sector of the flash, beside the log and
//! apart from the saved configuration. It is an 8-byte header followed by a
//! bitmap where bit `id - 1` belongs to flight `id`. NOR flash programs bits
//! from 1 to 0 without an erase, so acknowledging a flight clears its bit and
//! never rewrites the sector. A log erase clears the ledger along with the log
//! (see [`StorageLayout::log_erase_sector_address`]); flight identifiers then
//! restart at 1.

use crate::flash_storage::{CONFIG_SECTOR_SIZE, StorageLayout};

pub const SYNC_LEDGER_MAGIC: [u8; 8] = *b"FWSYNC01";
pub const SYNC_LEDGER_BITMAP_OFFSET: u32 = SYNC_LEDGER_MAGIC.len() as u32;
/// Flights one ledger can acknowledge before the log must be erased.
pub const SYNC_LEDGER_CAPACITY: u32 = (CONFIG_SECTOR_SIZE - SYNC_LEDGER_BITMAP_OFFSET) * 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LedgerHeader {
    /// Erased and never used.
    Blank,
    Valid,
    /// Something else, such as log pages written before the ledger existed.
    Foreign,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LedgerError<E> {
    /// Flight 0, or an identifier past [`SYNC_LEDGER_CAPACITY`].
    OutOfRange,
    Device(E),
}

/// The next flash operation needed to acknowledge a flight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkStep {
    /// The sector holds foreign data and must be erased first.
    EraseLedger {
        address: u32,
    },
    ProgramHeader {
        address: u32,
    },
    /// Program this one byte, which clears the flight's bit.
    ProgramBit {
        address: u32,
        value: u8,
    },
    /// The flight is already acknowledged.
    Done,
}

pub fn read_header<E, Read>(layout: StorageLayout, read: &mut Read) -> Result<LedgerHeader, E>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let mut header = [0u8; SYNC_LEDGER_MAGIC.len()];
    read(layout.sync_ledger_address(), &mut header)?;
    Ok(if header == SYNC_LEDGER_MAGIC {
        LedgerHeader::Valid
    } else if header.iter().all(|byte| *byte == 0xff) {
        LedgerHeader::Blank
    } else {
        LedgerHeader::Foreign
    })
}

/// The ledger byte holding `flight_id`'s bit, and that bit's mask.
pub const fn bit_location(layout: StorageLayout, flight_id: u32) -> Option<(u32, u8)> {
    if flight_id == 0 || flight_id > SYNC_LEDGER_CAPACITY {
        return None;
    }
    let bit = flight_id - 1;
    Some((
        layout.sync_ledger_address() + SYNC_LEDGER_BITMAP_OFFSET + bit / 8,
        1 << (bit % 8),
    ))
}

/// Whether a host has acknowledged `flight_id`. A blank or foreign ledger
/// acknowledges nothing, so its flights are offered again rather than lost.
pub fn is_synced<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    flight_id: u32,
) -> Result<bool, LedgerError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let (address, mask) = bit_location(layout, flight_id).ok_or(LedgerError::OutOfRange)?;
    if read_header(layout, read).map_err(LedgerError::Device)? != LedgerHeader::Valid {
        return Ok(false);
    }
    let mut byte = [0xff];
    read(address, &mut byte).map_err(LedgerError::Device)?;
    Ok(byte[0] & mask == 0)
}

/// Plans the next operation toward acknowledging `flight_id`. Call again after
/// each operation completes until it returns [`MarkStep::Done`].
pub fn next_mark_step<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    flight_id: u32,
) -> Result<MarkStep, LedgerError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let (address, mask) = bit_location(layout, flight_id).ok_or(LedgerError::OutOfRange)?;
    let ledger = layout.sync_ledger_address();
    match read_header(layout, read).map_err(LedgerError::Device)? {
        LedgerHeader::Foreign => Ok(MarkStep::EraseLedger { address: ledger }),
        LedgerHeader::Blank => Ok(MarkStep::ProgramHeader { address: ledger }),
        LedgerHeader::Valid => {
            let mut byte = [0xff];
            read(address, &mut byte).map_err(LedgerError::Device)?;
            if byte[0] & mask == 0 {
                Ok(MarkStep::Done)
            } else {
                Ok(MarkStep::ProgramBit {
                    address,
                    value: byte[0] & !mask,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPACITY: u32 = 64 * 1024;

    /// A NOR model: programming can only clear bits, erasing sets a sector.
    struct Nor([u8; CAPACITY as usize]);

    impl Nor {
        fn new() -> Self {
            Self([0xff; CAPACITY as usize])
        }

        fn read(&self, address: u32, output: &mut [u8]) -> Result<(), ()> {
            let start = address as usize;
            output.copy_from_slice(&self.0[start..start + output.len()]);
            Ok(())
        }

        fn program(&mut self, address: u32, bytes: &[u8]) {
            for (offset, byte) in bytes.iter().enumerate() {
                self.0[address as usize + offset] &= byte;
            }
        }

        fn erase(&mut self, address: u32) {
            let start = address as usize;
            self.0[start..start + CONFIG_SECTOR_SIZE as usize].fill(0xff);
        }

        fn mark(&mut self, layout: StorageLayout, flight_id: u32) -> usize {
            let mut operations = 0;
            loop {
                let step =
                    next_mark_step(layout, &mut |a, o: &mut [u8]| self.read(a, o), flight_id)
                        .unwrap();
                match step {
                    MarkStep::EraseLedger { address } => self.erase(address),
                    MarkStep::ProgramHeader { address } => {
                        self.program(address, &SYNC_LEDGER_MAGIC)
                    }
                    MarkStep::ProgramBit { address, value } => self.program(address, &[value]),
                    MarkStep::Done => return operations,
                }
                operations += 1;
            }
        }

        fn synced(&self, layout: StorageLayout, flight_id: u32) -> bool {
            is_synced(layout, &mut |a, o: &mut [u8]| self.read(a, o), flight_id).unwrap()
        }
    }

    #[test]
    fn marking_a_flight_writes_the_header_once_then_only_its_own_bit() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let mut nor = Nor::new();
        assert!(!nor.synced(layout, 3));

        assert_eq!(nor.mark(layout, 3), 2);
        assert_eq!(nor.mark(layout, 9), 1);
        assert_eq!(nor.mark(layout, 3), 0);

        assert!(nor.synced(layout, 3));
        assert!(nor.synced(layout, 9));
        for other in [1, 2, 4, 8, 10, SYNC_LEDGER_CAPACITY] {
            assert!(!nor.synced(layout, other), "flight {other}");
        }
    }

    #[test]
    fn a_foreign_ledger_acknowledges_nothing_until_erased() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let mut nor = Nor::new();
        // Old log pages in the ledger sector: mostly zero bits.
        nor.program(
            layout.sync_ledger_address(),
            &[0x46, 0x42, 0, 0, 0, 0, 0, 0, 0],
        );
        assert!(!nor.synced(layout, 1));

        assert_eq!(nor.mark(layout, 1), 3);
        assert!(nor.synced(layout, 1));
        assert!(!nor.synced(layout, 2));
    }

    #[test]
    fn identifiers_outside_the_bitmap_are_rejected() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let nor = Nor::new();
        let mut read = |a, o: &mut [u8]| nor.read(a, o);
        assert_eq!(
            next_mark_step(layout, &mut read, 0),
            Err(LedgerError::OutOfRange)
        );
        assert_eq!(
            next_mark_step(layout, &mut read, SYNC_LEDGER_CAPACITY + 1),
            Err(LedgerError::OutOfRange)
        );
        assert_eq!(
            bit_location(layout, SYNC_LEDGER_CAPACITY).map(|(address, _)| address),
            Some(CAPACITY - 1)
        );
    }
}
