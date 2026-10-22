//! The serial transport every port shares, safety-critical (ADR-0060): which
//! function each port serves, its line settings, and the bounded reader and
//! writer between a port's DMA and its consumer. A port's function is chosen
//! at boot, and the same transport may carry RC, so it is safety-critical for
//! every port.
//!
//! Safety-critical.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

mod binding;
mod profile;
mod reader;
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
pub use routing::{SerialCapabilities, SerialRoute, SerialRouteError};
pub use writer::{
    SerialTxChannel, SerialTxCompletion, SerialTxOwner, SerialTxStatus, SerialWriter,
};
