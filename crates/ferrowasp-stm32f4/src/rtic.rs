//! STM32F4 RTIC application facade.
//!
//! Application shells import this module instead of importing HAL or monotonic
//! crates directly. It deliberately exposes only the RTIC wiring surface.

#[cfg(feature = "stm32f405-tim2-monotonic")]
pub use rtic_monotonics::stm32_tim2_monotonic;
pub use rtic_monotonics::systick_monotonic;
pub use stm32f4xx_hal as hal;

pub mod monotonic {
    pub use rtic_monotonics::systick::prelude::*;
}

/// Imports used by narrow STM32F4 RTIC application shells.
pub mod prelude {
    #[cfg(feature = "stm32f405-tim2-monotonic")]
    pub use super::stm32_tim2_monotonic;
    pub use super::{
        hal::{
            dma::StreamsTuple,
            gpio::{Edge, ExtiPin, Input, Output, Pin, PinState, Pull, PushPull, Speed},
            prelude::*,
        },
        monotonic::*,
        systick_monotonic,
    };
}
