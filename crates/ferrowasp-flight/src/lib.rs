#![no_std]
#![forbid(unsafe_code)]

pub use ferrowasp_control::calibration::accel_level;
pub mod actuator;
pub mod arming;
#[cfg(feature = "mspv2_configurator")]
pub mod blackbox_storage;
pub mod command_link;
pub mod config_save;
pub mod drone_toolbox;
pub mod dynamic_idle;
pub mod esc_manager;
pub mod flash_storage;
pub mod log_sync;
pub mod log_writer;
pub mod osd;
pub use ferrowasp_input::rc_receiver;
#[cfg(feature = "mspv2_configurator")]
pub mod rpc_commands;
pub mod rpm_filter;
pub mod storage_commands;
pub mod storage_maintenance;
pub mod sync_ledger;
pub mod usb_debug;

#[cfg(test)]
mod test_nor;
