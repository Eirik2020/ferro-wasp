//! The onboard flight log, non-critical (ADR-0060): the record queue and page
//! assembly, the log writer, the sync ledger of acknowledged flights, and log
//! maintenance (erase and the scratch-sector test).

#![forbid(unsafe_code)]
#![no_std]

pub mod log_writer;
pub mod records;
pub mod storage_maintenance;
pub mod sync_ledger;
