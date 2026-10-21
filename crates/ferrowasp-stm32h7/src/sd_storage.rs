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
//! never crosses a 256-byte page. The status register never reports busy.
//!
//! # Write-back window
//!
//! An SD card writes one block at a time slowly and many consecutive blocks
//! quickly, so programs at or above the address given to
//! [`SdNor::with_write_back_from`] land in a RAM window of `WINDOW_BLOCKS`
//! aligned blocks and reach the card in one multi-block write: when a program
//! or erase moves to another window, or on [`SdNor::flush`]. Until then a
//! power loss loses them, so the owner flushes when logging pauses or stops.
//! Everything below that address, the configuration, is written through
//! before the call returns. An erase inside the window writes its zeros to
//! the card before the sector map marks the sector erased, so a power loss
//! can never make stale contents read as erased-then-programmed data.

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
/// Blocks in the write-back window: 8 KiB, two sectors, one multi-block write.
pub const WINDOW_BLOCKS: usize = 16;

/// The identity `read_jedec_id` reports once a card is claimed: an unused
/// manufacturer code, so tools cannot mistake it for a real part.
pub const SD_NOR_MANUFACTURER: u8 = 0xfd;
pub const SD_NOR_MEMORY_TYPE: u8 = 0x5d;

const _: () = assert!(SECTOR_COUNT as usize == BLOCK_SIZE * 8);
// A sector lies wholly inside or outside a window.
const _: () = assert!(WINDOW_BLOCKS.is_multiple_of(BLOCKS_PER_SECTOR as usize));

/// 512-byte block storage addressed by block number.
pub trait BlockDevice {
    type Error;

    fn block_count(&self) -> u32;
    fn read_block(&mut self, lba: u32, block: &mut [u8; BLOCK_SIZE]) -> Result<(), Self::Error>;
    fn write_block(&mut self, lba: u32, block: &[u8; BLOCK_SIZE]) -> Result<(), Self::Error>;

    /// Consecutive blocks from `lba`; a card reads these in one command.
    fn read_blocks(
        &mut self,
        lba: u32,
        blocks: &mut [[u8; BLOCK_SIZE]],
    ) -> Result<(), Self::Error> {
        for (offset, block) in (0u32..).zip(blocks.iter_mut()) {
            self.read_block(lba + offset, block)?;
        }
        Ok(())
    }

    /// Consecutive blocks from `lba`; a card writes these in one command, far
    /// faster than block by block.
    fn write_blocks(&mut self, lba: u32, blocks: &[[u8; BLOCK_SIZE]]) -> Result<(), Self::Error> {
        for (offset, block) in (0u32..).zip(blocks) {
            self.write_block(lba + offset, block)?;
        }
        Ok(())
    }
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
    /// Emulated addresses from here up are written back through the window.
    write_back_from: u32,
    /// The first block the window holds, when it holds any.
    window_lba: Option<u32>,
    window: [[u8; BLOCK_SIZE]; WINDOW_BLOCKS],
    /// Window blocks `start..end` hold programs the card has not seen.
    window_dirty: Option<(usize, usize)>,
}

impl<D> SdNor<D>
where
    D: BlockDevice,
{
    /// Every operation reaches the card before it returns.
    pub const fn new(device: D) -> Self {
        Self::with_write_back_from(device, NOR_CAPACITY_BYTES)
    }

    /// Programs from `address` up go through the write-back window; see the
    /// module documentation.
    pub const fn with_write_back_from(device: D, address: u32) -> Self {
        Self {
            device,
            claimed: false,
            sector_map: [0; BLOCK_SIZE],
            cache: None,
            write_back_from: address,
            window_lba: None,
            window: [[0; BLOCK_SIZE]; WINDOW_BLOCKS],
            window_dirty: None,
        }
    }

    /// The card, without flushing: unflushed programs are lost, as in a
    /// power loss.
    pub fn free(self) -> D {
        self.device
    }

    /// Whether programs are waiting in RAM for [`Self::flush`].
    pub const fn write_back_pending(&self) -> bool {
        self.window_dirty.is_some()
    }

    /// Write the window's pending programs to the card in one command.
    pub fn flush(&mut self) -> Result<(), Error<D::Error>> {
        let (Some(first), Some((start, end))) = (self.window_lba, self.window_dirty) else {
            return Ok(());
        };
        self.device
            .write_blocks(first + start as u32, &self.window[start..end])?;
        self.window_dirty = None;
        Ok(())
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
        if address >= self.write_back_from {
            let index = self.open_window(lba)?;
            program_inverted(&mut self.window[index][offset..offset + bytes.len()], bytes);
            self.mark_dirty(index);
            return Ok(());
        }
        self.close_window_over(lba)?;
        let mut block = *self.load(lba)?;
        program_inverted(&mut block[offset..offset + bytes.len()], bytes);
        self.store(lba, &block)
    }

    pub fn erase_sector_4k(&mut self, address: u32) -> Result<(), Error<D::Error>> {
        self.claim()?;
        if !address.is_multiple_of(SECTOR_SIZE) {
            return Err(Error::InvalidAddress);
        }
        check_range(address, SECTOR_SIZE as usize)?;

        let first = data_lba(address);
        if let Some(index) = self.window_index(first) {
            // The zeros must be on the card before the map says erased.
            let end = index + BLOCKS_PER_SECTOR as usize;
            self.window[index..end].fill([0; BLOCK_SIZE]);
            self.mark_dirty(index);
            self.mark_dirty(end - 1);
            self.flush()?;
        } else {
            if self
                .cache
                .as_ref()
                .is_some_and(|cached| (first..first + BLOCKS_PER_SECTOR).contains(&cached.lba))
            {
                self.cache = None;
            }
            self.device
                .write_blocks(first, &[[0; BLOCK_SIZE]; BLOCKS_PER_SECTOR as usize])?;
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
        // Read first, so a card that never came up reports its own error.
        let mut header = [0; BLOCK_SIZE];
        self.device.read_block(HEADER_LBA, &mut header)?;
        if self.device.block_count() < REQUIRED_BLOCKS {
            return Err(Error::Refused(RefusalReason::TooSmall));
        }
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
        if let Some(index) = self.window_index(lba) {
            return Ok(&self.window[index]);
        }
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

    fn window_index(&self, lba: u32) -> Option<usize> {
        let first = self.window_lba?;
        (lba >= first && lba < first + WINDOW_BLOCKS as u32).then(|| (lba - first) as usize)
    }

    /// The window holding `lba`, loading it in one read after flushing the
    /// one it replaces.
    fn open_window(&mut self, lba: u32) -> Result<usize, Error<D::Error>> {
        if let Some(index) = self.window_index(lba) {
            return Ok(index);
        }
        self.flush()?;
        self.window_lba = None;
        let first = DATA_LBA + (lba - DATA_LBA) / WINDOW_BLOCKS as u32 * WINDOW_BLOCKS as u32;
        self.device.read_blocks(first, &mut self.window)?;
        self.window_lba = Some(first);
        if self
            .cache
            .as_ref()
            .is_some_and(|cached| self.window_index(cached.lba).is_some())
        {
            self.cache = None;
        }
        Ok((lba - first) as usize)
    }

    /// A write-through to a block the window holds: flush it and let go.
    fn close_window_over(&mut self, lba: u32) -> Result<(), Error<D::Error>> {
        if self.window_index(lba).is_some() {
            self.flush()?;
            self.window_lba = None;
        }
        Ok(())
    }

    fn mark_dirty(&mut self, index: usize) {
        self.window_dirty = Some(match self.window_dirty {
            Some((start, end)) => (start.min(index), end.max(index + 1)),
            None => (index, index + 1),
        });
    }

    /// Write through the one-block cache.
    fn store(&mut self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), Error<D::Error>> {
        self.cache = None;
        self.device.write_block(lba, data)?;
        self.cache = Some(CachedBlock { lba, data: *data });
        Ok(())
    }
}

/// The flash manager's view: never busy, since every operation completes
/// before it returns, and log programs held in the write-back window until
/// a flush.
impl<D> ferrowasp_storage::nor_storage::NorStorage for SdNor<D>
where
    D: BlockDevice,
{
    type Error = Error<D::Error>;

    fn read(&mut self, address: u32, output: &mut [u8]) -> Result<(), Self::Error> {
        SdNor::read(self, address, output)
    }

    fn page_program(&mut self, address: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        SdNor::page_program(self, address, bytes)
    }

    fn erase_sector_4k(&mut self, address: u32) -> Result<(), Self::Error> {
        SdNor::erase_sector_4k(self, address)
    }

    fn busy(&mut self) -> Result<bool, Self::Error> {
        Ok(SdNor::read_status(self)?.busy())
    }

    fn write_back_pending(&self) -> bool {
        SdNor::write_back_pending(self)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        SdNor::flush(self)
    }
}

/// A card slot whose card may have failed to come up. The flash manager
/// owns the slot either way; a failed card answers every access with the
/// error it failed with.
pub enum CardSlot<D, E> {
    Ready(D),
    Failed(E),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardSlotError<D, E> {
    Device(D),
    NotReady(E),
}

impl<D, E> BlockDevice for CardSlot<D, E>
where
    D: BlockDevice,
    E: Copy,
{
    type Error = CardSlotError<D::Error, E>;

    fn block_count(&self) -> u32 {
        match self {
            Self::Ready(device) => device.block_count(),
            Self::Failed(_) => 0,
        }
    }

    fn read_block(&mut self, lba: u32, block: &mut [u8; BLOCK_SIZE]) -> Result<(), Self::Error> {
        match self {
            Self::Ready(device) => device.read_block(lba, block).map_err(CardSlotError::Device),
            Self::Failed(error) => Err(CardSlotError::NotReady(*error)),
        }
    }

    fn write_block(&mut self, lba: u32, block: &[u8; BLOCK_SIZE]) -> Result<(), Self::Error> {
        match self {
            Self::Ready(device) => device
                .write_block(lba, block)
                .map_err(CardSlotError::Device),
            Self::Failed(error) => Err(CardSlotError::NotReady(*error)),
        }
    }

    fn read_blocks(
        &mut self,
        lba: u32,
        blocks: &mut [[u8; BLOCK_SIZE]],
    ) -> Result<(), Self::Error> {
        match self {
            Self::Ready(device) => device
                .read_blocks(lba, blocks)
                .map_err(CardSlotError::Device),
            Self::Failed(error) => Err(CardSlotError::NotReady(*error)),
        }
    }

    fn write_blocks(&mut self, lba: u32, blocks: &[[u8; BLOCK_SIZE]]) -> Result<(), Self::Error> {
        match self {
            Self::Ready(device) => device
                .write_blocks(lba, blocks)
                .map_err(CardSlotError::Device),
            Self::Failed(error) => Err(CardSlotError::NotReady(*error)),
        }
    }
}

fn check_range<E>(address: u32, len: usize) -> Result<(), Error<E>> {
    let end = u64::from(address) + len as u64;
    if end > u64::from(NOR_CAPACITY_BYTES) {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

/// Stored inverted, so a program is `!(old & new) == stored | !new`.
fn program_inverted(stored: &mut [u8], bytes: &[u8]) {
    for (stored, byte) in stored.iter_mut().zip(bytes) {
        *stored |= !byte;
    }
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

        fn read_blocks(
            &mut self,
            lba: u32,
            blocks: &mut [[u8; BLOCK_SIZE]],
        ) -> Result<(), sdmmc::Error> {
            match blocks {
                [] => Ok(()),
                [block] => self.sdmmc.read_block(lba, block),
                _ => self.sdmmc.read_blocks(lba, blocks.as_flattened_mut()),
            }
        }

        fn write_blocks(
            &mut self,
            lba: u32,
            blocks: &[[u8; BLOCK_SIZE]],
        ) -> Result<(), sdmmc::Error> {
            match blocks {
                [] => Ok(()),
                [block] => self.sdmmc.write_block(lba, block),
                _ => self.sdmmc.write_blocks(lba, blocks.as_flattened()),
            }
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
        /// Blocks written, however many per command.
        writes: usize,
        /// Write commands: one per `write_block` or `write_blocks` call.
        write_commands: usize,
    }

    impl MemoryCard {
        fn new(block_count: u32) -> Self {
            Self {
                blocks: vec![[0; BLOCK_SIZE]; block_count as usize],
                writes: 0,
                write_commands: 0,
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
            self.write_commands += 1;
            Ok(())
        }

        fn write_blocks(&mut self, lba: u32, blocks: &[[u8; BLOCK_SIZE]]) -> Result<(), ()> {
            for (offset, block) in (0u32..).zip(blocks) {
                *self.blocks.get_mut((lba + offset) as usize).ok_or(())? = *block;
            }
            self.writes += blocks.len();
            self.write_commands += 1;
            Ok(())
        }
    }

    /// The configuration below this address writes through; logs above it
    /// write back, as on the Lucid.
    const LOG_START: u32 = 3 * SECTOR_SIZE;

    /// A claimed card with every log sector already erased, as after a log
    /// erase, so tests see only the cost of programming.
    fn erased_write_back_card() -> SdNor<MemoryCard> {
        let mut nor = SdNor::with_write_back_from(MemoryCard::new(REQUIRED_BLOCKS), LOG_START);
        nor.read_jedec_id().unwrap();
        for sector in 0..8 {
            nor.erase_sector_4k(LOG_START + sector * SECTOR_SIZE)
                .unwrap();
        }
        nor
    }

    fn reclaim(nor: SdNor<MemoryCard>) -> SdNor<MemoryCard> {
        let mut nor = SdNor::with_write_back_from(nor.free(), LOG_START);
        nor.read_jedec_id().unwrap();
        nor
    }

    fn claimed_card() -> SdNor<MemoryCard> {
        let mut nor = SdNor::new(MemoryCard::new(REQUIRED_BLOCKS));
        let id = nor.read_jedec_id().unwrap();
        assert!(id.plausible());
        assert_eq!(id.capacity_bytes(), Some(NOR_CAPACITY_BYTES));
        nor
    }

    #[test]
    fn a_log_program_waits_in_ram_until_flushed_yet_reads_back_at_once() {
        let mut nor = erased_write_back_card();
        let writes = nor.device.writes;
        nor.page_program(LOG_START, &[0x12, 0x34]).unwrap();

        assert!(nor.write_back_pending());
        assert_eq!(nor.device.writes, writes);
        let mut bytes = [0; 3];
        nor.read(LOG_START, &mut bytes).unwrap();
        assert_eq!(bytes, [0x12, 0x34, 0xff]);

        nor.flush().unwrap();
        assert!(!nor.write_back_pending());
        let mut nor = reclaim(nor);
        nor.read(LOG_START, &mut bytes).unwrap();
        assert_eq!(bytes, [0x12, 0x34, 0xff]);
    }

    #[test]
    fn an_unflushed_program_is_lost_like_a_power_cut() {
        let mut nor = erased_write_back_card();
        nor.page_program(LOG_START, &[0x00]).unwrap();

        let mut nor = reclaim(nor);
        let mut byte = [0];
        nor.read(LOG_START, &mut byte).unwrap();
        assert_eq!(byte, [0xff]);
    }

    #[test]
    fn a_window_of_log_pages_reaches_the_card_in_one_command() {
        let mut nor = erased_write_back_card();
        let base = nor.device.write_commands;
        let pages_per_window = (WINDOW_BLOCKS * BLOCK_SIZE / PAGE_SIZE) as u32;
        // The log starts mid-window on this layout; fill from the next one.
        let window_start = LOG_START.next_multiple_of((WINDOW_BLOCKS * BLOCK_SIZE) as u32);
        for page in 0..=pages_per_window {
            nor.page_program(
                window_start + page * PAGE_SIZE as u32,
                &[page as u8; PAGE_SIZE],
            )
            .unwrap();
        }

        // The first window flushed as one command when the next page opened
        // the next window; that page still waits.
        assert_eq!(nor.device.write_commands - base, 1);
        assert!(nor.write_back_pending());
        nor.flush().unwrap();
        let mut nor = reclaim(nor);
        for page in 0..=pages_per_window {
            let mut bytes = [0; PAGE_SIZE];
            nor.read(window_start + page * PAGE_SIZE as u32, &mut bytes)
                .unwrap();
            assert_eq!(bytes, [page as u8; PAGE_SIZE]);
        }
    }

    #[test]
    fn configuration_below_the_log_writes_through() {
        let mut nor = erased_write_back_card();
        nor.erase_sector_4k(0).unwrap();
        nor.page_program(0, &[0x5a]).unwrap();

        assert!(!nor.write_back_pending());
        let mut nor = reclaim(nor);
        let mut byte = [0];
        nor.read(0, &mut byte).unwrap();
        assert_eq!(byte, [0x5a]);
    }

    #[test]
    fn programs_still_only_clear_bits_across_a_flush() {
        let mut nor = erased_write_back_card();
        nor.page_program(LOG_START + 4, &[0xf0, 0x0f]).unwrap();
        nor.flush().unwrap();
        nor.page_program(LOG_START + 4, &[0x3c, 0xff]).unwrap();
        nor.flush().unwrap();

        let mut nor = reclaim(nor);
        let mut bytes = [0; 2];
        nor.read(LOG_START + 4, &mut bytes).unwrap();
        assert_eq!(bytes, [0x30, 0x0f]);
    }

    #[test]
    fn erasing_inside_the_window_writes_zeros_before_marking_the_sector() {
        let mut nor = erased_write_back_card();
        nor.page_program(LOG_START, &[0x00; 4]).unwrap();
        nor.flush().unwrap();
        nor.page_program(LOG_START + 8, &[0x00]).unwrap();
        nor.erase_sector_4k(LOG_START).unwrap();

        // Erased on the card itself, not only in RAM.
        let card = nor.free();
        let first = data_lba(LOG_START) as usize;
        assert!(
            card.blocks[first..first + BLOCKS_PER_SECTOR as usize]
                .iter()
                .all(|block| *block == [0; BLOCK_SIZE])
        );
    }

    #[test]
    fn a_write_through_into_the_window_flushes_it_first() {
        // Configuration from the start of the card, logs from one sector in:
        // both share the first window.
        let mut nor = SdNor::with_write_back_from(MemoryCard::new(REQUIRED_BLOCKS), SECTOR_SIZE);
        nor.read_jedec_id().unwrap();
        nor.erase_sector_4k(0).unwrap();
        nor.erase_sector_4k(SECTOR_SIZE).unwrap();
        nor.page_program(SECTOR_SIZE, &[0x11]).unwrap();
        assert!(nor.write_back_pending());

        nor.page_program(0, &[0x22]).unwrap();

        assert!(!nor.write_back_pending());
        let mut nor = SdNor::new(nor.free());
        nor.read_jedec_id().unwrap();
        let mut bytes = [0];
        nor.read(SECTOR_SIZE, &mut bytes).unwrap();
        assert_eq!(bytes, [0x11]);
        nor.read(0, &mut bytes).unwrap();
        assert_eq!(bytes, [0x22]);
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

    #[test]
    fn failed_card_reports_its_init_error() {
        let mut flash = SdNor::new(CardSlot::<MemoryCard, u8>::Failed(7));
        assert_eq!(
            flash.read_jedec_id(),
            Err(Error::Device(CardSlotError::NotReady(7)))
        );
    }
}
