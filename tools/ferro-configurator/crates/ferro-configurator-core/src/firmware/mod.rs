mod board;
mod dfu_entry;
mod dfu_util;
mod elf;
mod manifest;

pub use board::BoardProfile;
pub use dfu_entry::{
    DFU_ENTRY_TIMEOUT, RESTART_TIMEOUT, enter_rom_bootloader, find_board_port, wait_for_board_port,
    wait_for_dfu_device,
};
pub use dfu_util::{DfuDetection, FlashProgress, FlashResult, detect_dfu, flash_firmware};
pub use elf::{PreparedImage, prepare_elf};
pub use manifest::{
    BundledFirmware, FirmwareArtifact, ReleaseManifest, find_bundled_firmware,
    load_bundled_firmware, sha256_file,
};
