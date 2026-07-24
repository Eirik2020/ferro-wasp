use ferrowasp_serial_osd_compat::{OsdComponent, OsdTelemetryState, SerialRxTx, Usart1RxDma, Usart1TxDma};
use stm32f4xx_hal::{
    dma::StreamsTuple,
    pac::USART1,
    serial::{self, Config as SerialConfig, RxListen, Serial},
};
