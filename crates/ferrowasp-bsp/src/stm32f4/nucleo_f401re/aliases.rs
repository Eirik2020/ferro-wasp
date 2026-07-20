use stm32f4xx_hal::gpio::*;
use stm32f4xx_hal::pac::USART2;
use stm32f4xx_hal::serial::Tx;

pub type HeartbeatTxPin = PA2<Input>;
pub type HeartbeatTx = Tx<USART2>;
pub type UserLedPin = PA5<Input>;
pub type UserLed = PA5<Output<PushPull>>;
