//! Registry of reusable task definitions and their colocated bodies.

mod blink;
mod button_exti;
mod report_blink;
mod sbus_parse;
mod uart_rx_dma_irq;
mod uart_rx_idle_irq;

pub use blink::BLINK;
pub use button_exti::BUTTON_EXTI;
pub use report_blink::REPORT_BLINK;
pub use sbus_parse::SBUS_PARSE;
pub use uart_rx_dma_irq::UART_RX_DMA_IRQ;
pub use uart_rx_idle_irq::UART_RX_IDLE_IRQ;
