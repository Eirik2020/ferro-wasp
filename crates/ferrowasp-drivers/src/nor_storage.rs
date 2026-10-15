//! Storage with SPI NOR semantics, as the flash manager uses it: reads,
//! page programs that only clear bits and never cross a 256-byte page, and
//! 4 KiB sector erases. A board's store is SPI NOR itself or a card standing
//! in for it.

use crate::spi_nor::{Error, SpiNor};
use embedded_hal::{digital::OutputPin, spi::SpiBus};

pub trait NorStorage {
    type Error;

    fn read(&mut self, address: u32, output: &mut [u8]) -> Result<(), Self::Error>;
    fn page_program(&mut self, address: u32, bytes: &[u8]) -> Result<(), Self::Error>;
    fn erase_sector_4k(&mut self, address: u32) -> Result<(), Self::Error>;

    /// Whether the last program or erase is still running. A store that
    /// completes each operation before returning is never busy.
    fn busy(&mut self) -> Result<bool, Self::Error>;

    /// Whether programs are held in RAM, waiting for [`Self::flush`]. Only a
    /// store with a write-back buffer ever says so.
    fn write_back_pending(&self) -> bool {
        false
    }

    /// Write held programs to the medium.
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// SPI NOR programs and erases in the background: the next operation waits
/// for the chip to report idle.
impl<SPI, CS> NorStorage for SpiNor<SPI, CS>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    type Error = Error<SPI::Error, CS::Error>;

    fn read(&mut self, address: u32, output: &mut [u8]) -> Result<(), Self::Error> {
        SpiNor::read(self, address, output)
    }

    fn page_program(&mut self, address: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        SpiNor::page_program(self, address, bytes)
    }

    fn erase_sector_4k(&mut self, address: u32) -> Result<(), Self::Error> {
        SpiNor::erase_sector_4k(self, address)
    }

    fn busy(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_status()?.busy())
    }
}
