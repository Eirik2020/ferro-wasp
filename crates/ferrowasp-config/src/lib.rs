//! Stored configuration, safety-critical (design §14): the configuration keys
//! and their value ranges, the page a configuration slot is stored in, the
//! stored configuration's schema, encoding and migration, the choice of
//! slot at boot, and the config owner that every change passes. Storage is
//! untrusted input, so decoding and validating it is the trust boundary.
//!
//! Safety-critical.

#![forbid(unsafe_code)]
#![no_std]

pub mod boot;
pub mod owner;
/// The configuration keys, which requests to the coordinator name.
pub use ferrowasp_boundary_contracts::config_keys as keys;
pub mod page;
pub mod stored;
