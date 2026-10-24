//! The names task bodies are written against, so a body moves out of an app
//! unchanged. Module aliases only: every type is named by its own crate.

/// The selected chip family's backend. Only what differs between families
/// is named through it: the HAL's own interrupt and pin types, and the motor
/// bank over the family's timers.
#[cfg(feature = "stm32f405")]
pub use ferrowasp_stm32f4 as backend;
#[cfg(feature = "stm32h743")]
pub use ferrowasp_stm32h7 as backend;

pub use crate::dt;
pub use crate::flash_task;
pub use backend::hal_prelude::pac;
pub use core::sync::atomic::Ordering;
pub use defmt::{info, warn};
pub use embedded_hal::spi::Operation;
pub use embedded_hal_async::spi::SpiDevice as _;
pub use embedded_io_async::Read as _;
pub use ferrowasp_border::observer::Ledger;
pub use ferrowasp_boundary_contracts::messages as observed;
pub use ferrowasp_boundary_contracts::requests;
pub use ferrowasp_feedback::esc_manager as esc;
pub use ferrowasp_feedback::esc_manager::{
    DSHOT_IDLE_QUALIFICATION_CONFIG, DSHOT_PREARM_STOP_HOLD_MS,
};
pub use ferrowasp_flight_control::actuator::{remap_motor_outputs, throttle_to_u16};
pub use ferrowasp_icm42688p as icm;
pub use ferrowasp_imu::spi::{
    AsyncSpiDevice, CriticalSectionSpiExecutor, SharedSpiRequestMailbox, SpiRequestMailbox,
};
pub use ferrowasp_input::rc_receiver::RcReceiver;
pub use ferrowasp_io_contracts::link::{LinkConsumer, LinkProducer, Stamped};
pub use ferrowasp_io_contracts::time::TimestampMicros;
pub use ferrowasp_mpu6000 as mpu6000;
pub use ferrowasp_mpu6500 as imu;
pub use ferrowasp_mspv1 as mspv1;
pub use ferrowasp_osd as osd;
pub use ferrowasp_safety as safety;
pub use ferrowasp_safety::actuator as actuator_task;
pub use ferrowasp_safety::signals;
pub use ferrowasp_serial::{Discontinuity, RxChunk, SerialFault};
pub use ferrowasp_stm32::adc as stm32_adc;
pub use ferrowasp_stm32::adc::Adc1ObservationDma as _;
pub use ferrowasp_stm32::app_config::{
    ARMING_GUARD_POLL_MS, BENCH_EQUAL_MOTOR_MAX_THROTTLE, ESC_MANAGER_PERIOD_MS,
};
pub use ferrowasp_stm32::memory as stm32_memory;
pub use ferrowasp_stm32::memory::{SPI1_JOB_MAX_BYTES, SPI1_JOB_MAX_OPERATIONS};
pub use ferrowasp_stm32::scheduler as stm32_scheduler;
pub use ferrowasp_stm32::spi_common as stm32_spi;
pub use ferrowasp_stm32::spi_common::{SPI_BUFFER_SIZE, SpiDmaService as _};
pub use ferrowasp_stm32::timebase as stm32_timebase;
pub use ferrowasp_stm32::timebase::Timebase as _;
pub use ferrowasp_stm32::uart_common as stm32_uart;
pub use ferrowasp_stm32::uart_common::{UartRxIrqService as _, UartTxDmaService as _};
pub use ferrowasp_stm32::watchdog as stm32_watchdog;
pub use fugit::ExtU64 as _;
