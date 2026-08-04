#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;
use defmt_rtt as _;

#[rtic::app(device = ferrowasp_stm32f4::rtic::hal::pac, peripherals = true, dispatchers = [{{DISPATCHERS}}])]
mod app {
    {{RTIC_IMPORTS}}

    // Monotonic timer declarations generated from the board declaration.
    {{MONOTONIC_DECLARATION}}

    #[shared]
    {{SHARED_STRUCT}}

    #[local]
    {{LOCAL_STRUCT}}

    {{INIT_ATTRIBUTE}}
    fn init(cx: init::Context) -> (Shared, Local) {
        // Board clock, monotonic, and hardware initialization.
        {{BOARD_INIT}}

        // Initial tasks selected by AppDeclaration::init.spawns.
        {{INIT_SPAWNS}}

        ({{SHARED_VALUE}}, {{LOCAL_VALUE}})
    }

    {{TASKS}}
}
