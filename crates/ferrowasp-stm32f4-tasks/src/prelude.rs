//! The names task bodies are written against, so a body moves out of an app
//! unchanged. Module aliases only: every type is named by its own crate.

pub use core::sync::atomic::Ordering;
pub use defmt::{info, warn};
pub use embedded_hal::spi::Operation;
pub use embedded_hal_async::spi::SpiDevice as _;
pub use embedded_io_async::Read as _;
pub use ferrowasp_core::actuator::{remap_motor_outputs, throttle_to_u16};
pub use ferrowasp_core::safety;
pub use ferrowasp_core::safety::signals;
pub use ferrowasp_drivers::icm42688p as icm;
pub use ferrowasp_drivers::mpu6500 as imu;
pub use ferrowasp_io_core::serial::{Discontinuity, RxChunk, SerialFault};
pub use ferrowasp_io_core::spi::{
    AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiRequestMailbox,
};
pub use ferrowasp_io_core::time::TimestampMicros;
pub use ferrowasp_mspv1 as mspv1;
pub use ferrowasp_stm32f4::adc as stm32_adc;
pub use ferrowasp_stm32f4::adc::Adc1ObservationDma as _;
pub use ferrowasp_stm32f4::app_config::{
    ARMING_GUARD_POLL_MS, BENCH_EQUAL_MOTOR_MAX_THROTTLE, ESC_MANAGER_PERIOD_MS,
};
pub use ferrowasp_stm32f4::hal_prelude::{ExtiPin, VtempCal30, VtempCal110, pac};
pub use ferrowasp_stm32f4::memory as stm32_memory;
pub use ferrowasp_stm32f4::memory::{SPI1_JOB_MAX_BYTES, SPI1_JOB_MAX_OPERATIONS};
pub use ferrowasp_stm32f4::scheduler as stm32_scheduler;
pub use ferrowasp_stm32f4::spi_dma as stm32_spi;
pub use ferrowasp_stm32f4::spi_dma::{SPI_BUFFER_SIZE, SpiDmaService as _};
pub use ferrowasp_stm32f4::timebase as stm32_timebase;
pub use ferrowasp_stm32f4::timebase::Timebase as _;
pub use ferrowasp_stm32f4::uart_dma as stm32_uart;
pub use ferrowasp_stm32f4::watchdog as stm32_watchdog;
pub use ferrowasp_tasks::actuator as actuator_task;
pub use ferrowasp_tasks::drone_toolbox as dt;
pub use ferrowasp_tasks::esc_manager as esc;
pub use ferrowasp_tasks::esc_manager::{
    DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_PREARM_STOP_HOLD_MS,
};
pub use ferrowasp_tasks::flash_storage as flash_task;
pub use ferrowasp_tasks::osd;
pub use fugit::ExtU32 as _;
pub use sbus_rs::StreamingParser;
