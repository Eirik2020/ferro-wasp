#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;

#[rtic::app(device = {{PAC_PATH}}, peripherals = true{{RTIC_DISPATCHERS}})]
mod app {
    use rtic_monotonics::systick::prelude::*;
    {{FEATURE_IMPORTS}}

    // Backend-owned base timer. Logical component timers derive delays from
    // this one monotonic endpoint instead of reserving STM32 TIM peripherals.
    systick_monotonic!(Mono, 1_000);

    #[shared]
    struct Shared {
        {{SHARED_RESOURCES}}
    }

    #[local]
    struct Local {
        {{LOCAL_RESOURCES}}
    }

    #[init{{RTIC_INIT_LOCALS}}]
    fn init({{INIT_CONTEXT}}: init::Context) -> (Shared, Local) {
        {{BASE_INIT}}
        {{FEATURE_INIT}}

        (
            Shared {
                {{SHARED_RESOURCE_VALUES}}
            },
            Local {
                {{LOCAL_RESOURCE_VALUES}}
            },
        )
    }

    {{FEATURE_TASKS}}
}
