//! STM32F4 RTIC application facade.
//!
//! Application shells import this module instead of importing HAL or monotonic
//! crates directly. It deliberately exposes only the RTIC wiring surface.

pub use rtic_monotonics::systick_monotonic;
pub use stm32f4xx_hal as hal;

pub mod monotonic {
    pub use rtic_monotonics::systick::prelude::*;
}

/// Imports used by narrow STM32F4 RTIC application shells.
pub mod prelude {
    pub use super::{
        hal::{
            gpio::{Edge, ExtiPin, Input, Output, PA5, PC13, PinState, Pull, PushPull, Speed},
            prelude::*,
        },
        monotonic::*,
        systick_monotonic,
    };
}
