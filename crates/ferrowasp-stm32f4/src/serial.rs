pub mod endpoint;
pub mod irq_plan;
pub mod rx_state;
pub mod tx_state;

pub use endpoint::{
    UartOwnedRxBridgeOutcome, UartOwnedRxBridgeService, UartRxDeliveryError, UartRxIrqOutcome,
    UartRxIrqService, UartTxDmaError, UartTxDmaService, UartTxIrqOutcome, UartTxStartError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxBufferingMode {
    DoubleBufferedFullChunks,
    ActiveSpareIdleChunks,
}

pub const LIVE_RX_BUFFERING_MODE: RxBufferingMode = RxBufferingMode::ActiveSpareIdleChunks;
pub const TARGET_RX_BUFFERING_MODE: RxBufferingMode = RxBufferingMode::ActiveSpareIdleChunks;
