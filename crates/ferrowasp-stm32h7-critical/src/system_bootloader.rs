//! Rebooting into the firmware or the STM32H743's ROM bootloader.
//!
//! The mechanism is `ferrowasp_stm32_critical::system_reset`. Unsafe code here does
//! three things: places the request word in RAM startup leaves alone, reads
//! the reset flags before the HAL owns RCC, and jumps to system memory.
//!
//! Requirements: REBOOT-002
//! Safety-critical.

use core::mem::MaybeUninit;

use cortex_m::peripheral::{NVIC, SCB, SYST};
use ferrowasp_stm32_critical::system_reset::{self, RebootTarget};
use stm32h7xx_hal::pac;

/// The ROM bootloader's vector table in the H743's system memory (AN2606).
const SYSTEM_MEMORY: u32 = 0x1FF0_9800;

// `.uninit` is NOLOAD RAM that cortex-m-rt neither copies nor zeroes, so
// the word keeps its value across a software reset. It has no initial value
// the compiler could assume; every access is volatile.
#[unsafe(link_section = ".uninit.ferrowasp_reboot_request")]
static mut REBOOT_REQUEST: MaybeUninit<u32> = MaybeUninit::uninit();

fn write_request(word: u32) {
    // SAFETY: one aligned word, written only by the task about to reset and
    // by startup before any task runs.
    unsafe { core::ptr::write_volatile((&raw mut REBOOT_REQUEST).cast::<u32>(), word) }
}

fn read_request() -> u32 {
    // SAFETY: as for `write_request`; read only after a software reset,
    // which follows a `write_request`.
    unsafe { core::ptr::read_volatile((&raw const REBOOT_REQUEST).cast::<u32>()) }
}

/// Reset the chip toward `target`. The caller has checked that the system is
/// disarmed and storage is idle.
pub fn reboot(target: RebootTarget) -> ! {
    write_request(system_reset::request_word(target));
    cortex_m::asm::dsb();
    SCB::sys_reset()
}

/// Enter the ROM bootloader if the last reset asked for it; otherwise return.
///
/// Call it first in `init`, before the power supply, clocks, caches or any
/// peripheral is configured: the ROM bootloader expects the chip as reset
/// left it, and the motor pins are still in their reset state, as at
/// power-on.
pub fn enter_rom_bootloader_if_requested() {
    // SAFETY: a read of RCC's reset flags before the HAL takes RCC.
    let rcc = unsafe { &*pac::RCC::ptr() };
    let software_reset = rcc.rsr.read().sftrstf().bit_is_set();
    let word = if software_reset { read_request() } else { 0 };
    write_request(0);
    if !system_reset::enters_rom_bootloader(software_reset, word) {
        return;
    }

    cortex_m::interrupt::disable();
    // SAFETY: init owns the chip and nothing has been configured; the caches
    // are still off. SysTick and every NVIC source are stopped, so the
    // bootloader starts with no interrupt of ours pending, and SYSTEM_MEMORY
    // holds the bootloader's stack pointer and reset vector.
    unsafe {
        (*SYST::PTR).csr.write(0);
        let nvic = &*NVIC::PTR;
        for index in 0..nvic.icer.len() {
            nvic.icer[index].write(u32::MAX);
            nvic.icpr[index].write(u32::MAX);
        }
        (*SCB::PTR).vtor.write(SYSTEM_MEMORY);
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
        cortex_m::interrupt::enable();
        cortex_m::asm::bootload(SYSTEM_MEMORY as *const u32)
    }
}
