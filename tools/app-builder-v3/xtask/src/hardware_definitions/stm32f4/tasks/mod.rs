//! Reusable tasks whose contracts depend on STM32F4 hardware capabilities.

#[path = "button_exti.rs"]
mod button_exti_task;
#[path = "serial_rx_dma_irq.rs"]
mod serial_rx_dma_irq_task;
#[path = "serial_rx_idle_irq.rs"]
mod serial_rx_idle_irq_task;
#[path = "serial_tx_dma_irq.rs"]
mod serial_tx_dma_irq_task;
#[path = "serial_tx_worker.rs"]
mod serial_tx_worker_task;

pub use button_exti_task::button_exti;
pub use serial_rx_dma_irq_task::serial_rx_dma_irq;
pub use serial_rx_idle_irq_task::serial_rx_idle_irq;
pub use serial_tx_dma_irq_task::serial_tx_dma_irq;
pub use serial_tx_worker_task::serial_tx_worker;
