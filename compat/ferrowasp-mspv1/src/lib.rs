#![no_std]
#![forbid(unsafe_code)]

pub mod commands;
pub mod packet;
pub mod structs;

use commands::*;
pub use packet::MspParser;
use packet::{MAX_FRAME_LEN, MspDirection, MspPacket, encode};
pub use structs::MspOsdTelemetry;

pub const OSD_TX_BUFFER_LEN: usize = MAX_FRAME_LEN;

pub struct MspResponder {
    sequence: u8,
}

impl MspResponder {
    pub const fn new() -> Self {
        Self { sequence: 0 }
    }

    pub fn reply(
        &mut self,
        request: &MspPacket,
        telemetry: &MspOsdTelemetry,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
    ) -> Option<usize> {
        if request.direction != MspDirection::ToFlightController {
            return None;
        }

        let mut payload = [0u8; 64];
        let len = match request.cmd {
            MSP_FC_VERSION => {
                payload[..3].copy_from_slice(&[4, 5, 3]);
                3
            }
            MSP_NAME => {
                let name = b"FERROWASP";
                payload[..name.len()].copy_from_slice(name);
                name.len()
            }
            MSP_STATUS => write_status(&mut payload, telemetry, false),
            MSP_STATUS_EX => write_status(&mut payload, telemetry, true),
            MSP_RC => write_rc(&mut payload, telemetry),
            MSP_ANALOG => write_analog(&mut payload, telemetry),
            MSP_RC_TUNING => write_rc_tuning(&mut payload),
            MSP_PID => write_pid(&mut payload),
            MSP_BATTERY_STATE => write_battery_state(&mut payload, telemetry),
            MSP_FILTER_CONFIG | MSP_PID_ADVANCED => 0,
            _ => 0,
        };

        encode(
            MspDirection::FromFlightController,
            request.cmd,
            &payload[..len],
            output,
        )
        .ok()
    }

    pub fn clear_screen(&mut self, output: &mut [u8; OSD_TX_BUFFER_LEN]) -> Option<usize> {
        encode(
            MspDirection::FromFlightController,
            MSP_OSD_VIDEO_STATUS,
            &[displayport::CLEAR_SCREEN],
            output,
        )
        .ok()
    }

    pub fn draw_screen(&mut self, output: &mut [u8; OSD_TX_BUFFER_LEN]) -> Option<usize> {
        encode(
            MspDirection::FromFlightController,
            MSP_OSD_VIDEO_STATUS,
            &[displayport::DRAW_SCREEN],
            output,
        )
        .ok()
    }

    pub fn heartbeat(&mut self, output: &mut [u8; OSD_TX_BUFFER_LEN]) -> Option<usize> {
        encode(
            MspDirection::FromFlightController,
            MSP_OSD_VIDEO_STATUS,
            &[displayport::HEARTBEAT],
            output,
        )
        .ok()
    }

    pub fn write_string(
        &mut self,
        row: u8,
        col: u8,
        attr: u8,
        text: &[u8],
        output: &mut [u8; OSD_TX_BUFFER_LEN],
    ) -> Option<usize> {
        let max_text_len = output.len().saturating_sub(11).min(30);
        let text_len = text.len().min(max_text_len);
        let mut payload = [0u8; 64];

        payload[0] = displayport::WRITE_STRING;
        payload[1] = row;
        payload[2] = col;
        payload[3] = attr;
        payload[4..4 + text_len].copy_from_slice(&text[..text_len]);
        self.sequence = self.sequence.wrapping_add(1);

        encode(
            MspDirection::FromFlightController,
            MSP_OSD_VIDEO_STATUS,
            &payload[..4 + text_len],
            output,
        )
        .ok()
    }
}

impl Default for MspResponder {
    fn default() -> Self {
        Self::new()
    }
}

fn write_le_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_le_i16(output: &mut [u8], offset: usize, value: i16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_le_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_status(output: &mut [u8], telemetry: &MspOsdTelemetry, extended: bool) -> usize {
    write_le_u16(output, 0, 2500);
    write_le_u16(output, 2, 0);
    output[4] = 0x01;
    output[5] = 0;
    write_le_u32(output, 6, telemetry.armed as u32);
    output[10] = 0;

    if extended {
        write_le_u16(output, 11, 5);
        output[13] = 3;
        output[14] = 0;
        15
    } else {
        11
    }
}

fn write_rc(output: &mut [u8], telemetry: &MspOsdTelemetry) -> usize {
    let channels = [
        telemetry.rc_roll,
        telemetry.rc_pitch,
        telemetry.rc_yaw,
        telemetry.rc_throttle,
        1500,
        1500,
        1500,
        if telemetry.armed { 2000 } else { 1000 },
    ];

    for (index, value) in channels.iter().enumerate() {
        write_le_u16(output, index * 2, *value);
    }

    channels.len() * 2
}

fn write_analog(output: &mut [u8], telemetry: &MspOsdTelemetry) -> usize {
    output[0] = telemetry.battery_voltage_v10;
    write_le_u16(output, 1, telemetry.mah_drawn);
    write_le_u16(output, 3, telemetry.rssi);
    write_le_i16(output, 5, telemetry.amperage_ca);
    7
}

fn write_rc_tuning(output: &mut [u8]) -> usize {
    let data = [
        70, 0, 70, 70, 70, 0, 50, 0, 0xCE, 0x07, 0, 70, 0xCE, 0x07, 0xCE, 0x07, 3, 0, 0, 0, 0, 0, 0,
    ];
    output[..data.len()].copy_from_slice(&data);
    data.len()
}

fn write_pid(output: &mut [u8]) -> usize {
    let data = [45, 80, 40, 47, 84, 46, 45, 80, 0, 50, 75, 75, 40, 0, 0];
    output[..data.len()].copy_from_slice(&data);
    data.len()
}

fn write_battery_state(output: &mut [u8], telemetry: &MspOsdTelemetry) -> usize {
    output[0] = 0;
    write_le_u16(output, 1, 0);
    output[3] = telemetry.battery_voltage_v10;
    write_le_u16(output, 4, telemetry.mah_drawn);
    write_le_i16(output, 6, telemetry.amperage_ca);
    output[8] = 0;
    9
}
