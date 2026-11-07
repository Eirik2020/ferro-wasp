//! Board support: the HAL glue over the board's data crate, which holds the
//! hardware facts (`ferrowasp-board-speedybee-f405-v5`, in `board/`).
//!
//! Requirements: BUILD-001

#[cfg(all(target_arch = "arm", feature = "board-speedybee-f405-v5"))]
pub mod aliases;
#[cfg(all(target_arch = "arm", feature = "board-speedybee-f405-v5"))]
pub mod init;

pub use ferrowasp_board_speedybee_f405_v5::*;
