//! Registry of reusable component definitions.

mod command_input;
mod comport;
mod serial_port;

pub use command_input::COMMAND_INPUT_COMPONENT;
pub use comport::COMPORT_COMPONENT;
pub use serial_port::SERIAL_PORT_COMPONENT;
