#![allow(dead_code)]

pub const MSP_NAME: u8 = 10;
pub const MSP_FILTER_CONFIG: u8 = 92;
pub const MSP_PID_ADVANCED: u8 = 94;
pub const MSP_STATUS: u8 = 101;
pub const MSP_RC: u8 = 105;
pub const MSP_ANALOG: u8 = 110;
pub const MSP_RC_TUNING: u8 = 111;
pub const MSP_PID: u8 = 112;
pub const MSP_BATTERY_STATE: u8 = 130;
pub const MSP_STATUS_EX: u8 = 150;
pub const MSP_FC_VERSION: u8 = 3;
pub const MSP_OSD_VIDEO_STATUS: u8 = 182;

pub mod displayport {
    pub const HEARTBEAT: u8 = 0;
    pub const RELEASE: u8 = 1;
    pub const CLEAR_SCREEN: u8 = 2;
    pub const WRITE_STRING: u8 = 3;
    pub const DRAW_SCREEN: u8 = 4;
    pub const OPTIONS: u8 = 5;
}
