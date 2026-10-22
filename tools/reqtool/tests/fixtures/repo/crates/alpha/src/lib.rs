//! Safety-critical.

pub mod input;
pub mod hidden;

pub use hidden::reexported;
pub use hidden::{self as renamed_module, Original as Renamed};
pub use input::nested::*;
