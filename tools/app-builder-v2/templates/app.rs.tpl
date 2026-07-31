#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;

#[rtic::app(device = ferrowasp_stm32f4::rtic::hal::pac, peripherals = true, dispatchers = [EXTI0])]
mod app {
    {{RTIC_IMPORTS}}

    // Monotonic timer declarations generated from the board declaration.
    {{MONOTONIC_DECLARATION}}

    #[shared]
    struct Shared {}

    #[local]
    {{LOCAL_STRUCT}}

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // Board clock, monotonic, and hardware initialization.
        {{BOARD_INIT}}

        // Initial tasks selected by AppDeclaration::init.spawns.
        {{INIT_SPAWNS}}

        (Shared {}, {{LOCAL_VALUE}})
    }

    {{TASKS}}
}
