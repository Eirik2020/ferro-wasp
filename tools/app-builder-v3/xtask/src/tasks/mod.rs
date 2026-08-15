//! Registry of reusable HAL-agnostic task definitions and ordinary Rust bodies.

#[path = "actuator_fault_reporter.rs"]
mod actuator_fault_reporter_task;
#[path = "adc_observation.rs"]
mod adc_observation_task;
#[path = "blink_led.rs"]
mod blink_led_task;
#[path = "dshot_dma.rs"]
mod dshot_dma_task;
#[path = "dshot_service.rs"]
mod dshot_service_task;
#[path = "esc_manager.rs"]
mod esc_manager_task;
#[path = "esc_uart_irq.rs"]
mod esc_uart_irq_task;
#[path = "foxeer_safety_master.rs"]
mod foxeer_safety_master_task;
#[path = "golden_flash.rs"]
mod golden_flash_task;
#[path = "heartbeat.rs"]
mod heartbeat_task;
#[path = "imu_control_bridge.rs"]
mod imu_control_bridge_task;
#[path = "inhibited_actuator.rs"]
mod inhibited_actuator_task;
#[path = "io_watchdog.rs"]
mod io_watchdog_task;
#[path = "msp_osd.rs"]
mod msp_osd_task;
#[path = "observe_button_change.rs"]
mod observe_button_change_task;
#[path = "physical_actuator.rs"]
mod physical_actuator_task;
#[path = "serial_discard.rs"]
mod serial_discard_task;
#[path = "simple_osd.rs"]
mod simple_osd_task;
#[path = "usb_cdc.rs"]
mod usb_cdc_task;

pub use actuator_fault_reporter_task::actuator_fault_reporter;
pub use adc_observation_task::{adc_observation_dma, adc_observation_poll};
pub use blink_led_task::blink_led;
pub use dshot_dma_task::dshot_dma_complete;
pub use dshot_service_task::dshot_service;
pub use esc_manager_task::esc_manager;
pub use esc_uart_irq_task::{esc_uart_rx_dma, esc_uart_rx_idle};
pub use foxeer_safety_master_task::foxeer_safety_master;
pub use golden_flash_task::golden_flash;
pub use heartbeat_task::heartbeat;
pub use imu_control_bridge_task::imu_control_bridge;
pub use inhibited_actuator_task::inhibited_actuator;
pub use io_watchdog_task::io_watchdog;
pub use msp_osd_task::msp_osd;
pub use observe_button_change_task::observe_button_change;
pub use physical_actuator_task::physical_actuator;
pub use serial_discard_task::serial_discard;
pub use simple_osd_task::simple_osd;
pub use usb_cdc_task::usb_cdc;
