//! The command line's RTIC task definitions (ADR-0060, ADR-0065),
//! target-only: USB and its MSPv2 configurator path, the command line on a
//! UART, the storage owner, and the heartbeat, with the reports they build
//! from the observer's ledger.

pub mod flash;
pub mod heartbeat;
pub mod link;
pub mod usb;
pub mod views;

/// The names the task modules are written against; each comes from this
/// crate or one it depends on.
#[allow(unused_imports)]
mod prelude {
    pub use core::sync::atomic::Ordering;
    pub use defmt::{info, warn};
    pub use embedded_io_async::Read as _;
    pub use ferrowasp_border::observer::Ledger;
    pub use ferrowasp_boundary_contracts::messages as observed;
    pub use ferrowasp_feedback::esc_manager as esc;
    pub use ferrowasp_flight_control::drone_toolbox as dt;
    pub use ferrowasp_io_contracts::link::{LinkConsumer, LinkProducer, Stamped};
    pub use ferrowasp_io_contracts::time::TimestampMicros;
    pub use ferrowasp_mpu6500 as imu;
    pub use ferrowasp_safety as safety;
    pub use ferrowasp_safety::signals;
    pub use ferrowasp_serial::{Discontinuity, RxChunk, SerialFault};
    pub use ferrowasp_stm32_critical::memory as stm32_memory;
    pub use ferrowasp_stm32_critical::uart_common as stm32_uart;
    pub use fugit::ExtU64 as _;

    /// The storage layout, flight records, command vocabulary and stored
    /// configuration, under the one name the task bodies use.
    pub mod flash_task {
        pub use crate::commands::*;
        pub use ferrowasp_blackbox::records::*;
        pub use ferrowasp_config::stored::*;
        pub use ferrowasp_storage::layout::*;
    }
}
