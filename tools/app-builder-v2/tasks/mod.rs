//! Registry of reusable task definitions and their colocated bodies.

mod blink;
mod button_exti;
mod rc_heartbeat;
mod report_blink;
mod serial_consumer;
mod uart_rx_dma_irq;
mod uart_rx_idle_irq;

pub use blink::BLINK;
pub use button_exti::BUTTON_EXTI;
pub use rc_heartbeat::RC_HEARTBEAT;
pub use report_blink::REPORT_BLINK;
pub use serial_consumer::SERIAL_CONSUMER;
pub use uart_rx_dma_irq::UART_RX_DMA_IRQ;
pub use uart_rx_idle_irq::UART_RX_IDLE_IRQ;
