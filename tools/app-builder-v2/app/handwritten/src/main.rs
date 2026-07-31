#![no_main]
#![no_std]
#![forbid(unsafe_code)]

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
    struct Shared {}

    #[local]
    struct Local {
        led2: PA5<Output<PushPull>>,
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

        (Shared {}, Local { led2 })
    }

    #[task(
    priority = 1,
    local = [
        led2,
    ]
)]
    async fn blink_led(mut cx: blink_led::Context) {
        loop {
            Mono::delay(1000.millis()).await;
            let _ = cx.local.led2.set_low();
        }
    }
}
