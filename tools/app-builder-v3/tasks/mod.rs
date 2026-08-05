//! Registry of reusable task definitions and their colocated bodies.

mod blink;
mod button_exti;
mod command_input;
mod comport;
mod rc_heartbeat;
mod report_blink;
mod uart_rx_dma_irq;
mod uart_rx_idle_irq;

pub use blink::BLINK;
pub use button_exti::BUTTON_EXTI;
pub use command_input::COMMAND_INPUT;
pub use comport::COMPORT;
pub use rc_heartbeat::RC_HEARTBEAT;
pub use report_blink::REPORT_BLINK;
pub use uart_rx_dma_irq::UART_RX_DMA_IRQ;
pub use uart_rx_idle_irq::UART_RX_IDLE_IRQ;
