//! The controller's former home: re-exports `ferrowasp-flight-control`'s
//! drone_toolbox and `ferrowasp-protocol`'s compact rate record under their
//! old path.

#![allow(dead_code)]
pub use ferrowasp_flight_control::drone_toolbox::*;
pub use ferrowasp_protocol::rate_record::*;
