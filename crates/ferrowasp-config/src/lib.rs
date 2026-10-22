//! Stored configuration, safety-critical (design §14): the configuration keys
//! and their value ranges, the page a configuration slot is stored in, and
//! the stored configuration's schema, encoding and migration. Storage is
//! untrusted input, so decoding it is the trust boundary.
//!
//! Safety-critical.

#![forbid(unsafe_code)]
#![no_std]

pub mod keys;
pub mod page;
pub mod stored;
