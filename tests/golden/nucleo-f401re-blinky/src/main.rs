#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [EXTI0, EXTI1])]
mod app {
    use rtic_monotonics::systick::prelude::*;
    use stm32f4xx_hal::gpio::Edge;
    use stm32f4xx_hal::gpio::Input;
    use stm32f4xx_hal::gpio::Output;
    use stm32f4xx_hal::gpio::PA5;
    use stm32f4xx_hal::gpio::PC13;
    use stm32f4xx_hal::gpio::PinState;
    use stm32f4xx_hal::gpio::Pull;
    use stm32f4xx_hal::gpio::PushPull;
    use stm32f4xx_hal::gpio::Speed;
    use stm32f4xx_hal::pac::EXTI;
    use stm32f4xx_hal::prelude::*;
    use stm32f4xx_hal::rcc::Config;

    // Backend-owned base timer. Logical component timers derive delays from
    // this one monotonic endpoint instead of reserving STM32 TIM peripherals.
    systick_monotonic!(Mono, 1_000);

    #[shared]
    struct Shared {
        led2: PA5<Output<PushPull>>,
        blinker_enabled: bool,

        user_button: PC13<Input>,
        user_button_exti: EXTI,
        button_debounce_spawn_failures: u32,
    }

    #[local]
    struct Local {
        // The blink-led feature has no task-local resources.

        // Blinker state is owned by the blink component.
    }

    #[init]
    fn init(mut cx: init::Context) -> (Shared, Local) {
        let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
        Mono::start(cx.core.SYST, 84_000_000);
        let mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);
        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let gpioc = cx.device.GPIOC.split(&mut rcc);
        let mut led2 = gpioa.pa5.into_push_pull_output_in_state(PinState::High);
        led2.set_internal_resistor(Pull::None);
        led2.set_speed(Speed::Low);
        if blink_led::spawn().is_err() {
            panic!("failed to start the divergent blink task during initialization");
        }

        let mut user_button = Input::new(gpioc.pc13, Pull::Up);
        user_button.make_interrupt_source(&mut syscfg);
        user_button.trigger_on_edge(&mut cx.device.EXTI, Edge::Falling);
        user_button.enable_interrupt(&mut cx.device.EXTI);
        let user_button_exti = cx.device.EXTI;

        (
            Shared {
                led2,
                blinker_enabled: true,

                user_button,
                user_button_exti,
                button_debounce_spawn_failures: 0,
            },
            Local {
                // The blink-led feature has no task-local resource values.

// No task-local state.
            },
        )
    }

    #[task(
    priority = 1,
    shared = [
        led2,
        blinker_enabled
    ]
)]
    async fn blink_led(mut cx: blink_led::Context) {
        loop {
            Mono::delay(1000.millis()).await;
            (&mut cx.shared.blinker_enabled, &mut cx.shared.led2).lock(|enabled, led| {
                if *enabled {
                    let _ = led.toggle();
                }
            });
        }
    }

    #[task(
    binds = EXTI15_10,
    priority = 2,
    shared = [user_button, user_button_exti, button_debounce_spawn_failures]
)]
    fn button_toggle(cx: button_toggle::Context) {
        (
            cx.shared.user_button,
            cx.shared.user_button_exti,
            cx.shared.button_debounce_spawn_failures,
        )
            .lock(|button, exti, faults| {
                button.clear_interrupt_pending_bit();
                button.disable_interrupt(exti);
                if button_toggle_debounce::spawn().is_err() {
                    *faults = faults.saturating_add(1);
                }
            });
    }

    #[task(
    priority = 2,
    shared = [
        user_button,
        user_button_exti,
        led2,
        blinker_enabled
    ]
)]
    async fn button_toggle_debounce(cx: button_toggle_debounce::Context) {
        Mono::delay(20.millis()).await;
        let pressed = (cx.shared.user_button, cx.shared.user_button_exti).lock(|button, exti| {
            let pressed = button.is_low();
            button.clear_interrupt_pending_bit();
            button.enable_interrupt(exti);
            pressed
        });

        if pressed {
            (cx.shared.blinker_enabled, cx.shared.led2).lock(|enabled, led| {
                *enabled = !*enabled;
                if !*enabled {
                    let _ = led.set_low();
                }
            });
        }
    }
}
