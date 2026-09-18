//! The names task bodies are written against, so a body moves out of an app
//! unchanged. Module aliases only: every type is named by its own crate.

pub use core::sync::atomic::Ordering;
pub use defmt::{info, warn};
pub use embedded_io_async::Read as _;
pub use ferrowasp_core::safety::signals;
pub use ferrowasp_drivers::mpu6500 as imu;
pub use ferrowasp_io_core::serial::{RxChunk, SerialFault};
pub use ferrowasp_io_core::time::TimestampMicros;
pub use ferrowasp_mspv1 as mspv1;
pub use ferrowasp_stm32f4::app_config::ESC_MANAGER_PERIOD_MS;
pub use ferrowasp_stm32f4::hal_prelude::pac;
pub use ferrowasp_stm32f4::memory as stm32_memory;
pub use ferrowasp_stm32f4::uart_dma as stm32_uart;
pub use ferrowasp_tasks::drone_toolbox as dt;
pub use ferrowasp_tasks::esc_manager as esc;
pub use ferrowasp_tasks::osd;
pub use fugit::ExtU32 as _;
