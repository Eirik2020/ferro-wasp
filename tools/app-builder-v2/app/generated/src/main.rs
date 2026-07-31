#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;

#[rtic::app(device = ferrowasp_stm32f4::rtic::hal::pac, peripherals = true, dispatchers = [EXTI0])]
mod app {
    use ferrowasp_stm32f4::rtic::prelude::*;

    // Monotonic timer declarations generated from the board declaration.
    systick_monotonic!(Mono, 1_000);

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        led2: PA5<Output<PushPull>>,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // Board clock, monotonic, and hardware initialization.
        let mut rcc =
            ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), 84000000, false);
        Mono::start(cx.core.SYST, 84000000);
        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let mut led2 = gpioa.pa5.into_push_pull_output_in_state(PinState::Low);
        led2.set_internal_resistor(Pull::None);
        led2.set_speed(Speed::Low);

        // Initial tasks selected by AppDeclaration::init.spawns.

        (Shared {}, Local { led2 })
    }

    #[task(priority = 1, local = [led2])]
    async fn blink_led(cx: blink_led::Context) {
        loop {
            Mono::delay(1000.millis()).await;
            // HELLO
            let _ = cx.local.led2.toggle();
            Mono::delay(5000.millis()).await;
        }
    }
}
