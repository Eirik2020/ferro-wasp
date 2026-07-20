#![deny(unsafe_code)]
#![no_main]
#![no_std]

use core::fmt::Write;
use defmt::{info, warn};
use defmt_rtt as _;
use ferrowasp_bsp::stm32f4::nucleo_f401re as board;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;
use stm32f4xx_hal::{pac, prelude::*};

#[rtic::app(device = pac, peripherals = true, dispatchers = [EXTI0])]
mod app {
    use super::*;

    systick_monotonic!(Mono, 1000);

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        led: board::aliases::UserLed,
        heartbeat_tx: board::aliases::HeartbeatTx,
        sequence: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let device = cx.device;
        let mut rcc = board::init::init_clocks(device.RCC);
        let gpioa = device.GPIOA.split(&mut rcc);
        let mut led = board::init::init_user_led(gpioa.pa5);
        let heartbeat_tx = board::init::init_heartbeat_tx(device.USART2, gpioa.pa2, &mut rcc)
            .expect("USART2 heartbeat configuration must be valid");

        Mono::start(cx.core.SYST, board::SYSTEM_CLOCK_HZ);
        led.set_low();

        info!(
            "FerroWasp {} RTIC bring-up started",
            board::BOARD_IDENTITY.name
        );
        heartbeat::spawn().unwrap();

        (
            Shared {},
            Local {
                led,
                heartbeat_tx,
                sequence: 0,
            },
        )
    }

    #[task(priority = 1, local = [led, heartbeat_tx, sequence])]
    async fn heartbeat(cx: heartbeat::Context) {
        info!("Running RTIC heartbeat!");

        loop {
            cx.local.led.toggle();

            if write!(
                cx.local.heartbeat_tx,
                "FerroWasp NUCLEO-F401RE heartbeat {}\r\n",
                *cx.local.sequence
            )
            .is_err()
            {
                warn!("USART2 heartbeat write failed");
            }

            info!("NUCLEO-F401RE RTIC heartbeat {}", *cx.local.sequence);
            *cx.local.sequence = cx.local.sequence.wrapping_add(1);
            Mono::delay(1_000.millis()).await;
        }
    }
}
