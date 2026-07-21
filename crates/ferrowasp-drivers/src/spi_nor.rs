//! Small allocation-free driver for JEDEC-compatible SPI NOR flash.
//!
//! Operations start a bounded wire transaction and return immediately after
//! the bytes are transferred. Callers poll [`SpiNor::read_status`] later;
//! this driver never spins for an erase or program operation to finish.

use embedded_hal::{digital::OutputPin, spi::SpiBus};

pub const PAGE_SIZE: usize = 256;
pub const SECTOR_SIZE: u32 = 4096;
const CMD_READ: u8 = 0x03;
const CMD_READ_STATUS: u8 = 0x05;
const CMD_WRITE_ENABLE: u8 = 0x06;
const CMD_PAGE_PROGRAM: u8 = 0x02;
const CMD_SECTOR_ERASE_4K: u8 = 0x20;
const CMD_JEDEC_ID: u8 = 0x9f;
const STATUS_BUSY: u8 = 1 << 0;
const STATUS_WRITE_ENABLE_LATCH: u8 = 1 << 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JedecId {
    pub manufacturer: u8,
    pub memory_type: u8,
    pub capacity_code: u8,
}

impl JedecId {
    pub const fn capacity_bytes(self) -> Option<u32> {
        if self.capacity_code < 8 || self.capacity_code > 31 {
            None
        } else {
            Some(1u32 << self.capacity_code)
        }
    }

    pub const fn plausible(self) -> bool {
        self.manufacturer != 0 && self.manufacturer != 0xff && self.capacity_bytes().is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Status(pub u8);

impl Status {
    pub const fn busy(self) -> bool {
        self.0 & STATUS_BUSY != 0
    }

    pub const fn write_enabled(self) -> bool {
        self.0 & STATUS_WRITE_ENABLE_LATCH != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error<SpiError, PinError> {
    Spi(SpiError),
    ChipSelect(PinError),
    InvalidAddress,
    EmptyWrite,
    PageBoundary,
}

pub struct SpiNor<SPI, CS> {
    spi: SPI,
    cs: CS,
}

impl<SPI, CS> SpiNor<SPI, CS> {
    pub const fn new(spi: SPI, cs: CS) -> Self {
        Self { spi, cs }
    }

    pub fn free(self) -> (SPI, CS) {
        (self.spi, self.cs)
    }
}

impl<SPI, CS> SpiNor<SPI, CS>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
{
    pub fn read_jedec_id(&mut self) -> Result<JedecId, Error<SPI::Error, CS::Error>> {
        let mut bytes = [CMD_JEDEC_ID, 0, 0, 0];
        self.transaction(|spi| spi.transfer_in_place(&mut bytes))?;
        Ok(JedecId {
            manufacturer: bytes[1],
            memory_type: bytes[2],
            capacity_code: bytes[3],
        })
    }

    pub fn read_status(&mut self) -> Result<Status, Error<SPI::Error, CS::Error>> {
        let mut bytes = [CMD_READ_STATUS, 0];
        self.transaction(|spi| spi.transfer_in_place(&mut bytes))?;
        Ok(Status(bytes[1]))
    }

    pub fn read(
        &mut self,
        address: u32,
        output: &mut [u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let header = address_header(CMD_READ, address)?;
        self.cs.set_low().map_err(Error::ChipSelect)?;
        let result = self
            .spi
            .write(&header)
            .and_then(|()| self.spi.read(output))
            .map_err(Error::Spi);
        let deselect = self.cs.set_high().map_err(Error::ChipSelect);
        result.and(deselect)
    }

    pub fn write_enable(&mut self) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.transaction(|spi| spi.write(&[CMD_WRITE_ENABLE]))
    }

    pub fn page_program(
        &mut self,
        address: u32,
        bytes: &[u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        if bytes.is_empty() {
            return Err(Error::EmptyWrite);
        }
        let page_offset = address as usize & (PAGE_SIZE - 1);
        if bytes.len() > PAGE_SIZE || page_offset + bytes.len() > PAGE_SIZE {
            return Err(Error::PageBoundary);
        }
        let header = address_header(CMD_PAGE_PROGRAM, address)?;
        self.write_enable()?;
        self.cs.set_low().map_err(Error::ChipSelect)?;
        let result = self
            .spi
            .write(&header)
            .and_then(|()| self.spi.write(bytes))
            .map_err(Error::Spi);
        let deselect = self.cs.set_high().map_err(Error::ChipSelect);
        result.and(deselect)
    }

    pub fn erase_sector_4k(&mut self, address: u32) -> Result<(), Error<SPI::Error, CS::Error>> {
        if !address.is_multiple_of(SECTOR_SIZE) {
            return Err(Error::InvalidAddress);
        }
        let command = address_header(CMD_SECTOR_ERASE_4K, address)?;
        self.write_enable()?;
        self.transaction(|spi| spi.write(&command))
    }

    fn transaction<F>(&mut self, operation: F) -> Result<(), Error<SPI::Error, CS::Error>>
    where
        F: FnOnce(&mut SPI) -> Result<(), SPI::Error>,
    {
        self.cs.set_low().map_err(Error::ChipSelect)?;
        let result = operation(&mut self.spi).map_err(Error::Spi);
        let deselect = self.cs.set_high().map_err(Error::ChipSelect);
        result.and(deselect)
    }
}

fn address_header<SpiError, PinError>(
    command: u8,
    address: u32,
) -> Result<[u8; 4], Error<SpiError, PinError>> {
    if address > 0x00ff_ffff {
        return Err(Error::InvalidAddress);
    }
    Ok([
        command,
        (address >> 16) as u8,
        (address >> 8) as u8,
        address as u8,
    ])
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::{digital::ErrorType as DigitalErrorType, spi::ErrorType as SpiErrorType};
    use std::vec::Vec;

    #[derive(Default)]
    struct MockPin {
        states: Vec<bool>,
    }

    impl DigitalErrorType for MockPin {
        type Error = Infallible;
    }

    impl OutputPin for MockPin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.states.push(false);
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.states.push(true);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockSpi {
        writes: Vec<Vec<u8>>,
    }

    impl SpiErrorType for MockSpi {
        type Error = Infallible;
    }

    impl SpiBus<u8> for MockSpi {
        fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            words.fill(0x5a);
            Ok(())
        }

        fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
            self.writes.push(words.to_vec());
            Ok(())
        }

        fn transfer(&mut self, read: &mut [u8], _write: &[u8]) -> Result<(), Self::Error> {
            read.fill(0);
            Ok(())
        }

        fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            if words.first() == Some(&CMD_JEDEC_ID) {
                words[1..].copy_from_slice(&[0xef, 0x40, 0x18]);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn jedec_128_megabit_capacity_is_sixteen_megabytes() {
        let id = JedecId {
            manufacturer: 0xef,
            memory_type: 0x40,
            capacity_code: 0x18,
        };
        assert!(id.plausible());
        assert_eq!(id.capacity_bytes(), Some(16 * 1024 * 1024));
    }

    #[test]
    fn three_byte_address_rejects_out_of_range_values() {
        assert_eq!(
            address_header::<(), ()>(CMD_READ, 0x0100_0000),
            Err(Error::InvalidAddress)
        );
        assert_eq!(
            address_header::<(), ()>(CMD_READ, 0x00ab_cdef),
            Ok([CMD_READ, 0xab, 0xcd, 0xef])
        );
    }

    #[test]
    fn jedec_probe_selects_device_and_decodes_reply() {
        let mut flash = SpiNor::new(MockSpi::default(), MockPin::default());
        assert_eq!(
            flash.read_jedec_id().unwrap(),
            JedecId {
                manufacturer: 0xef,
                memory_type: 0x40,
                capacity_code: 0x18,
            }
        );
        let (_, pin) = flash.free();
        assert_eq!(pin.states, [false, true]);
    }

    #[test]
    fn page_program_sends_write_enable_header_and_data_with_bounded_cs() {
        let mut flash = SpiNor::new(MockSpi::default(), MockPin::default());
        flash.page_program(0x0012_3400, &[1, 2, 3]).unwrap();
        let (spi, pin) = flash.free();
        assert_eq!(
            spi.writes,
            [
                Vec::from([CMD_WRITE_ENABLE]),
                Vec::from([CMD_PAGE_PROGRAM, 0x12, 0x34, 0x00]),
                Vec::from([1, 2, 3]),
            ]
        );
        assert_eq!(pin.states, [false, true, false, true]);
    }

    #[test]
    fn read_keeps_chip_selected_across_header_and_payload() {
        let mut output = [0u8; 4];
        let mut flash = SpiNor::new(MockSpi::default(), MockPin::default());
        flash.read(0x0001_0203, &mut output).unwrap();
        let (spi, pin) = flash.free();
        assert_eq!(spi.writes, [Vec::from([CMD_READ, 1, 2, 3])]);
        assert_eq!(output, [0x5a; 4]);
        assert_eq!(pin.states, [false, true]);
    }
}
