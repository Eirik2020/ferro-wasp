//! Motor output, safety-critical (ADR-0060): DShot packet and waveform
//! encoding, the command lease and special-command sequences, and the
//! bidirectional reply decoding. The output group's task joins it once the
//! backends split.

#![forbid(unsafe_code)]
#![no_std]

pub mod dshot;
pub mod dshot_reply;
