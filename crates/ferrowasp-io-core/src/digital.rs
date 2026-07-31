//! HAL-independent digital I/O capabilities used by reusable task logic.

pub mod prelude {
    pub use embedded_hal::digital::{OutputPin, StatefulOutputPin};
}
