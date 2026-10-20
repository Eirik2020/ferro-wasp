//! Stored configuration, safety-critical (ADR-0059): the configuration keys
//! and their value ranges, the page a configuration slot is stored in, and
//! the stored configuration's schema, encoding and migration. Storage is
//! untrusted input, so decoding it is the trust boundary.

#![forbid(unsafe_code)]
#![no_std]

pub mod keys;
pub mod page;
pub mod stored;
