#![deny(unsafe_code)]
#![no_std]

pub mod board;

// The RTIC app's imports, and the runtime crates below, exist only on the
// target; the board manifest's tests run on the host.
#[cfg(target_arch = "arm")]
pub mod internal {
    pub use crate::board;
    pub use crate::board::heartbeat::{
        HeartbeatConfig, HeartbeatResourceInputs, HeartbeatResources, heartbeat_task,
        init_heartbeat_resources,
    };
    pub use defmt::info;
    pub use ferrowasp_stm32::app_config::BRINGUP_HEARTBEAT_PERIOD_MS;
    pub use ferrowasp_stm32f4::clocks as stm32_clocks;
    pub use rtic_monotonics::systick::prelude::*;
    pub use stm32f4xx_hal::{pac, prelude::*};
}

#[cfg(target_arch = "arm")]
use defmt_rtt as _;
#[cfg(target_arch = "arm")]
use panic_probe as _;
