//! Rebooting on request, into the firmware or into the chip's ROM bootloader.
//!
//! The ROM bootloader is the USB DFU device the configurator flashes through.
//! A request to enter it is a word in RAM that startup neither copies nor
//! zeroes, written just before a software reset. Startup trusts the word only
//! after a software reset; after power-on the RAM holds anything, so the word
//! is cleared without being read. Each chip backend owns the word, the reset,
//! and the jump: `ferrowasp-stm32f4::system_bootloader` and
//! `ferrowasp-stm32h7::system_bootloader`.
//!
//! Requirements: REBOOT-002, REBOOT-005
//! Safety-critical.

/// Where a requested reboot goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebootTarget {
    /// Restart the flight firmware.
    Firmware,
    /// Start the chip's ROM bootloader, which enumerates as a USB DFU device.
    RomBootloader,
}

/// The word that sends the next startup to the ROM bootloader: `DFU!`.
pub const ROM_BOOTLOADER_REQUEST: u32 = u32::from_le_bytes(*b"DFU!");

/// The word a backend stores before resetting toward `target`.
pub const fn request_word(target: RebootTarget) -> u32 {
    match target {
        RebootTarget::Firmware => 0,
        RebootTarget::RomBootloader => ROM_BOOTLOADER_REQUEST,
    }
}

/// Whether startup enters the ROM bootloader: only after a software reset,
/// and only for the exact request word.
pub const fn enters_rom_bootloader(software_reset: bool, word: u32) -> bool {
    software_reset && word == ROM_BOOTLOADER_REQUEST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_software_reset_with_the_request_enters_the_bootloader() {
        let request = request_word(RebootTarget::RomBootloader);
        assert!(enters_rom_bootloader(true, request));
        assert!(!enters_rom_bootloader(false, request));
        assert!(!enters_rom_bootloader(
            true,
            request_word(RebootTarget::Firmware)
        ));
        assert!(!enters_rom_bootloader(true, request ^ 1));
    }
}
