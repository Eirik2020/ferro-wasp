use core::fmt::Write;

use defmt::{info, warn};
use stm32f4xx_hal::{
    gpio::{Input, Output, PA2, PA5, PushPull},
    pac::USART2,
    prelude::*,
    rcc::Rcc,
    serial::{Tx, config::InvalidConfig},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeartbeatConfig {
    pub system_clock_hz: u32,
    pub baud: u32,
    pub period_ms: u32,
}

impl HeartbeatConfig {
    pub const fn new(system_clock_hz: u32, baud: u32, period_ms: u32) -> Self {
        Self {
            system_clock_hz,
            baud,
            period_ms,
        }
    }
}

pub struct HeartbeatResourceInputs {
    pub usart: USART2,
    pub led_pin: PA5<Input>,
    pub tx_pin: PA2<Input>,
}

pub struct HeartbeatResources {
    pub led: PA5<Output<PushPull>>,
    pub tx: Tx<USART2>,
    pub sequence: u32,
    pub system_clock_hz: u32,
    pub period_ms: u32,
}

pub fn init_heartbeat_resources(
    resources: HeartbeatResourceInputs,
    rcc: &mut Rcc,
    config: HeartbeatConfig,
) -> Result<HeartbeatResources, InvalidConfig> {
    let mut led = resources.led_pin.into_push_pull_output();
    let tx = resources
        .usart
        .tx(resources.tx_pin, config.baud.bps(), rcc)?;

    let _ = led.set_low();

    Ok(HeartbeatResources {
        led,
        tx,
        sequence: 0,
        system_clock_hz: config.system_clock_hz,
        period_ms: config.period_ms,
    })
}

pub fn run_heartbeat(resources: &mut HeartbeatResources) {
    resources.led.toggle();

    if write!(
        resources.tx,
        "FerroWasp NUCLEO-F401RE heartbeat {}\r\n",
        resources.sequence
    )
    .is_err()
    {
        warn!("USART2 heartbeat write failed");
    }

    info!("NUCLEO-F401RE RTIC heartbeat {}", resources.sequence);
    resources.sequence = resources.sequence.wrapping_add(1);
}
