//! Serial types and transport, re-exported from `ferrowasp-serial` and
//! `ferrowasp-io-contracts` under their former paths until this crate
//! dissolves, and the `serial` command's reply.

pub use ferrowasp_serial::*;

mod reply;

pub use reply::{SERIAL_REPLY_LINE_CAPACITY, write_serial_bindings};
