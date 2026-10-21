//! The command line, non-critical (ADR-0060): the commands USB and the UART
//! command line carry, the link that routes them, the storage, log-sync and
//! debug commands, and the opt-in MSPv2 configurator RPC.

#![forbid(unsafe_code)]
#![no_std]

#[cfg(feature = "mspv2_configurator")]
pub mod blackbox_storage;
pub mod command_link;
pub mod commands;
pub mod log_sync;
#[cfg(feature = "mspv2_configurator")]
pub mod rpc_commands;
pub mod serial_reply;
pub mod storage_commands;
#[cfg(all(feature = "tasks", target_arch = "arm"))]
pub mod tasks;
pub mod usb_debug;
