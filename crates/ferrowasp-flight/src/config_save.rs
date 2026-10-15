//! Saving the configuration so that no failure loses the one that boots.
//!
//! The configuration lives in two flash slots; startup takes the valid one
//! with the newer sequence (`flash_storage::load_config`). A save writes the
//! slot not in use, under the next sequence: erase it, program the page, read
//! it back. Only a page that reads back exactly as written is adopted. A
//! failure or a power loss at any step leaves the active slot, and so the
//! configuration the board boots with, untouched.
//!
//! Each step runs one flash operation, so the storage owner interleaves its
//! other work and can stop a save between steps.

use ferrowasp_core::blackbox::{FLASH_PAGE_LEN, decode_config_page, encode_config_page};
use ferrowasp_drivers::nor_storage::NorStorage;

use crate::flash_storage::{StorageLayout, StoredConfig};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Idle,
    Erase,
    Program,
    Verify,
}

/// Where a save stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveFailure {
    /// The configuration did not fit a page; nothing was written.
    Encoding,
    Erase,
    Program,
    /// The page read back differently from what was written.
    Verify,
}

/// What one step did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SaveStep {
    /// No save is running.
    Idle,
    /// One flash operation ran; step again on a later pass.
    Running,
    /// The configuration is on flash and verified: adopt it.
    Saved {
        config: StoredConfig,
        slot: u8,
        sequence: u32,
    },
    /// The save stopped; the active slot is untouched.
    Failed(SaveFailure),
}

/// One configuration save in progress, or none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfigSave {
    phase: Phase,
    slot: u8,
    sequence: u32,
    candidate: StoredConfig,
    page: [u8; FLASH_PAGE_LEN],
}

impl Default for ConfigSave {
    fn default() -> Self {
        Self::idle()
    }
}

impl ConfigSave {
    pub const fn idle() -> Self {
        Self {
            phase: Phase::Idle,
            slot: 0,
            sequence: 0,
            candidate: StoredConfig::first_hop_default(),
            page: [0xff; FLASH_PAGE_LEN],
        }
    }

    pub const fn is_active(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Starts saving `config` after the active one, `active_sequence` in
    /// slot `active_slot`. Nothing touches flash until the first step.
    pub fn begin(
        &mut self,
        config: StoredConfig,
        active_sequence: u32,
        active_slot: u8,
    ) -> Result<(), SaveFailure> {
        let sequence = active_sequence.wrapping_add(1);
        let page =
            encode_config_page(sequence, &config.encode()).map_err(|_| SaveFailure::Encoding)?;
        *self = Self {
            phase: Phase::Erase,
            slot: 1 - active_slot,
            sequence,
            candidate: config,
            page,
        };
        Ok(())
    }

    /// Stops a save between operations, as when the craft arms. The slot
    /// being written is abandoned; the active slot is untouched.
    pub fn cancel(&mut self) {
        self.phase = Phase::Idle;
    }

    /// Runs the next flash operation.
    pub fn step<F: NorStorage>(&mut self, flash: &mut F, layout: StorageLayout) -> SaveStep {
        let address = layout.config_slot_addresses[usize::from(self.slot)];
        match self.phase {
            Phase::Idle => SaveStep::Idle,
            Phase::Erase => self.advance(
                flash.erase_sector_4k(address).is_ok(),
                Phase::Program,
                SaveFailure::Erase,
            ),
            Phase::Program => self.advance(
                flash.page_program(address, &self.page).is_ok(),
                Phase::Verify,
                SaveFailure::Program,
            ),
            Phase::Verify => {
                let mut persisted = [0xff; FLASH_PAGE_LEN];
                let expected = self.candidate.encode();
                let verified = flash.read(address, &mut persisted).is_ok()
                    && matches!(
                        decode_config_page(&persisted),
                        Ok((sequence, payload))
                            if sequence == self.sequence && payload == expected.as_slice()
                    );
                self.phase = Phase::Idle;
                if verified {
                    SaveStep::Saved {
                        config: self.candidate,
                        slot: self.slot,
                        sequence: self.sequence,
                    }
                } else {
                    SaveStep::Failed(SaveFailure::Verify)
                }
            }
        }
    }

    fn advance(&mut self, done: bool, next: Phase, failure: SaveFailure) -> SaveStep {
        if done {
            self.phase = next;
            SaveStep::Running
        } else {
            self.phase = Phase::Idle;
            SaveStep::Failed(failure)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flash_storage::load_config;
    use crate::test_nor::MemoryNor;
    use ferrowasp_core::config::ConfigKey;

    const CAPACITY: u32 = 64 * 1024;

    fn layout() -> StorageLayout {
        StorageLayout::new(CAPACITY).unwrap()
    }

    fn boots_with(nor: &mut MemoryNor) -> (StoredConfig, u32, u8) {
        load_config(
            layout(),
            StoredConfig::first_hop_default(),
            |address, page| nor.read(address, page),
        )
        .unwrap()
    }

    fn tuned(roll_p: f32) -> StoredConfig {
        let mut config = StoredConfig::first_hop_default();
        assert!(config.set(ConfigKey::RollP, roll_p));
        config
    }

    /// Steps until the save ends, counting the flash operations.
    fn run(save: &mut ConfigSave, nor: &mut MemoryNor) -> (SaveStep, u32) {
        let mut operations = 0;
        loop {
            let step = save.step(nor, layout());
            operations += 1;
            if step != SaveStep::Running {
                return (step, operations);
            }
        }
    }

    fn saved_once(nor: &mut MemoryNor, config: StoredConfig) -> (u32, u8) {
        let (_, sequence, slot) = boots_with(nor);
        let mut save = ConfigSave::idle();
        save.begin(config, sequence, slot).unwrap();
        match run(&mut save, nor).0 {
            SaveStep::Saved { sequence, slot, .. } => (sequence, slot),
            other => panic!("save failed: {other:?}"),
        }
    }

    #[test]
    fn a_save_writes_the_other_slot_and_boots_with_it_after_verifying() {
        let mut nor = MemoryNor::erased(CAPACITY);
        let (first_sequence, first_slot) = saved_once(&mut nor, tuned(3.0));
        let (sequence, slot) = saved_once(&mut nor, tuned(4.0));
        assert_eq!(sequence, first_sequence + 1);
        assert_ne!(slot, first_slot);
        let (config, booted_sequence, booted_slot) = boots_with(&mut nor);
        assert_eq!(config, tuned(4.0));
        assert_eq!((booted_sequence, booted_slot), (sequence, slot));
    }

    #[test]
    fn one_flash_operation_per_step_and_idle_after() {
        let mut nor = MemoryNor::erased(CAPACITY);
        let mut save = ConfigSave::idle();
        assert_eq!(save.step(&mut nor, layout()), SaveStep::Idle);
        save.begin(tuned(3.0), 0, 1).unwrap();
        assert!(save.is_active());
        let (step, operations) = run(&mut save, &mut nor);
        assert!(matches!(step, SaveStep::Saved { .. }));
        assert_eq!(operations, 3, "erase, program, verify");
        assert!(!save.is_active());
    }

    #[test]
    fn every_failure_leaves_the_booting_configuration_untouched() {
        type Breaks = fn(&mut MemoryNor);
        let breaks: [(Breaks, SaveFailure); 3] = [
            (|nor| nor.fail_erase = true, SaveFailure::Erase),
            (|nor| nor.fail_program = true, SaveFailure::Program),
            (|nor| nor.stuck_first_byte = Some(0x00), SaveFailure::Verify),
        ];
        for (fault, failure) in breaks {
            let mut nor = MemoryNor::erased(CAPACITY);
            saved_once(&mut nor, tuned(3.0));
            let before = boots_with(&mut nor);
            fault(&mut nor);
            let mut save = ConfigSave::idle();
            save.begin(tuned(4.0), before.1, before.2).unwrap();
            assert_eq!(run(&mut save, &mut nor).0, SaveStep::Failed(failure));
            assert!(!save.is_active());
            assert_eq!(boots_with(&mut nor), before, "{failure:?}");
        }
    }

    #[test]
    fn a_cancelled_save_stops_and_the_active_slot_still_boots() {
        let mut nor = MemoryNor::erased(CAPACITY);
        saved_once(&mut nor, tuned(3.0));
        let before = boots_with(&mut nor);
        let mut save = ConfigSave::idle();
        save.begin(tuned(4.0), before.1, before.2).unwrap();
        assert_eq!(save.step(&mut nor, layout()), SaveStep::Running);
        save.cancel();
        assert_eq!(save.step(&mut nor, layout()), SaveStep::Idle);
        assert_eq!(boots_with(&mut nor), before);
    }
}
