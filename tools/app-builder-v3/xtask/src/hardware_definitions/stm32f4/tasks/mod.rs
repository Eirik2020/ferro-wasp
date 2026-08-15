//! Reusable tasks whose contracts depend on STM32F4 hardware capabilities.

#[path = "button_exti.rs"]
mod button_exti_task;
#[path = "foxeer_control.rs"]
mod foxeer_control_task;
#[path = "imu_data_ready.rs"]
mod imu_data_ready_task;
#[path = "periodic_control_tick.rs"]
mod periodic_control_tick_task;
#[path = "serial_rx_bridge.rs"]
mod serial_rx_bridge_task;
#[path = "serial_rx_dma_irq.rs"]
mod serial_rx_dma_irq_task;
#[path = "serial_rx_idle_irq.rs"]
mod serial_rx_idle_irq_task;
#[path = "serial_tx_dma_irq.rs"]
mod serial_tx_dma_irq_task;
#[path = "serial_tx_worker.rs"]
mod serial_tx_worker_task;
#[path = "spi_imu_owner_service.rs"]
mod spi_imu_owner_service_task;
#[path = "spi_imu_parser.rs"]
mod spi_imu_parser_task;
#[path = "spi_imu_poll.rs"]
mod spi_imu_poll_task;
#[path = "spi_imu_rx_dma_irq.rs"]
mod spi_imu_rx_dma_irq_task;
#[path = "spi_imu_timeout.rs"]
mod spi_imu_timeout_task;

pub use button_exti_task::button_exti;
pub use foxeer_control_task::foxeer_control;
pub use imu_data_ready_task::imu_data_ready;
pub use periodic_control_tick_task::periodic_control_tick;
pub use serial_rx_bridge_task::serial_rx_bridge;
pub use serial_rx_dma_irq_task::serial_rx_dma_irq;
pub use serial_rx_idle_irq_task::serial_rx_idle_irq;
pub use serial_tx_dma_irq_task::serial_tx_dma_irq;
pub use serial_tx_worker_task::serial_tx_worker;
pub use spi_imu_owner_service_task::spi_imu_owner_service;
pub use spi_imu_parser_task::spi_imu_parser;
pub use spi_imu_poll_task::spi_imu_poll;
pub use spi_imu_rx_dma_irq_task::spi_imu_rx_dma_irq;
pub use spi_imu_timeout_task::spi_imu_timeout;
