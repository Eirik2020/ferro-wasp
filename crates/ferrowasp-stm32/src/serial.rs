pub mod irq_plan;
pub mod rx_state;
pub mod tx_state;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RxBufferingMode {
    DoubleBufferedFullChunks,
    ActiveSpareIdleChunks,
}

pub const LIVE_RX_BUFFERING_MODE: RxBufferingMode = RxBufferingMode::ActiveSpareIdleChunks;
pub const TARGET_RX_BUFFERING_MODE: RxBufferingMode = RxBufferingMode::ActiveSpareIdleChunks;
