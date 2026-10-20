mod binding;
mod profile;
mod reader;
mod reply;
mod routing;
mod writer;

pub use binding::{
    BindingFault, BindingIssue, RcProtocol, ResolvedBindings, SERIAL_PORT_SLOTS, SerialBindings,
    SerialFunction, SerialFunctionSlots, resolve_bindings,
};
pub use ferrowasp_io_contracts::serial::{
    Discontinuity, DiscontinuityRecord, RxChunk, RxChunkError, RxCompletion, SerialFault,
    StreamGeneration, TxChunk, TxChunkError,
};
pub use profile::{
    CRSF_FRAME_LEN, LogicalSerialPort, MAVLINK_MIN_FRAME_LEN, MSP_V1_MAX_FRAME_LEN,
    MSP_V1_MAX_PAYLOAD_LEN, SBUS_FRAME_LEN, SerialProfile, SerialProtocol,
};
pub use reader::{
    DiscontinuityReader, SerialReader, SerialRxChannel, SerialRxProducer, SerialRxStatus,
};
pub use reply::{SERIAL_REPLY_LINE_CAPACITY, write_serial_bindings};
pub use routing::{SerialCapabilities, SerialRoute, SerialRouteError};
pub use writer::{
    SerialTxChannel, SerialTxCompletion, SerialTxOwner, SerialTxStatus, SerialWriter,
};
