//! Choosing the configuration a board boots with (design §14): read each
//! slot, keep the newest one that decodes and validates, and say when the
//! choice fell back, so the safety master refuses arming until the pilot
//! acknowledges it. Never a clamped value: a value that fails validation
//! rejects its whole slot.
//!
//! Requirements: CONFIG-013, CONFIG-014, CONFIG-025
//! Safety-critical.

use crate::owner::validate;
use crate::page::{FLASH_PAGE_LEN, decode_config_page};
use crate::stored::StoredConfig;

/// What one slot holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotRead {
    /// Every byte erased: never written.
    Erased,
    /// Written, but the page is not whole: a cut save, or another
    /// firmware's data.
    Damaged,
    /// The page is whole, but its payload is refused: a value outside its
    /// range, or a schema this firmware cannot read.
    Invalid,
    Valid {
        sequence: u32,
        config: StoredConfig,
    },
}

/// Read one slot's page.
pub fn read_slot(page: &[u8; FLASH_PAGE_LEN]) -> SlotRead {
    if page.iter().all(|byte| *byte == 0xff) {
        return SlotRead::Erased;
    }
    let Ok((sequence, payload)) = decode_config_page(page) else {
        return SlotRead::Damaged;
    };
    match StoredConfig::decode(payload) {
        None => SlotRead::Invalid,
        Some(config) if !validate(&config) => SlotRead::Invalid,
        Some(config) => SlotRead::Valid { sequence, config },
    }
}

/// Why the board did not boot with what it last saved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fallback {
    /// A slot was written but its page is not whole.
    Damaged,
    /// A slot's page is whole but its payload is refused.
    Invalid,
    /// The store could not be read at all.
    Unreadable,
}

/// The configuration a board boots with, and where it came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BootConfig {
    pub config: StoredConfig,
    /// The chosen slot's sequence, 0 for the board default.
    pub sequence: u32,
    /// The chosen slot; the next save writes the other. 1 for the default,
    /// so the first save writes slot 0.
    pub slot: u8,
    /// `Some` when arming must wait for the pilot's acknowledgement.
    pub fallback: Option<Fallback>,
}

impl BootConfig {
    /// The board default, as a store that cannot be read gives it.
    pub const fn unreadable(default: StoredConfig) -> Self {
        Self {
            config: default,
            sequence: 0,
            slot: 1,
            fallback: Some(Fallback::Unreadable),
        }
    }
}

/// The newest valid slot, or `default` when there is none. Any slot that
/// was written and is not valid is a fallback: the pilot's last save is not
/// what the board flies.
pub fn select(slots: [SlotRead; 2], default: StoredConfig) -> BootConfig {
    let mut chosen: Option<(StoredConfig, u32, u8)> = None;
    let mut fallback = None;
    for (slot, read) in slots.into_iter().enumerate() {
        match read {
            SlotRead::Erased => {}
            SlotRead::Damaged => fallback = Some(fallback.unwrap_or(Fallback::Damaged)),
            SlotRead::Invalid => fallback = Some(Fallback::Invalid),
            SlotRead::Valid { sequence, config } => {
                if chosen
                    .as_ref()
                    .is_none_or(|(_, current, _)| sequence_is_newer(sequence, *current))
                {
                    chosen = Some((config, sequence, slot as u8));
                }
            }
        }
    }
    let (config, sequence, slot) = chosen.unwrap_or((default, 0, 1));
    BootConfig {
        config,
        sequence,
        slot,
        fallback,
    }
}

/// Whether `candidate` follows `current` in the wrapping slot sequence.
pub const fn sequence_is_newer(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < 0x8000_0000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::encode_config_page;

    fn page_of(sequence: u32, config: StoredConfig) -> [u8; FLASH_PAGE_LEN] {
        encode_config_page(sequence, &config.encode()).unwrap()
    }

    fn tuned(p: f32) -> StoredConfig {
        let mut config = StoredConfig::first_hop_default();
        config.tuning.rate_gains.roll.p = p;
        config
    }

    #[test]
    fn a_slot_reads_as_erased_damaged_invalid_or_valid() {
        assert_eq!(read_slot(&[0xff; FLASH_PAGE_LEN]), SlotRead::Erased);
        let mut torn = page_of(3, tuned(0.05));
        torn[20] ^= 1;
        assert_eq!(read_slot(&torn), SlotRead::Damaged);
        assert_eq!(read_slot(&[0x00; FLASH_PAGE_LEN]), SlotRead::Damaged);
        let mut out_of_range = tuned(0.05);
        out_of_range.tuning.rate_gains.roll.p = 1.0e6;
        assert_eq!(
            read_slot(&page_of(4, out_of_range)),
            SlotRead::Invalid,
            "a value outside its range rejects the slot, never clamped"
        );
        assert_eq!(
            read_slot(&page_of(5, tuned(0.05))),
            SlotRead::Valid {
                sequence: 5,
                config: tuned(0.05)
            }
        );
    }

    #[test]
    fn a_fresh_board_boots_its_default_and_may_arm() {
        let boot = select([SlotRead::Erased, SlotRead::Erased], tuned(0.01));
        assert_eq!(boot.config, tuned(0.01));
        assert_eq!((boot.sequence, boot.slot, boot.fallback), (0, 1, None));
    }

    #[test]
    fn the_newest_valid_slot_wins_across_the_sequence_wrap() {
        let older = SlotRead::Valid {
            sequence: u32::MAX,
            config: tuned(0.02),
        };
        let newer = SlotRead::Valid {
            sequence: 0,
            config: tuned(0.03),
        };
        let boot = select([newer, older], tuned(0.01));
        assert_eq!(
            (boot.config, boot.slot, boot.fallback),
            (tuned(0.03), 0, None)
        );
    }

    #[test]
    fn a_damaged_or_invalid_slot_falls_back_and_must_be_acknowledged() {
        let valid = SlotRead::Valid {
            sequence: 7,
            config: tuned(0.02),
        };
        let boot = select([SlotRead::Damaged, valid], tuned(0.01));
        assert_eq!(boot.config, tuned(0.02), "the pilot's previous save");
        assert_eq!(boot.fallback, Some(Fallback::Damaged));

        let boot = select([valid, SlotRead::Invalid], tuned(0.01));
        assert_eq!(boot.config, tuned(0.02));
        assert_eq!(boot.fallback, Some(Fallback::Invalid));

        let boot = select([SlotRead::Invalid, SlotRead::Damaged], tuned(0.01));
        assert_eq!(boot.config, tuned(0.01), "then the default");
        assert_eq!(boot.fallback, Some(Fallback::Invalid));
    }

    #[test]
    fn an_unreadable_store_boots_the_default_and_must_be_acknowledged() {
        let boot = BootConfig::unreadable(tuned(0.01));
        assert_eq!(boot.config, tuned(0.01));
        assert_eq!(boot.fallback, Some(Fallback::Unreadable));
    }
}
