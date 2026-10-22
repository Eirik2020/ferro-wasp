//! The contracts between the safety-critical core and everything outside it
//! (design §10, §11, §16): the messages tasks send the observer, the topics
//! it publishes on the ledger, and the requests tasks send the coordinator.
//! Readers depend on this crate, not on the
//! border roles, so a border change never re-owes their evidence.
//!
//! Safety-critical.

#![no_std]
#![forbid(unsafe_code)]

pub mod messages;
pub mod requests;
pub mod topics;
