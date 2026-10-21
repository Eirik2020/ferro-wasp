//! The contracts between the safety-critical core and everything outside it
//! (ADR-0056, ADR-0060): the messages tasks send the observer and the topics
//! it publishes on the ledger. Readers depend on this crate, not on the
//! border roles, so a border change never re-owes their evidence.

#![no_std]
#![forbid(unsafe_code)]

pub mod messages;
pub mod topics;
