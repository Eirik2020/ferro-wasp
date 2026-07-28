#![deny(unsafe_code)]
#![no_std]

pub mod board;

pub mod internal {
    pub use crate::board;
    pub use defmt::info;
    pub use ferrowasp_stm32f4::app_config::BRINGUP_HEARTBEAT_PERIOD_MS;
    pub use ferrowasp_stm32f4::bringup::{
        HeartbeatConfig, HeartbeatResourceInputs, HeartbeatResources, init_heartbeat_resources,
        run_heartbeat,
    };
    pub use ferrowasp_stm32f4::clocks as stm32_clocks;
    pub use rtic_monotonics::systick::prelude::*;
    pub use stm32f4xx_hal::{pac, prelude::*};
}

use defmt_rtt as _;
use panic_probe as _;
