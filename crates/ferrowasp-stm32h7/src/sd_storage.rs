//! SD card storage presented as a small SPI NOR flash, so the flash log and
//! configuration manager written for the F405 boards' SPI flash runs
//! unchanged on a board with an SD card slot.
//!
//! # Card layout
//!
//! - Block 0: a header naming this layout. A card is claimed only when block
//!   0 carries the header already, or carries no boot-sector signature
//!   (`0x55AA` at offset 510). A card holding a partition table or file
//!   system is refused, never overwritten: to use it, zero its first block.
//! - Block 1: one bit per 4 KiB sector, set once the sector has been erased
//!   since the card was claimed. A sector never erased reads as erased, so
//!   old card contents cannot look like log records.
//! - Blocks 2 onward: `NOR_CAPACITY_BYTES` of emulated flash, stored
//!   bit-inverted so a zeroed block reads as erased (`0xFF`).
//!
//! NOR semantics are kept: programming can only clear bits, and a program
//! never crosses a 256-byte page. Every operation completes before it
//! returns, so the status register never reports busy.

pub const BLOCK_SIZE: usize = 512;
pub const PAGE_SIZE: usize = 256;
pub const SECTOR_SIZE: u32 = 4096;
/// The emulated flash: 16 MiB, JEDEC capacity code 24.
pub const NOR_CAPACITY_CODE: u8 = 24;
pub const NOR_CAPACITY_BYTES: u32 = 1 << NOR_CAPACITY_CODE;
const SECTOR_COUNT: u32 = NOR_CAPACITY_BYTES / SECTOR_SIZE;
const BLOCKS_PER_SECTOR: u32 = SECTOR_SIZE / BLOCK_SIZE as u32;
const HEADER_LBA: u32 = 0;
const SECTOR_MAP_LBA: u32 = 1;
const DATA_LBA: u32 = 2;
/// Blocks the layout needs on the card.
pub const REQUIRED_BLOCKS: u32 = DATA_LBA + NOR_CAPACITY_BYTES / BLOCK_SIZE as u32;
const HEADER_MAGIC: &[u8; 16] = b"FerroWasp SD-NOR";
const HEADER_VERSION: u8 = 1;
const BOOT_SIGNATURE: [u8; 2] = [0x55, 0xaa];

/// The identity `read_jedec_id` reports once a card is claimed: an unused
/// manufacturer code, so tools cannot mistake it for a real part.
pub const SD_NOR_MANUFACTURER: u8 = 0xfd;
pub const SD_NOR_MEMORY_TYPE: u8 = 0x5d;

const _: () = assert!(SECTOR_COUNT as usize == BLOCK_SIZE * 8);

/// 512-byte block storage addressed by block number.
pub trait BlockDevice {
    type Error;

    fn block_count(&self) -> u32;
    fn read_block(&mut self, lba: u32, block: &mut [u8; BLOCK_SIZE]) -> Result<(), Self::Error>;
    fn write_block(&mut self, lba: u32, block: &[u8; BLOCK_SIZE]) -> Result<(), Self::Error>;
}

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
        self.0 & 1 != 0
    }

    pub const fn write_enabled(self) -> bool {
        self.0 & 2 != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefusalReason {
    /// Block 0 holds a partition table or file system.
    FormattedCard,
    /// The card has fewer than `REQUIRED_BLOCKS` blocks.
    TooSmall,
    /// Block 0 names a newer layout version.
    UnknownVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error<E> {
    Device(E),
    Refused(RefusalReason),
    InvalidAddress,
    EmptyWrite,
    PageBoundary,
}

impl<E> From<E> for Error<E> {
    fn from(error: E) -> Self {
        Self::Device(error)
    }
}

struct CachedBlock {
    lba: u32,
    data: [u8; BLOCK_SIZE],
}

/// An SD card seen as a 16 MiB SPI NOR flash. The card is claimed on the
/// first `read_jedec_id`.
pub struct SdNor<D> {
    device: D,
    claimed: bool,
    sector_map: [u8; BLOCK_SIZE],
    cache: Option<CachedBlock>,
}

impl<D> SdNor<D>
where
    D: BlockDevice,
{
    pub const fn new(device: D) -> Self {
        Self {
            device,
            claimed: false,
            sector_map: [0; BLOCK_SIZE],
            cache: None,
        }
    }

    pub fn free(self) -> D {
        self.device
    }

    /// Claim the card, then report the emulated part.
    pub fn read_jedec_id(&mut self) -> Result<JedecId, Error<D::Error>> {
        self.claim()?;
        Ok(JedecId {
            manufacturer: SD_NOR_MANUFACTURER,
            memory_type: SD_NOR_MEMORY_TYPE,
            capacity_code: NOR_CAPACITY_CODE,
        })
    }

    /// Never busy: every operation completes before it returns.
    pub fn read_status(&mut self) -> Result<Status, Error<D::Error>> {
        self.claim()?;
        Ok(Status(0))
    }

    pub fn write_enable(&mut self) -> Result<(), Error<D::Error>> {
        self.claim()
    }

    pub fn read(&mut self, address: u32, output: &mut [u8]) -> Result<(), Error<D::Error>> {
        self.claim()?;
        check_range(address, output.len())?;
        let mut address = address;
        let mut remaining = output;
        while !remaining.is_empty() {
            let offset = address as usize % BLOCK_SIZE;
            let len = remaining.len().min(BLOCK_SIZE - offset);
            let (chunk, rest) = remaining.split_at_mut(len);
            if self.sector_erased(address) {
                let block = self.load(data_lba(address))?;
                for (byte, stored) in chunk.iter_mut().zip(&block[offset..offset + len]) {
                    *byte = !stored;
                }
            } else {
                chunk.fill(0xff);
            }
            address += len as u32;
            remaining = rest;
        }
        Ok(())
    }

    /// NOR page program: each byte becomes `old & new`.
    pub fn page_program(&mut self, address: u32, bytes: &[u8]) -> Result<(), Error<D::Error>> {
        self.claim()?;
        if bytes.is_empty() {
            return Err(Error::EmptyWrite);
        }
        let page_offset = address as usize & (PAGE_SIZE - 1);
        if bytes.len() > PAGE_SIZE || page_offset + bytes.len() > PAGE_SIZE {
            return Err(Error::PageBoundary);
        }
        check_range(address, bytes.len())?;
        if !self.sector_erased(address) {
            // Programming a never-erased sector: erase it first, as the
            // flash it stands in for would already read erased.
            self.erase_sector_4k(address - address % SECTOR_SIZE)?;
        }

        let lba = data_lba(address);
        let offset = address as usize % BLOCK_SIZE;
        let mut block = *self.load(lba)?;
        for (stored, byte) in block[offset..offset + bytes.len()].iter_mut().zip(bytes) {
            // Stored inverted: !(old & new) == stored | !new.
            *stored |= !byte;
        }
        self.store(lba, &block)
    }

    pub fn erase_sector_4k(&mut self, address: u32) -> Result<(), Error<D::Error>> {
        self.claim()?;
        if !address.is_multiple_of(SECTOR_SIZE) {
            return Err(Error::InvalidAddress);
        }
        check_range(address, SECTOR_SIZE as usize)?;

        let zero = [0; BLOCK_SIZE];
        let first = data_lba(address);
        for lba in first..first + BLOCKS_PER_SECTOR {
            self.store(lba, &zero)?;
        }
        let sector = (address / SECTOR_SIZE) as usize;
        if self.sector_map[sector / 8] & (1 << (sector % 8)) == 0 {
            self.sector_map[sector / 8] |= 1 << (sector % 8);
            let map = self.sector_map;
            self.device.write_block(SECTOR_MAP_LBA, &map)?;
        }
        Ok(())
    }

    fn claim(&mut self) -> Result<(), Error<D::Error>> {
        if self.claimed {
            return Ok(());
        }
        if self.device.block_count() < REQUIRED_BLOCKS {
            return Err(Error::Refused(RefusalReason::TooSmall));
        }

        let mut header = [0; BLOCK_SIZE];
        self.device.read_block(HEADER_LBA, &mut header)?;
        if header[..HEADER_MAGIC.len()] == HEADER_MAGIC[..] {
            if header[HEADER_MAGIC.len()] != HEADER_VERSION {
                return Err(Error::Refused(RefusalReason::UnknownVersion));
            }
            let mut map = [0; BLOCK_SIZE];
            self.device.read_block(SECTOR_MAP_LBA, &mut map)?;
            self.sector_map = map;
        } else if header[BLOCK_SIZE - 2..] == BOOT_SIGNATURE {
            return Err(Error::Refused(RefusalReason::FormattedCard));
        } else {
            // Unrecognized contents: take the card, with no sector erased.
            self.sector_map = [0; BLOCK_SIZE];
            self.device.write_block(SECTOR_MAP_LBA, &[0; BLOCK_SIZE])?;
            let mut header = [0; BLOCK_SIZE];
            header[..HEADER_MAGIC.len()].copy_from_slice(HEADER_MAGIC);
            header[HEADER_MAGIC.len()] = HEADER_VERSION;
            self.device.write_block(HEADER_LBA, &header)?;
        }
        self.claimed = true;
        Ok(())
    }

    fn sector_erased(&self, address: u32) -> bool {
        let sector = (address / SECTOR_SIZE) as usize;
        self.sector_map[sector / 8] & (1 << (sector % 8)) != 0
    }

    fn load(&mut self, lba: u32) -> Result<&[u8; BLOCK_SIZE], Error<D::Error>> {
        if self.cache.as_ref().is_none_or(|cached| cached.lba != lba) {
            let mut data = [0; BLOCK_SIZE];
            self.cache = None;
            self.device.read_block(lba, &mut data)?;
            self.cache = Some(CachedBlock { lba, data });
        }
        match &self.cache {
            Some(cached) => Ok(&cached.data),
            None => unreachable!("the block was just cached"),
        }
    }

    /// Write through the one-block cache.
    fn store(&mut self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), Error<D::Error>> {
        self.cache = None;
        self.device.write_block(lba, data)?;
        self.cache = Some(CachedBlock { lba, data: *data });
        Ok(())
    }
}

fn check_range<E>(address: u32, len: usize) -> Result<(), Error<E>> {
    let end = u64::from(address) + len as u64;
    if end > u64::from(NOR_CAPACITY_BYTES) {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

const fn data_lba(address: u32) -> u32 {
    DATA_LBA + address / BLOCK_SIZE as u32
}

#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
pub use hal_card::{SdCardBlocks, SdCardInitError, SdCardResources, init_sd_card};

#[cfg(all(target_arch = "arm", feature = "stm32h743"))]
mod hal_card {
    use super::{BLOCK_SIZE, BlockDevice};
    use sdio_host::sd::CardCapacity;
    use stm32h7xx_hal::{
        gpio::{PC8, PC9, PC10, PC11, PC12, PD2, Pull, Speed},
        pac::SDMMC1,
        prelude::*,
        rcc::{CoreClocks, rec},
        sdmmc::{self, SdCard, Sdmmc, SdmmcExt},
    };

    /// The SD bus clock after identification.
    const SD_BUS_HZ: u32 = 25_000_000;

    pub struct SdCardResources {
        pub sdmmc: SDMMC1,
        pub prec: rec::Sdmmc1,
        pub clk: PC12,
        pub cmd: PD2,
        pub d0: PC8,
        pub d1: PC9,
        pub d2: PC10,
        pub d3: PC11,
    }

    /// An initialized high-capacity card on SDMMC1, in 4-bit mode. Transfers
    /// are polled, so no buffer needs to be DMA-reachable.
    pub struct SdCardBlocks {
        sdmmc: Sdmmc<SDMMC1, SdCard>,
        block_count: u32,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum SdCardInitError {
        Bus(sdmmc::Error),
        /// SDSC cards use byte addresses, which this driver does not.
        StandardCapacity,
    }

    pub fn init_sd_card(
        resources: SdCardResources,
        clocks: &CoreClocks,
    ) -> Result<SdCardBlocks, SdCardInitError> {
        let clk = resources
            .clk
            .into_alternate::<12>()
            .internal_pull_up(false)
            .speed(Speed::VeryHigh);
        let cmd = resources
            .cmd
            .into_alternate::<12>()
            .internal_pull_up(true)
            .speed(Speed::VeryHigh);
        let d0 = resources
            .d0
            .into_alternate::<12>()
            .internal_resistor(Pull::Up)
            .speed(Speed::VeryHigh);
        let d1 = resources
            .d1
            .into_alternate::<12>()
            .internal_resistor(Pull::Up)
            .speed(Speed::VeryHigh);
        let d2 = resources
            .d2
            .into_alternate::<12>()
            .internal_resistor(Pull::Up)
            .speed(Speed::VeryHigh);
        let d3 = resources
            .d3
            .into_alternate::<12>()
            .internal_resistor(Pull::Up)
            .speed(Speed::VeryHigh);

        let mut sdmmc: Sdmmc<SDMMC1, SdCard> =
            resources
                .sdmmc
                .sdmmc((clk, cmd, d0, d1, d2, d3), resources.prec, clocks);
        sdmmc.init(SD_BUS_HZ.Hz()).map_err(SdCardInitError::Bus)?;
        let card = sdmmc.card().map_err(SdCardInitError::Bus)?;
        if matches!(card.capacity, CardCapacity::StandardCapacity) {
            return Err(SdCardInitError::StandardCapacity);
        }
        let blocks = card.size() / BLOCK_SIZE as u64;
        Ok(SdCardBlocks {
            sdmmc,
            block_count: blocks.min(u64::from(u32::MAX)) as u32,
        })
    }

    impl BlockDevice for SdCardBlocks {
        type Error = sdmmc::Error;

        fn block_count(&self) -> u32 {
            self.block_count
        }

        fn read_block(
            &mut self,
            lba: u32,
            block: &mut [u8; BLOCK_SIZE],
        ) -> Result<(), sdmmc::Error> {
            self.sdmmc.read_block(lba, block)
        }

        fn write_block(&mut self, lba: u32, block: &[u8; BLOCK_SIZE]) -> Result<(), sdmmc::Error> {
            self.sdmmc.write_block(lba, block)
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::{vec, vec::Vec};

    struct MemoryCard {
        blocks: Vec<[u8; BLOCK_SIZE]>,
        writes: usize,
    }

    impl MemoryCard {
        fn new(block_count: u32) -> Self {
            Self {
                blocks: vec![[0; BLOCK_SIZE]; block_count as usize],
                writes: 0,
            }
        }
    }

    impl BlockDevice for MemoryCard {
        type Error = ();

        fn block_count(&self) -> u32 {
            self.blocks.len() as u32
        }

        fn read_block(&mut self, lba: u32, block: &mut [u8; BLOCK_SIZE]) -> Result<(), ()> {
            *block = *self.blocks.get(lba as usize).ok_or(())?;
            Ok(())
        }

        fn write_block(&mut self, lba: u32, block: &[u8; BLOCK_SIZE]) -> Result<(), ()> {
            *self.blocks.get_mut(lba as usize).ok_or(())? = *block;
            self.writes += 1;
            Ok(())
        }
    }

    fn claimed_card() -> SdNor<MemoryCard> {
        let mut nor = SdNor::new(MemoryCard::new(REQUIRED_BLOCKS));
        let id = nor.read_jedec_id().unwrap();
        assert!(id.plausible());
        assert_eq!(id.capacity_bytes(), Some(NOR_CAPACITY_BYTES));
        nor
    }

    #[test]
    fn formatted_card_is_refused_and_untouched() {
        let mut card = MemoryCard::new(REQUIRED_BLOCKS);
        card.blocks[0][510] = 0x55;
        card.blocks[0][511] = 0xaa;
        let mut nor = SdNor::new(card);
        assert_eq!(
            nor.read_jedec_id(),
            Err(Error::Refused(RefusalReason::FormattedCard))
        );
        let card = nor.free();
        assert_eq!(card.writes, 0);
    }

    #[test]
    fn small_card_is_refused() {
        let mut nor = SdNor::new(MemoryCard::new(REQUIRED_BLOCKS - 1));
        assert_eq!(
            nor.read_jedec_id(),
            Err(Error::Refused(RefusalReason::TooSmall))
        );
    }

    #[test]
    fn stale_card_contents_read_as_erased() {
        let mut card = MemoryCard::new(REQUIRED_BLOCKS);
        card.blocks[DATA_LBA as usize] = [0x12; BLOCK_SIZE];
        let mut nor = SdNor::new(card);
        nor.read_jedec_id().unwrap();
        let mut page = [0; PAGE_SIZE];
        nor.read(0, &mut page).unwrap();
        assert_eq!(page, [0xff; PAGE_SIZE]);
    }

    #[test]
    fn program_clears_bits_only_and_survives_a_reclaim() {
        let mut nor = claimed_card();
        nor.erase_sector_4k(SECTOR_SIZE).unwrap();
        nor.page_program(SECTOR_SIZE + 4, &[0xf0, 0x0f]).unwrap();
        nor.page_program(SECTOR_SIZE + 4, &[0x3c, 0xff]).unwrap();

        let mut nor = SdNor::new(nor.free());
        let mut bytes = [0; 4];
        nor.read(SECTOR_SIZE + 2, &mut bytes).unwrap();
        assert_eq!(bytes, [0xff, 0xff, 0x30, 0x0f]);
    }

    #[test]
    fn erase_restores_ones() {
        let mut nor = claimed_card();
        nor.erase_sector_4k(0).unwrap();
        nor.page_program(0, &[0; PAGE_SIZE]).unwrap();
        nor.erase_sector_4k(0).unwrap();
        let mut page = [0; PAGE_SIZE];
        nor.read(0, &mut page).unwrap();
        assert_eq!(page, [0xff; PAGE_SIZE]);
    }

    #[test]
    fn reads_span_blocks() {
        let mut nor = claimed_card();
        nor.erase_sector_4k(0).unwrap();
        nor.page_program(BLOCK_SIZE as u32 - 2, &[1, 2]).unwrap();
        nor.page_program(BLOCK_SIZE as u32, &[3, 4]).unwrap();
        let mut bytes = [0; 4];
        nor.read(BLOCK_SIZE as u32 - 2, &mut bytes).unwrap();
        assert_eq!(bytes, [1, 2, 3, 4]);
    }

    #[test]
    fn nor_rules_are_enforced() {
        let mut nor = claimed_card();
        assert_eq!(nor.page_program(0, &[]), Err(Error::EmptyWrite));
        assert_eq!(nor.page_program(255, &[0, 0]), Err(Error::PageBoundary));
        assert_eq!(nor.erase_sector_4k(1), Err(Error::InvalidAddress));
        let mut byte = [0];
        assert_eq!(
            nor.read(NOR_CAPACITY_BYTES, &mut byte),
            Err(Error::InvalidAddress)
        );
        assert!(!nor.read_status().unwrap().busy());
    }
}
