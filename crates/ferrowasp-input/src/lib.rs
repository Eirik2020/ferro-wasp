//! Command Input, safety-critical (ADR-0060): the RC receiver that turns
//! either wire protocol into one frame shape. The RC group's task and the
//! Command Input decisions join it as the redesign proceeds.

#![forbid(unsafe_code)]
#![no_std]

pub mod channel_map;
pub mod rc_receiver;
