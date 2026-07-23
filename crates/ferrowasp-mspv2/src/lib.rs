#![no_std]
#![forbid(unsafe_code)]

//! Bounded native MSPv2 framing and the opt-in FerroWasp configurator RPC.
//!
//! The crate owns wire-visible types but no transport, storage, safety, or
//! actuator resources. Application code decides which operations are
//! available and retains final authority over every request.

pub mod commands;
pub mod packet;
pub mod rpc;

pub use packet::{
    EncodedFrame, MAX_FRAME_LEN, MAX_PAYLOAD_LEN, MspDirection, MspPacket, MspPacketError,
    MspParser, crc8_dvb_s2, encode,
};
