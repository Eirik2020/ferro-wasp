//! Board support: the HAL glue over the board's data crate, which holds the
//! hardware facts (`ferrowasp-board-foxeer-f405-v2`, in `board/`).
//!
//! Requirements: BUILD-001

#[cfg(all(target_arch = "arm", feature = "board-foxeer-f405-v2"))]
pub mod aliases;
#[cfg(all(target_arch = "arm", feature = "board-foxeer-f405-v2"))]
pub mod init;

pub use ferrowasp_board_foxeer_f405_v2::*;
