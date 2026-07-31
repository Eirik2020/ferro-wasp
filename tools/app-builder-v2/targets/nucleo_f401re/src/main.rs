#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;
use defmt_rtt as _;

#[rtic::app(device = ferrowasp_stm32f4::rtic::hal::pac, peripherals = true, dispatchers = [EXTI0])]
mod app {
    use ferrowasp_io_core::digital::prelude::{OutputPin, StatefulOutputPin};
    use ferrowasp_stm32f4::rtic::prelude::*;

    // Monotonic timer declarations generated from the board declaration.
    systick_monotonic!(Mono, 1_000);

    #[shared]
    struct Shared {
        blink_enabled: bool,
    }

    #[local]
    struct Local {
        led3: PA5<Output<PushPull>>,
        user_button: PC13<Input>,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // Board clock, monotonic, and hardware initialization.
        let mut rcc =
            ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), 84000000, false);
        Mono::start(cx.core.SYST, 84000000);
        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let gpioc = cx.device.GPIOC.split(&mut rcc);
        let mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);
        let mut exti = cx.device.EXTI;
        let mut led3 = gpioa.pa5.into_push_pull_output_in_state(PinState::Low);
        led3.set_internal_resistor(Pull::None);
        led3.set_speed(Speed::Low);
        let user_button = Input::new(gpioc.pc13, Pull::Up);
        let user_button = ferrowasp_stm32f4::exti::init_input(user_button, &mut syscfg, &mut exti, Edge::Falling);

        // Initial tasks selected by AppDeclaration::init.spawns.
        blink_led::spawn().expect("init must spawn declared task blink_led");

        (Shared { blink_enabled: true }, Local { led3, user_button })
    }

    #[task(priority = 1, local = [led3], shared = [blink_enabled])]
    async fn blink_led(mut cx: blink_led::Context) {
        let mut blink_count = 0_u32;
        loop {
            Mono::delay(1000.millis()).await;
            let enabled = cx.shared.blink_enabled.lock(|enabled| *enabled);
            if enabled {
                let _ = StatefulOutputPin::toggle(cx.local.led3);
                blink_count = blink_count.wrapping_add(1);
                report_blink::spawn(blink_count).expect("blink report task queue must have capacity");
            } else {
                let _ = OutputPin::set_low(cx.local.led3);
            }
            Mono::delay(5000.millis()).await;
        }
    }

    #[task(priority = 1)]
    async fn report_blink(cx: report_blink::Context, count: u32) {
        let _ = cx;
        defmt::info!("Blink {}", count);
    }

    #[task(binds = EXTI15_10, priority = 2, local = [user_button], shared = [blink_enabled])]
    fn button_exti(mut cx: button_exti::Context) {
        cx.local.user_button.clear_interrupt_pending_bit();
        let enabled = cx.shared.blink_enabled.lock(|enabled| {
            *enabled = !*enabled;
            *enabled
        });
        defmt::info!("Blink enabled: {}", enabled);
    }
}
