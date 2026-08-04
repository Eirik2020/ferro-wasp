mod assignment;
mod chunk;
mod discontinuity;
mod fault;
mod profile;
mod rc_input;
mod reader;
mod routing;
mod tx_chunk;
mod writer;

pub use assignment::{RcProtocol, SerialPortAssignment};
pub use chunk::{RxChunk, RxChunkError, RxCompletion};
pub use discontinuity::{Discontinuity, DiscontinuityRecord, StreamGeneration};
pub use fault::SerialFault;
pub use profile::{
    CRSF_FRAME_LEN, LogicalSerialPort, MAVLINK_MIN_FRAME_LEN, MSP_V1_MAX_FRAME_LEN,
    MSP_V1_MAX_PAYLOAD_LEN, RAW_LINE_MAX_LEN, SBUS_FRAME_LEN, SerialProfile, SerialProtocol,
};
pub use rc_input::RcInputSnapshot;
pub use reader::{
    DiscontinuityReader, SerialReader, SerialRxChannel, SerialRxProducer, SerialRxStatus,
};
pub use routing::{
    SerialCapabilities, SerialRoute, SerialRouteError, UART1_CONSUMER, UART2_CONSUMER,
    UART3_CONSUMER, UART4_CONSUMER, UartConsumer, route_uart_to_task,
};
pub use tx_chunk::{TxChunk, TxChunkError};
pub use writer::{
    SerialTxChannel, SerialTxCompletion, SerialTxOwner, SerialTxStatus, SerialWriter,
};
