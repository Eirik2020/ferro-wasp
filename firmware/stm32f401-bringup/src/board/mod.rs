#[cfg(target_arch = "arm")]
pub mod heartbeat;
pub mod manifest;

pub use manifest::{BOARD_CAPABILITIES, BOARD_IDENTITY, CLAIMS, HEARTBEAT_BAUD, PIN_MAP};
