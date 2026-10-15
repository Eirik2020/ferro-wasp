//! Board support: the HAL glue over the board's data crate, which holds the
//! hardware facts (`ferrowasp-board-tbs-lucid-h7`, in `board/`).

#[cfg(all(target_arch = "arm", feature = "board-tbs-lucid-h7"))]
pub mod aliases;
#[cfg(all(target_arch = "arm", feature = "board-tbs-lucid-h7"))]
pub mod init;

pub use ferrowasp_board_tbs_lucid_h7::*;
