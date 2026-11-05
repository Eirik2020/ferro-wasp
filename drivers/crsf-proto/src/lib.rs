//! CRSF, the serial protocol ExpressLRS and TBS Crossfire receivers speak to
//! a flight controller, for `no_std` firmware: a bounded frame parser, the
//! telemetry encoders, and a reader and writer over `embedded-io`.
//!
//! The link runs at 420 000 baud, 8N1, not inverted. A frame is an address
//! byte, a length byte counting the type, payload and CRC, a type byte, the
//! payload, and a CRC-8/DVB-S2 over the type and payload; [`frame`] holds the
//! layout.
//!
//! - [`StreamingParser`] takes one byte at a time, so a DMA or interrupt
//!   handler with a chunk of bytes feeds it directly. Only the flight
//!   controller, receiver and transmitter addresses start a frame. A bad
//!   length or CRC drops the frame and returns to address search. Its state
//!   is one 64-byte buffer, and each byte costs bounded work.
//! - The encoders in [`telemetry`] build battery, GPS, vario, barometric
//!   altitude, attitude and flight-mode frames from typed units, integer-only,
//!   bounding every field explicitly.
//! - With the default `io` feature, [`FrameReader`] pulls bytes from an
//!   `embedded_io::Read` source into the parser, at most one read per call,
//!   and [`TelemetryWriter`] sends a checked frame to an `embedded_io::Write`
//!   sink without `write_all`'s panic on a sink that accepts nothing.
//!
//! CRSF has no failsafe flag in the channel frame. An ExpressLRS receiver
//! stops sending channels when its link drops, so the consumer's frame
//! timeout is the primary loss detector; an uplink link quality of zero in
//! the link statistics is an explicit loss signal. Judging the link is the
//! consumer's job; this crate only reports what arrived.
//!
//! The crate never panics on any input, never allocates, and has no unsafe
//! code.
//!
//! Safety-critical.

#![no_std]
#![forbid(unsafe_code)]

pub mod channels;
pub mod frame;
#[cfg(feature = "io")]
pub mod io;
pub mod parser;
pub mod telemetry;

#[cfg(test)]
mod cross_check;

pub use channels::{CHANNEL_COUNT, pack_channels, unpack_channels};
pub use frame::{MAX_FRAME_LEN, crc8_dvb_s2};
#[cfg(feature = "io")]
pub use io::{FrameReader, ReadError, TelemetryWriter, WriteError};
pub use parser::{DecodeError, Frame, LinkStatistics, ParseResult, StreamingParser};
