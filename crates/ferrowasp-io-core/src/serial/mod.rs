mod binding;
mod chunk;
mod discontinuity;
mod fault;
mod profile;
mod reader;
mod routing;
mod tx_chunk;
mod writer;

pub use binding::{
    BindingFault, BindingIssue, ResolvedBindings, SERIAL_PORT_SLOTS, SerialBindings,
    SerialFunction, SerialFunctionSlots, resolve_bindings,
};
pub use chunk::{RxChunk, RxChunkError, RxCompletion};
pub use discontinuity::{Discontinuity, DiscontinuityRecord, StreamGeneration};
pub use fault::SerialFault;
pub use profile::{
    CRSF_FRAME_LEN, LogicalSerialPort, MAVLINK_MIN_FRAME_LEN, MSP_V1_MAX_FRAME_LEN,
    MSP_V1_MAX_PAYLOAD_LEN, SBUS_FRAME_LEN, SerialProfile, SerialProtocol,
};
pub use reader::{
    DiscontinuityReader, SerialReader, SerialRxChannel, SerialRxProducer, SerialRxStatus,
};
pub use routing::{SerialCapabilities, SerialRoute, SerialRouteError};
pub use tx_chunk::{TxChunk, TxChunkError};
pub use writer::{
    SerialTxChannel, SerialTxCompletion, SerialTxOwner, SerialTxStatus, SerialWriter,
};
