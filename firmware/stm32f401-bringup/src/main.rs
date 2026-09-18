#![deny(unsafe_code)]
#![no_main]
#![no_std]

use ferrowasp_app_stm32f401_bringup::internal::*;

#[rtic::app(device = pac, peripherals = true, dispatchers = [EXTI0])]
mod app {
    use super::*;

    systick_monotonic!(Mono, 1000);

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        heartbeat: HeartbeatResources,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let device = cx.device;
        let mut rcc = stm32_clocks::freeze_hsi(
            device.RCC.constrain(),
            stm32_clocks::STM32F401_SYSTEM_CLOCK_HZ,
            false,
        );
        let gpioa = device.GPIOA.split(&mut rcc);
        let heartbeat = init_heartbeat_resources(
            HeartbeatResourceInputs {
                usart: device.USART2,
                led_pin: gpioa.pa5,
                tx_pin: gpioa.pa2,
            },
            &mut rcc,
            HeartbeatConfig::new(
                stm32_clocks::STM32F401_SYSTEM_CLOCK_HZ,
                board::HEARTBEAT_BAUD,
                BRINGUP_HEARTBEAT_PERIOD_MS,
            ),
        )
        .expect("NUCLEO-F401RE heartbeat configuration must be valid");

        Mono::start(cx.core.SYST, heartbeat.system_clock_hz);

        info!(
            "FerroWasp {} RTIC bring-up started",
            board::BOARD_IDENTITY.name
        );
        heartbeat::spawn().unwrap();

        (Shared {}, Local { heartbeat })
    }

    #[task(priority = 1, local = [heartbeat])]
    async fn heartbeat(cx: heartbeat::Context) {
        loop {
            run_heartbeat(cx.local.heartbeat);
            Mono::delay(cx.local.heartbeat.period_ms.millis()).await;
        }
    }
}
