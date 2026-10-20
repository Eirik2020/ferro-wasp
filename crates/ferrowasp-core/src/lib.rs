#![forbid(unsafe_code)]
#![no_std]

pub use ferrowasp_flight_control::actuator;
pub mod blackbox {
    //! The flight-log format and the configuration slot page, re-exported from
    //! `ferrowasp-protocol` and `ferrowasp-config` under their former path.
    pub use ferrowasp_config::page::{
        CONFIG_PAYLOAD_MAX, ConfigError, FLASH_PAGE_LEN, crc32, decode_config_page,
        encode_config_page,
    };
    pub use ferrowasp_protocol::blackbox::*;
}
pub use ferrowasp_config::keys as config;
pub use ferrowasp_control::frames;
pub mod safety;
