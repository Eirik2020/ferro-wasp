//! The formats FerroWasp shares with host tools, non-critical (design §16): the
//! onboard flight log's records and pages, and the compact rate record. W6
//! controls changes to them; FerroConfigurator reads the log through this
//! crate.

#![forbid(unsafe_code)]
#![no_std]

pub mod blackbox;
pub mod rate_record;
