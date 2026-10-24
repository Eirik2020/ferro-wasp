//! Where everything lives on the onboard storage: the two configuration
//! slots, the scratch sector, the flight log and the sync ledger, and the
//! choice of the newest valid configuration slot at boot.

use ferrowasp_config::page::FLASH_PAGE_LEN;
use ferrowasp_config::stored::StoredConfig;

pub const CONFIG_SLOT_COUNT: u32 = 2;

pub const CONFIG_SECTOR_SIZE: u32 = 4096;

/// Dedicated destructive-test sector. It is never used for configuration or logs.
pub const SCRATCH_SECTOR_ADDRESS: u32 = CONFIG_SLOT_COUNT * CONFIG_SECTOR_SIZE;

pub const LOG_START_ADDRESS: u32 = SCRATCH_SECTOR_ADDRESS + CONFIG_SECTOR_SIZE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageReadError<E> {
    InvalidLayout,
    Device(E),
}

/// Loads the newest decodable copy-on-write configuration slot: what a save
/// left, as the save tests read it back. The boot's choice, which also
/// validates every value and reports a fallback, is `ferrowasp_config::boot`.
pub fn load_config<E, Read>(
    layout: StorageLayout,
    default: StoredConfig,
    mut read: Read,
) -> Result<(StoredConfig, u32, u8), StorageReadError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    use ferrowasp_config::page::decode_config_page;

    let mut selected: Option<(StoredConfig, u32, u8)> = None;
    for slot in 0..2u8 {
        let mut page = [0xff; FLASH_PAGE_LEN];
        read(layout.config_slot_addresses[slot as usize], &mut page)
            .map_err(StorageReadError::Device)?;
        let Ok((sequence, payload)) = decode_config_page(&page) else {
            continue;
        };
        let Some(config) = StoredConfig::decode(payload) else {
            continue;
        };
        if selected
            .as_ref()
            .is_none_or(|(_, current, _)| sequence_is_newer(sequence, *current))
        {
            selected = Some((config, sequence, slot));
        }
    }
    Ok(selected.unwrap_or((default, 0, 1)))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageLayout {
    pub capacity_bytes: u32,
    pub config_slot_addresses: [u32; 2],
    pub log_start_address: u32,
    pub log_page_count: u32,
}

impl StorageLayout {
    pub const fn new(capacity_bytes: u32) -> Option<Self> {
        if capacity_bytes <= LOG_START_ADDRESS + CONFIG_SECTOR_SIZE
            || capacity_bytes > 0x0100_0000
            || !capacity_bytes.is_multiple_of(CONFIG_SECTOR_SIZE)
        {
            return None;
        }
        Some(Self {
            capacity_bytes,
            config_slot_addresses: [0, CONFIG_SECTOR_SIZE],
            log_start_address: LOG_START_ADDRESS,
            // The last sector holds the sync ledger, not log pages.
            log_page_count: (capacity_bytes - LOG_START_ADDRESS - CONFIG_SECTOR_SIZE)
                / FLASH_PAGE_LEN as u32,
        })
    }

    pub const fn log_page_address(self, page_index: u32) -> Option<u32> {
        if page_index >= self.log_page_count {
            None
        } else {
            Some(self.log_start_address + page_index * FLASH_PAGE_LEN as u32)
        }
    }

    /// Sectors a log erase clears: every log sector plus the sync ledger.
    pub const fn log_sector_count(self) -> u32 {
        (self.capacity_bytes - self.log_start_address) / CONFIG_SECTOR_SIZE
    }

    /// The last sector of the flash, which records synced flights.
    pub const fn sync_ledger_address(self) -> u32 {
        self.capacity_bytes - CONFIG_SECTOR_SIZE
    }

    /// The sector a log erase clears at `step`, from the end of the flash down.
    ///
    /// The ledger goes first so that a power cut part-way through leaves old
    /// flights looking unsynced. Erasing upward would leave the old ledger
    /// marking the next flights, whose identifiers restart at 1, as synced.
    pub const fn log_erase_sector_address(self, step: u32) -> Option<u32> {
        if step >= self.log_sector_count() {
            None
        } else {
            Some(self.capacity_bytes - (step + 1) * CONFIG_SECTOR_SIZE)
        }
    }
}

/// Sequence comparison for two copy-on-write configuration slots.
pub const fn sequence_is_newer(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < 0x8000_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_megabyte_layout_reserves_config_and_scratch_sectors() {
        let layout = StorageLayout::new(16 * 1024 * 1024).unwrap();
        assert_eq!(layout.config_slot_addresses, [0, 4096]);
        assert_eq!(SCRATCH_SECTOR_ADDRESS, 8192);
        assert_eq!(layout.log_start_address, 12288);
        assert_eq!(layout.log_page_address(0), Some(12288));
        assert_eq!(
            layout.log_page_address(layout.log_page_count - 1),
            Some(16 * 1024 * 1024 - 4096 - 256)
        );
        assert_eq!(layout.log_page_address(layout.log_page_count), None);
        assert_eq!(layout.sync_ledger_address(), 16 * 1024 * 1024 - 4096);
    }

    #[test]
    fn configuration_sequence_comparison_handles_wrap() {
        assert!(sequence_is_newer(11, 10));
        assert!(!sequence_is_newer(10, 10));
        assert!(sequence_is_newer(1, u32::MAX));
        assert!(!sequence_is_newer(u32::MAX, 1));
    }
}
