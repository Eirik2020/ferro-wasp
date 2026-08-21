//! Reusable tasks whose contracts depend on STM32F4 capabilities.

#[path = "adc_observation.rs"]
mod adc_observation_task;
#[path = "blink_led.rs"]
mod blink_led_task;
#[path = "button_exti.rs"]
mod button_exti_task;
#[path = "dshot_dma.rs"]
mod dshot_dma_task;
#[path = "dshot_service.rs"]
mod dshot_service_task;
#[path = "esc_manager.rs"]
mod esc_manager_task;
#[path = "esc_uart_irq.rs"]
mod esc_uart_irq_task;
#[path = "imu_data_ready.rs"]
mod imu_data_ready_task;
#[path = "io_watchdog.rs"]
mod io_watchdog_task;
#[path = "msp_osd.rs"]
mod msp_osd_task;
#[path = "periodic_control_tick.rs"]
mod periodic_control_tick_task;
#[path = "physical_actuator.rs"]
mod physical_actuator_task;
#[path = "serial_discard.rs"]
mod serial_discard_task;
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
#[path = "simple_osd.rs"]
mod simple_osd_task;
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

pub use adc_observation_task::{adc_observation_dma, adc_observation_poll};
pub use blink_led_task::blink_led;
pub use button_exti_task::button_exti;
pub use dshot_dma_task::dshot_dma_complete;
pub use dshot_service_task::dshot_service;
pub use esc_manager_task::esc_manager;
pub use esc_uart_irq_task::{esc_uart_rx_dma, esc_uart_rx_idle};
pub use imu_data_ready_task::imu_data_ready;
pub use io_watchdog_task::io_watchdog;
pub use msp_osd_task::msp_osd;
pub use periodic_control_tick_task::periodic_control_tick;
pub use physical_actuator_task::physical_actuator;
pub use serial_discard_task::serial_discard;
pub use serial_rx_bridge_task::serial_rx_bridge;
pub use serial_rx_dma_irq_task::serial_rx_dma_irq;
pub use serial_rx_idle_irq_task::serial_rx_idle_irq;
pub use serial_tx_dma_irq_task::serial_tx_dma_irq;
pub use serial_tx_worker_task::serial_tx_worker;
pub use simple_osd_task::simple_osd;
pub use spi_imu_owner_service_task::spi_imu_owner_service;
pub use spi_imu_parser_task::spi_imu_parser;
pub use spi_imu_poll_task::spi_imu_poll;
pub use spi_imu_rx_dma_irq_task::spi_imu_rx_dma_irq;
pub use spi_imu_timeout_task::spi_imu_timeout;
