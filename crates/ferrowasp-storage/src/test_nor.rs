//! An in-memory store with SPI NOR semantics, for host tests of the storage,
//! blackbox and command-line crates: programming only clears bits, erasing
//! sets a 4 KiB sector to `0xFF`, and each operation can be made to fail.

extern crate std;

use std::{vec, vec::Vec};

use crate::nor_storage::NorStorage;

use crate::layout::CONFIG_SECTOR_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fault;

pub struct MemoryNor {
    pub bytes: Vec<u8>,
    pub fail_erase: bool,
    pub fail_program: bool,
    /// Programs this byte value over the first byte of each page program, as
    /// a cell that will not take the bit pattern asked for.
    pub stuck_first_byte: Option<u8>,
    /// Holds programmed pages in RAM until `flush`, as an SD card's
    /// write-back window does.
    pub write_back: bool,
    pub held: bool,
    pub flushes: u32,
}

impl MemoryNor {
    pub fn erased(capacity: u32) -> Self {
        Self {
            bytes: vec![0xff; capacity as usize],
            fail_erase: false,
            fail_program: false,
            stuck_first_byte: None,
            write_back: false,
            held: false,
            flushes: 0,
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
        self.held |= self.write_back;
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

    fn write_back_pending(&self) -> bool {
        self.held
    }

    fn flush(&mut self) -> Result<(), Fault> {
        self.held = false;
        self.flushes += 1;
        Ok(())
    }
}
