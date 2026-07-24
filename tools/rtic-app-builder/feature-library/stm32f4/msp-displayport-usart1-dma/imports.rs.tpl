use ferrowasp_serial_osd_compat::{
    OsdComponent, OsdFaultId, OsdFaultState, OsdTelemetryState, OsdWork, RxServiceResult,
    SerialChunk, TxCompletion, Usart1RxDma, Usart1TxDma,
};
use rtic_sync::{
    channel::{Receiver, Sender, TrySendError},
    make_channel,
};
use stm32f4xx_hal::{
    dma::StreamsTuple,
    pac::USART1,
    serial::{self, Config as SerialConfig, RxListen, Serial},
};
