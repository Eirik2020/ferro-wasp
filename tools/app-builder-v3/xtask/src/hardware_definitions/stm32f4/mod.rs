//! Hardware-neutral physical definitions used by builder authoring APIs.

/// Board declaration types and structural validation.
pub mod board_declaration;

/// Reusable STM32F4 hardware endpoints that own hardware task graphs.
pub mod hw_endpoint;

/// DMA route definitions.
pub mod dma_route;

/// GPIO electrical and initialization declarations.
pub mod gpio;

/// MCU and clock declarations.
pub mod mcu;

/// STM32F4 lowering from resolved applications to RTIC source fragments.
pub(crate) mod lower;

/// Physical GPIO pin definitions.
pub mod pins;

/// Serial peripheral definitions.
pub mod serial;

/// Reusable tasks that depend directly on STM32F4 HAL capabilities.
pub mod tasks;

/// Types commonly used when declaring an STM32F4 board.
pub mod board_prelude {
    pub use super::{
        board_declaration::{BoardDeclaration, SerialHardwareDeclaration},
        dma_route::{DmaChannel, DmaController, DmaRoute, DmaStream},
        gpio::{
            Drive, ExternalInterrupt, GpioHardwareDeclaration, GpioMode, InterruptEdge, Level, Pull,
        },
        mcu::{ClockDeclaration, ClockSource, Mcu, McuDeclaration},
        pins::{GpioPort, PinId},
        serial::{SerialPeripheral, SerialRoute},
    };
}
