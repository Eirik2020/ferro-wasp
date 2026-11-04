//! Rebooting into the firmware or the ROM bootloader, through FerroHAL.
//!
//! Requirements: REBOOT-002, REBOOT-005, REBOOT-003, REBOOT-006, REBOOT-007
//! Safety-critical.

use cortex_m::peripheral::SCB;
use ferrohal_stm32::reset;
use ferrowasp_stm32_critical::system_reset::RebootTarget;

/// Reset the chip toward `target`. The caller has checked that the system is
/// disarmed and storage is idle.
pub fn reboot(target: RebootTarget) -> ! {
    match target {
        RebootTarget::Firmware => SCB::sys_reset(),
        RebootTarget::RomBootloader => reset::reboot_to_bootloader(),
    }
}

/// Enter the ROM bootloader if the last reset asked for it; otherwise return.
///
/// Call it first in `init`, before clocks or any peripheral is configured: the ROM
/// bootloader expects the chip as reset left it, and the motor pins are still in
/// their reset state, as at power-on.
pub fn enter_rom_bootloader_if_requested() {
    if reset::bootloader_requested() {
        // SAFETY: this runs first in `init`, before clocks or any peripheral is
        // configured, so the chip is as reset left it. RTIC's `init` holds interrupts
        // masked, which FerroHAL's entry undoes itself; nothing else has run.
        unsafe { reset::enter_bootloader() };
    }
}
