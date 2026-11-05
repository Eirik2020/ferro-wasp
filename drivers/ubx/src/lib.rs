//! The u-blox UBX binary protocol for a flight controller talking to an M10
//! receiver: framing, the configuration database, and the navigation
//! solution.
//!
//! - [`Deframer`] takes bytes one at a time and returns checked [`Frame`]s.
//!   NMEA and other bytes between frames are counted and dropped. A frame
//!   that fails its checksum or announces a payload longer than the buffer is
//!   dropped, and the bytes after its first are scanned again, so a real frame
//!   that started inside it is still found.
//! - [`encode`] writes a complete frame into a caller's slice.
//! - [`cfg`](mod@cfg) builds `CFG-VALSET` and `CFG-VALGET` requests, reads
//!   `CFG-VALGET` responses, and matches `ACK-ACK` and `ACK-NAK` to the
//!   request they answer.
//! - [`NavPvt`] decodes the 92-byte `NAV-PVT` payload into integers in the
//!   protocol's own units. A position is reachable only through
//!   [`NavPvt::fix`], which is `None` unless the receiver reports a valid 3D
//!   fix.
//! - [`UartReader`] and [`UartWriter`] carry frames over `embedded-io`, and
//!   [`Ddc`] over the receiver's I2C interface through `embedded-hal`.
//!
//! Every call does bounded work, nothing allocates, and no input panics.
//!
//! # Sources
//!
//! Message classes, identifiers, layouts and configuration key IDs are taken
//! from two independent implementations, `ublox` 0.10.0 and `kinavis-ubx`
//! 0.1.1, and cross-checked where both carry them; each constant's
//! documentation cites the file and line it came from. Where only one carries
//! a fact, the documentation says *single-source*. Where neither does, the
//! documentation says *inferred*: such a fact has no second implementation
//! behind it and needs checking against the u-blox M10 interface description
//! and a receiver. The crate's tests use both implementations as oracles.

#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::arithmetic_side_effects
    )
)]

pub mod cfg;
mod ddc;
mod frame;
mod nav_pvt;
mod uart;

pub use ddc::{DEFAULT_MAX_PENDING, Ddc, DdcAddress, DdcError};
pub use frame::{
    DeframeError, Deframer, EncodeError, Event, Frame, HEADER_LEN, OVERHEAD, SYNC_1, SYNC_2, Stats,
    checksum, encode,
};
pub use nav_pvt::{
    CarrierSolution, Fix, FixType, NAV_PVT_LEN, NavPvt, PvtError, PvtFlags, Solution, TimeValidity,
    UtcTime,
};
pub use uart::{ReadError, UartReader, UartWriter};

/// Message classes and identifiers this crate reads or writes. ublox paths
/// are under its `src/ubx_packets/packets/`, kinavis-ubx paths under its
/// `src/`.
pub mod id {
    /// Class `NAV`: ublox `nav_pvt/proto27.rs:17` (`class = 1`), kinavis-ubx
    /// `nav.rs:19`.
    pub const CLASS_NAV: u8 = 0x01;
    /// `NAV-PVT`: ublox `nav_pvt/proto27.rs:17` (`id = 0x07`), kinavis-ubx
    /// `nav.rs:25`.
    pub const NAV_PVT: u8 = 0x07;
    /// Class `ACK`: ublox `ack.rs:12,29` (`class = 5`), kinavis-ubx
    /// `message.rs:11`.
    pub const CLASS_ACK: u8 = 0x05;
    /// `ACK-ACK`: ublox `ack.rs:12` (`id = 1`), kinavis-ubx
    /// `message.rs:132-133`.
    pub const ACK_ACK: u8 = 0x01;
    /// `ACK-NAK`: ublox `ack.rs:29` (`id = 0`), kinavis-ubx
    /// `message.rs:125,135`.
    pub const ACK_NAK: u8 = 0x00;
    /// Class `CFG`: ublox `cfg_val.rs:19`, kinavis-ubx `config.rs:14`.
    pub const CLASS_CFG: u8 = 0x06;
    /// `CFG-VALSET`: ublox `cfg_val.rs:20`, kinavis-ubx `config.rs:18`.
    pub const CFG_VALSET: u8 = 0x8A;
    /// `CFG-VALGET`: ublox `cfg_val.rs:39,61`, kinavis-ubx `config.rs:20`.
    pub const CFG_VALGET: u8 = 0x8B;
}
