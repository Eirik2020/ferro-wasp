//! An in-memory store with SPI NOR semantics, for this crate's tests:
//! programming only clears bits, erasing sets a 4 KiB sector to `0xFF`, and
//! each operation can be made to fail.

extern crate std;

use std::{vec, vec::Vec};

use ferrowasp_drivers::nor_storage::NorStorage;

use crate::flash_storage::CONFIG_SECTOR_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fault;

pub(crate) struct MemoryNor {
    pub(crate) bytes: Vec<u8>,
    pub(crate) fail_erase: bool,
    pub(crate) fail_program: bool,
    /// Programs this byte value over the first byte of each page program, as
    /// a cell that will not take the bit pattern asked for.
    pub(crate) stuck_first_byte: Option<u8>,
}

impl MemoryNor {
    pub(crate) fn erased(capacity: u32) -> Self {
        Self {
            bytes: vec![0xff; capacity as usize],
            fail_erase: false,
            fail_program: false,
            stuck_first_byte: None,
        }
    }
}

impl NorStorage for MemoryNor {
    type Error = Fault;

    fn read(&mut self, address: u32, output: &mut [u8]) -> Result<(), Fault> {
        let start = address as usize;
        output.copy_from_slice(&self.bytes[start..start + output.len()]);
        Ok(())
    }

    fn page_program(&mut self, address: u32, bytes: &[u8]) -> Result<(), Fault> {
        if self.fail_program {
            return Err(Fault);
        }
        let start = address as usize;
        for (offset, byte) in bytes.iter().enumerate() {
            self.bytes[start + offset] &= byte;
        }
        if let Some(stuck) = self.stuck_first_byte {
            self.bytes[start] = stuck;
        }
        Ok(())
    }

    fn erase_sector_4k(&mut self, address: u32) -> Result<(), Fault> {
        if self.fail_erase {
            return Err(Fault);
        }
        let start = address as usize;
        self.bytes[start..start + CONFIG_SECTOR_SIZE as usize].fill(0xff);
        Ok(())
    }

    fn busy(&mut self) -> Result<bool, Fault> {
        Ok(false)
    }
}
