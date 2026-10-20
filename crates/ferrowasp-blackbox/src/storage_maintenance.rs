//! The storage owner's long operations, one flash operation per step: erasing
//! the log, writing a host's acknowledgement into the sync ledger, and
//! testing the scratch sector. Only one runs at a time; the owner refuses to
//! start another, or a configuration save, while one is running, and stops it
//! between steps if the craft arms.

use ferrowasp_protocol::blackbox::FLASH_PAGE_LEN;
use ferrowasp_storage::nor_storage::NorStorage;

use crate::records::scratch_test_page;
use crate::sync_ledger::{self, MarkStep};
use ferrowasp_storage::layout::SCRATCH_SECTOR_ADDRESS;
use ferrowasp_storage::layout::StorageLayout;

/// Ledger operations one acknowledgement may take: erase, header and bit.
pub const MAX_SYNC_MARK_STEPS: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScratchPhase {
    Erase,
    Program,
    Verify,
}

/// The operation running, or none.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Maintenance {
    #[default]
    Idle,
    /// Erasing the log region top down, sync ledger first
    /// (`StorageLayout::log_erase_sector_address`); `step` sectors done.
    EraseLog {
        step: u32,
    },
    /// Acknowledging `flight` in the sync ledger, `steps` operations in.
    SyncMark {
        flight: u32,
        steps: u8,
    },
    ScratchTest(ScratchPhase),
}

/// What an operation achieved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceDone {
    /// The log region is blank: the next page is 0 and the next flight 1.
    LogErased,
    Synced {
        flight: u32,
    },
    ScratchVerified,
}

/// Where an operation stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceFailure {
    LogErase,
    /// The ledger could not be read or written, or took too many steps.
    /// `write_fault` when a flash write itself failed.
    SyncLedger {
        write_fault: bool,
    },
    ScratchErase,
    ScratchProgram,
    ScratchReadback,
}

impl MaintenanceFailure {
    /// Whether this failure counts as a flash write fault.
    pub const fn is_write_fault(self) -> bool {
        match self {
            Self::SyncLedger { write_fault } => write_fault,
            _ => true,
        }
    }
}

/// What one step did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceStep {
    Idle,
    /// Step again on a later pass.
    Running,
    Done(MaintenanceDone),
    Failed(MaintenanceFailure),
}

impl Maintenance {
    pub const fn erase_log() -> Self {
        Self::EraseLog { step: 0 }
    }

    pub const fn sync_mark(flight: u32) -> Self {
        Self::SyncMark { flight, steps: 0 }
    }

    pub const fn scratch_test() -> Self {
        Self::ScratchTest(ScratchPhase::Erase)
    }

    pub const fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// Whether a log erase is under way: one stopped part-way has cleared the
    /// top of the log, so the owner must find the append point again.
    pub const fn is_erasing_log(&self) -> bool {
        matches!(self, Self::EraseLog { .. })
    }

    /// Stops between operations.
    pub fn cancel(&mut self) {
        *self = Self::Idle;
    }

    /// Runs the next operation.
    pub fn step<F: NorStorage>(&mut self, flash: &mut F, layout: StorageLayout) -> MaintenanceStep {
        let outcome = match *self {
            Self::Idle => return MaintenanceStep::Idle,
            Self::EraseLog { step } => match layout.log_erase_sector_address(step) {
                None => Err(Ok(MaintenanceDone::LogErased)),
                Some(address) if flash.erase_sector_4k(address).is_err() => {
                    Err(Err(MaintenanceFailure::LogErase))
                }
                Some(_) => Ok(Self::EraseLog { step: step + 1 }),
            },
            Self::SyncMark { flight, steps } => {
                let mut read = |address, output: &mut [u8]| flash.read(address, output);
                match sync_ledger::next_mark_step(layout, &mut read, flight) {
                    Ok(MarkStep::Done) => Err(Ok(MaintenanceDone::Synced { flight })),
                    Err(_) => Err(Err(MaintenanceFailure::SyncLedger { write_fault: false })),
                    Ok(_) if steps >= MAX_SYNC_MARK_STEPS => {
                        Err(Err(MaintenanceFailure::SyncLedger { write_fault: false }))
                    }
                    Ok(step) if start_mark_step(flash, step).is_err() => {
                        Err(Err(MaintenanceFailure::SyncLedger { write_fault: true }))
                    }
                    Ok(_) => Ok(Self::SyncMark {
                        flight,
                        steps: steps + 1,
                    }),
                }
            }
            Self::ScratchTest(ScratchPhase::Erase) => {
                match flash.erase_sector_4k(SCRATCH_SECTOR_ADDRESS) {
                    Ok(()) => Ok(Self::ScratchTest(ScratchPhase::Program)),
                    Err(_) => Err(Err(MaintenanceFailure::ScratchErase)),
                }
            }
            Self::ScratchTest(ScratchPhase::Program) => {
                match flash.page_program(SCRATCH_SECTOR_ADDRESS, &scratch_test_page()) {
                    Ok(()) => Ok(Self::ScratchTest(ScratchPhase::Verify)),
                    Err(_) => Err(Err(MaintenanceFailure::ScratchProgram)),
                }
            }
            Self::ScratchTest(ScratchPhase::Verify) => {
                let mut actual = [0u8; FLASH_PAGE_LEN];
                if flash.read(SCRATCH_SECTOR_ADDRESS, &mut actual).is_ok()
                    && actual == scratch_test_page()
                {
                    Err(Ok(MaintenanceDone::ScratchVerified))
                } else {
                    Err(Err(MaintenanceFailure::ScratchReadback))
                }
            }
        };
        match outcome {
            Ok(next) => {
                *self = next;
                MaintenanceStep::Running
            }
            Err(end) => {
                *self = Self::Idle;
                match end {
                    Ok(done) => MaintenanceStep::Done(done),
                    Err(failure) => MaintenanceStep::Failed(failure),
                }
            }
        }
    }
}

/// Starts one ledger operation.
fn start_mark_step<F: NorStorage>(flash: &mut F, step: MarkStep) -> Result<(), F::Error> {
    match step {
        MarkStep::EraseLedger { address } => flash.erase_sector_4k(address),
        MarkStep::ProgramHeader { address } => {
            flash.page_program(address, &sync_ledger::SYNC_LEDGER_MAGIC)
        }
        MarkStep::ProgramBit { address, value } => flash.page_program(address, &[value]),
        MarkStep::Done => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_storage::layout::CONFIG_SECTOR_SIZE;
    use ferrowasp_storage::test_nor::MemoryNor;

    const CAPACITY: u32 = 64 * 1024;

    fn layout() -> StorageLayout {
        StorageLayout::new(CAPACITY).unwrap()
    }

    /// Steps until the operation ends, counting the steps.
    fn run(maintenance: &mut Maintenance, nor: &mut MemoryNor) -> (MaintenanceStep, u32) {
        let mut steps = 0;
        loop {
            let step = maintenance.step(nor, layout());
            steps += 1;
            if step != MaintenanceStep::Running {
                return (step, steps);
            }
        }
    }

    #[test]
    fn a_log_erase_clears_every_log_sector_and_spares_the_configuration() {
        let mut nor = MemoryNor::erased(CAPACITY);
        nor.bytes.fill(0x00);
        let mut erase = Maintenance::erase_log();
        assert!(erase.is_erasing_log());
        let (step, steps) = run(&mut erase, &mut nor);
        assert_eq!(step, MaintenanceStep::Done(MaintenanceDone::LogErased));
        assert_eq!(steps, layout().log_sector_count() + 1);
        let log_start = layout().log_start_address as usize;
        assert!(nor.bytes[log_start..].iter().all(|byte| *byte == 0xff));
        assert!(nor.bytes[..log_start].iter().all(|byte| *byte == 0x00));
        assert!(!erase.is_active());
    }

    #[test]
    fn a_failed_log_erase_stops() {
        let mut nor = MemoryNor::erased(CAPACITY);
        nor.fail_erase = true;
        let mut erase = Maintenance::erase_log();
        assert_eq!(
            run(&mut erase, &mut nor),
            (MaintenanceStep::Failed(MaintenanceFailure::LogErase), 1)
        );
    }

    #[test]
    fn the_scratch_test_verifies_and_catches_each_failure() {
        let mut nor = MemoryNor::erased(CAPACITY);
        assert_eq!(
            run(&mut Maintenance::scratch_test(), &mut nor),
            (MaintenanceStep::Done(MaintenanceDone::ScratchVerified), 3)
        );
        type Breaks = fn(&mut MemoryNor);
        let breaks: [(Breaks, MaintenanceFailure); 3] = [
            (
                |nor| nor.fail_erase = true,
                MaintenanceFailure::ScratchErase,
            ),
            (
                |nor| nor.fail_program = true,
                MaintenanceFailure::ScratchProgram,
            ),
            (
                |nor| nor.stuck_first_byte = Some(0x00),
                MaintenanceFailure::ScratchReadback,
            ),
        ];
        for (fault, failure) in breaks {
            let mut nor = MemoryNor::erased(CAPACITY);
            fault(&mut nor);
            assert_eq!(
                run(&mut Maintenance::scratch_test(), &mut nor).0,
                MaintenanceStep::Failed(failure)
            );
            assert!(failure.is_write_fault());
        }
        // The scratch sector is the only one touched.
        let mut nor = MemoryNor::erased(CAPACITY);
        nor.bytes.fill(0x00);
        run(&mut Maintenance::scratch_test(), &mut nor);
        let scratch = SCRATCH_SECTOR_ADDRESS as usize;
        assert!(nor.bytes[..scratch].iter().all(|byte| *byte == 0x00));
        let after = scratch + CONFIG_SECTOR_SIZE as usize;
        assert!(nor.bytes[after..].iter().all(|byte| *byte == 0x00));
    }

    #[test]
    fn a_sync_mark_records_the_flight_in_the_ledger() {
        let mut nor = MemoryNor::erased(CAPACITY);
        let synced = |nor: &mut MemoryNor, flight| {
            sync_ledger::is_synced(layout(), &mut |a, o: &mut [u8]| nor.read(a, o), flight).unwrap()
        };
        assert!(!synced(&mut nor, 7));
        let (step, steps) = run(&mut Maintenance::sync_mark(7), &mut nor);
        assert_eq!(
            step,
            MaintenanceStep::Done(MaintenanceDone::Synced { flight: 7 })
        );
        assert!(steps <= u32::from(MAX_SYNC_MARK_STEPS) + 1);
        assert!(synced(&mut nor, 7));
        assert!(!synced(&mut nor, 8));
    }

    #[test]
    fn a_failed_ledger_write_is_a_write_fault() {
        let mut nor = MemoryNor::erased(CAPACITY);
        nor.fail_program = true;
        let (step, _) = run(&mut Maintenance::sync_mark(7), &mut nor);
        assert_eq!(
            step,
            MaintenanceStep::Failed(MaintenanceFailure::SyncLedger { write_fault: true })
        );
    }
}
