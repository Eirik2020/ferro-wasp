#[cfg(all(target_arch = "arm", feature = "nucleo-f401re"))]
pub mod aliases;
#[cfg(all(target_arch = "arm", feature = "nucleo-f401re"))]
pub mod init;
pub mod manifest;

pub use manifest::{
    BOARD_CAPABILITIES, BOARD_IDENTITY, CLAIMS, HEARTBEAT_BAUD, PIN_MAP, SYSTEM_CLOCK_HZ,
};
