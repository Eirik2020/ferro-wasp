use super::aliases::{HeartbeatTx, HeartbeatTxPin, UserLed, UserLedPin};
use super::manifest::{HEARTBEAT_BAUD, SYSTEM_CLOCK_HZ};
use stm32f4xx_hal::pac::{RCC, USART2};
use stm32f4xx_hal::prelude::*;
use stm32f4xx_hal::rcc::{Config, Rcc};
use stm32f4xx_hal::serial::config::InvalidConfig;

pub fn init_clocks(rcc: RCC) -> Rcc {
    rcc.freeze(Config::hsi().sysclk(SYSTEM_CLOCK_HZ.Hz()))
}

pub fn init_user_led(pin: UserLedPin) -> UserLed {
    pin.into_push_pull_output()
}

pub fn init_heartbeat_tx(
    usart: USART2,
    pin: HeartbeatTxPin,
    rcc: &mut Rcc,
) -> Result<HeartbeatTx, InvalidConfig> {
    usart.tx(pin, HEARTBEAT_BAUD.bps(), rcc)
}
