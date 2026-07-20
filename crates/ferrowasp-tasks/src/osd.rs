use crate::drone_toolbox::TuningProfile;
use ferrowasp_mspv1::{MspOsdTelemetry, MspParser, MspResponder, OSD_TX_BUFFER_LEN};
use heapless::spsc::Queue;

pub const OSD_TX_QUEUE_CAP: usize = 16;
const MENU_ITEMS: u8 = 10;
const MENU_STICK_THRESHOLD_DPS: i16 = 600;
const MENU_THROTTLE_MAX: u32 = 300;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OsdStickRates {
    pub roll: i16,
    pub pitch: i16,
    pub yaw: i16,
}

#[derive(Clone, Copy)]
pub struct OsdTxFrame {
    pub bytes: [u8; OSD_TX_BUFFER_LEN],
}

impl OsdTxFrame {
    pub fn new(frame: &[u8]) -> Self {
        let mut bytes = [0; OSD_TX_BUFFER_LEN];
        let len = frame.len().min(bytes.len());
        bytes[..len].copy_from_slice(&frame[..len]);
        Self { bytes }
    }
}

pub fn enqueue_tx_frame(queue: &mut Queue<OsdTxFrame, OSD_TX_QUEUE_CAP>, bytes: &[u8]) {
    if queue.enqueue(OsdTxFrame::new(bytes)).is_err() {
        let _ = queue.dequeue();
        let _ = queue.enqueue(OsdTxFrame::new(bytes));
    }
}

pub struct OsdTask {
    parser: MspParser,
    responder: MspResponder,
    overlay_step: u8,
    menu_active: bool,
    menu_row: u8,
    menu_debounce_ticks: u8,
}

impl OsdTask {
    pub const fn new() -> Self {
        Self {
            parser: MspParser::new(),
            responder: MspResponder::new(),
            overlay_step: 0,
            menu_active: false,
            menu_row: 0,
            menu_debounce_ticks: 0,
        }
    }

    pub fn ingest_byte(
        &mut self,
        byte: u8,
        telemetry: &MspOsdTelemetry,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
    ) -> Option<usize> {
        match self.parser.parse(byte) {
            Ok(Some(packet)) => self.responder.reply(&packet, telemetry, output),
            Ok(None) | Err(_) => None,
        }
    }

    pub fn next_overlay_frame(
        &mut self,
        telemetry: &MspOsdTelemetry,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
    ) -> Option<usize> {
        let len = match self.overlay_step {
            0 => self.responder.clear_screen(output),
            1 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"FERROWASP");
                self.responder
                    .write_string(1, 2, 0, trim_end(&text), output)
            }
            2 => {
                let mut text = [b' '; 18];
                if telemetry.armed {
                    copy_text(&mut text, b"ARMED");
                } else {
                    copy_text(&mut text, b"DISARMED");
                }
                self.responder
                    .write_string(2, 2, 0, trim_end(&text), output)
            }
            3 => {
                let mut text = [b' '; 18];
                if telemetry.imu_stale {
                    copy_text(&mut text, b"IMU STALE");
                } else {
                    copy_text(&mut text, b"IMU OK");
                }
                self.responder
                    .write_string(3, 2, 0, trim_end(&text), output)
            }
            4 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"VBAT ");
                write_vbat(&mut text[5..], telemetry.battery_voltage_v10);
                self.responder
                    .write_string(4, 2, 0, trim_end(&text), output)
            }
            5 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"CELL ");
                write_cell_voltage(
                    &mut text[5..],
                    telemetry.battery_cell_count,
                    telemetry.battery_cell_voltage_v100,
                );
                self.responder
                    .write_string(5, 2, 0, trim_end(&text), output)
            }
            6 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"CUR ");
                write_current(&mut text[4..], telemetry.amperage_ca);
                self.responder
                    .write_string(6, 2, 0, trim_end(&text), output)
            }
            7 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"THR ");
                write_u16(&mut text[4..], telemetry.osd_throttle);
                self.responder
                    .write_string(7, 2, 0, trim_end(&text), output)
            }
            8..=13 => self.write_blank_debug_row(self.overlay_step, output),
            14 => self.responder.draw_screen(output),
            _ => self.responder.heartbeat(output),
        };

        self.overlay_step = (self.overlay_step + 1) % 15;
        len
    }

    pub fn update_menu(
        &mut self,
        armed: bool,
        rates: OsdStickRates,
        throttle: u32,
        tuning: &mut TuningProfile,
    ) -> bool {
        if armed {
            self.menu_active = false;
            self.menu_debounce_ticks = 0;
            return false;
        }

        if self.menu_debounce_ticks > 0 {
            self.menu_debounce_ticks -= 1;
        }

        if !self.menu_active {
            if throttle <= MENU_THROTTLE_MAX
                && rates.yaw < -MENU_STICK_THRESHOLD_DPS
                && rates.pitch > MENU_STICK_THRESHOLD_DPS
                && self.menu_debounce_ticks == 0
            {
                self.menu_active = true;
                self.menu_row = 0;
                self.menu_debounce_ticks = 20;
            }

            return self.menu_active;
        }

        if self.menu_debounce_ticks != 0 {
            return true;
        }

        if rates.yaw > MENU_STICK_THRESHOLD_DPS {
            self.menu_active = false;
            self.menu_debounce_ticks = 20;
            return false;
        }

        if rates.pitch > MENU_STICK_THRESHOLD_DPS {
            self.menu_row = self.menu_row.saturating_sub(1);
            self.menu_debounce_ticks = 12;
        } else if rates.pitch < -MENU_STICK_THRESHOLD_DPS {
            self.menu_row = (self.menu_row + 1).min(MENU_ITEMS - 1);
            self.menu_debounce_ticks = 12;
        } else if rates.roll > MENU_STICK_THRESHOLD_DPS {
            adjust_menu_value(tuning, self.menu_row, 1);
            self.menu_debounce_ticks = 8;
        } else if rates.roll < -MENU_STICK_THRESHOLD_DPS {
            adjust_menu_value(tuning, self.menu_row, -1);
            self.menu_debounce_ticks = 8;
        }

        true
    }

    pub fn next_menu_frame(
        &mut self,
        tuning: &TuningProfile,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
    ) -> Option<usize> {
        if !self.menu_active {
            return None;
        }

        let len = match self.overlay_step {
            0 => self.responder.clear_screen(output),
            1 => self
                .responder
                .write_string(1, 2, 0, b"FERROWASP SETUP", output),
            2..=11 => {
                let index = self.overlay_step - 2;
                let mut text = [b' '; 24];
                write_menu_item(&mut text, index, tuning, self.menu_row == index);
                self.responder
                    .write_string(self.overlay_step, 2, 0, trim_end(&text), output)
            }
            12 => self
                .responder
                .write_string(13, 2, 0, b"YAW RIGHT: EXIT", output),
            13 => self
                .responder
                .write_string(14, 2, 0, b"DISARMED ONLY", output),
            14 => self.responder.draw_screen(output),
            _ => self.responder.heartbeat(output),
        };

        self.overlay_step = (self.overlay_step + 1) % 15;
        len
    }

    pub fn heartbeat_frame(&mut self, output: &mut [u8; OSD_TX_BUFFER_LEN]) -> Option<usize> {
        self.responder.heartbeat(output)
    }

    fn write_blank_debug_row(
        &mut self,
        row: u8,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
    ) -> Option<usize> {
        let text = [b' '; 18];
        self.responder.write_string(row, 2, 0, &text, output)
    }

    pub fn overlay_update<F>(
        &mut self,
        telemetry: &MspOsdTelemetry,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
        mut enqueue: F,
    ) where
        F: FnMut(&[u8]),
    {
        let mut text = [b' '; 18];
        if telemetry.armed {
            copy_text(&mut text, b"ARMED");
        } else {
            copy_text(&mut text, b"DISARMED");
        }
        if let Some(len) = self
            .responder
            .write_string(2, 2, 0, trim_end(&text), output)
        {
            enqueue(&output[..len]);
        }

        text.fill(b' ');
        if telemetry.imu_stale {
            copy_text(&mut text, b"IMU STALE");
        } else {
            copy_text(&mut text, b"IMU OK");
        }
        if let Some(len) = self
            .responder
            .write_string(3, 2, 0, trim_end(&text), output)
        {
            enqueue(&output[..len]);
        }

        text.fill(b' ');
        copy_text(&mut text, b"VBAT ");
        write_vbat(&mut text[5..], telemetry.battery_voltage_v10);
        if let Some(len) = self
            .responder
            .write_string(4, 2, 0, trim_end(&text), output)
        {
            enqueue(&output[..len]);
        }

        text.fill(b' ');
        copy_text(&mut text, b"CELL ");
        write_cell_voltage(
            &mut text[5..],
            telemetry.battery_cell_count,
            telemetry.battery_cell_voltage_v100,
        );
        if let Some(len) = self
            .responder
            .write_string(5, 2, 0, trim_end(&text), output)
        {
            enqueue(&output[..len]);
        }

        text.fill(b' ');
        copy_text(&mut text, b"CUR ");
        write_current(&mut text[4..], telemetry.amperage_ca);
        if let Some(len) = self
            .responder
            .write_string(6, 2, 0, trim_end(&text), output)
        {
            enqueue(&output[..len]);
        }

        text.fill(b' ');
        copy_text(&mut text, b"THR ");
        write_u16(&mut text[4..], telemetry.osd_throttle);
        if let Some(len) = self
            .responder
            .write_string(7, 2, 0, trim_end(&text), output)
        {
            enqueue(&output[..len]);
        }

        for row in 8..=15 {
            if let Some(len) = self.write_blank_debug_row(row, output) {
                enqueue(&output[..len]);
            }
        }

        if let Some(len) = self.responder.draw_screen(output) {
            enqueue(&output[..len]);
        }
    }
}

impl Default for OsdTask {
    fn default() -> Self {
        Self::new()
    }
}

pub fn map_throttle_to_msp_rc(throttle: u32) -> u16 {
    let value = 1000u32.saturating_add(throttle / 2);
    value.min(2000) as u16
}

pub fn map_rate_to_msp_rc(rate: i16) -> u16 {
    let value = 1500i32 + (rate as i32 / 2);
    value.clamp(1000, 2000) as u16
}

pub fn pack_millivolts_to_cell_centivolts(pack_mv: u32, cell_count: u8) -> u16 {
    if cell_count == 0 {
        return 0;
    }

    let cell_mv = pack_mv / cell_count as u32;
    ((cell_mv + 5) / 10).min(u16::MAX as u32) as u16
}

pub fn current_sample_to_centiamps(adc_mv: u32, betaflight_scale: u32) -> i16 {
    if betaflight_scale == 0 {
        return 0;
    }

    ((adc_mv * 100 + (betaflight_scale / 2)) / betaflight_scale).min(i16::MAX as u32) as i16
}

fn adjust_menu_value(tuning: &mut TuningProfile, row: u8, direction: i8) {
    let step = if direction >= 0 { 0.1 } else { -0.1 };
    let lpf_step = if direction >= 0 { 0.05 } else { -0.05 };

    match row {
        0 => tuning.rate_gains.roll.p += step,
        1 => tuning.rate_gains.roll.i += step,
        2 => tuning.rate_gains.roll.d += step,
        3 => tuning.rate_gains.pitch.p += step,
        4 => tuning.rate_gains.pitch.i += step,
        5 => tuning.rate_gains.pitch.d += step,
        6 => tuning.rate_gains.yaw.p += step,
        7 => tuning.rate_gains.yaw.i += step,
        8 => tuning.rate_gains.yaw.d += step,
        9 => tuning.imu_lpf_alpha += lpf_step,
        _ => {}
    }

    *tuning = tuning.sanitized();
}

fn write_menu_item(output: &mut [u8], index: u8, tuning: &TuningProfile, selected: bool) {
    output[0] = if selected { b'>' } else { b' ' };
    output[1] = b' ';

    let (label, value) = match index {
        0 => (b"ROLL P " as &[u8], tuning.rate_gains.roll.p),
        1 => (b"ROLL I " as &[u8], tuning.rate_gains.roll.i),
        2 => (b"ROLL D " as &[u8], tuning.rate_gains.roll.d),
        3 => (b"PITCH P " as &[u8], tuning.rate_gains.pitch.p),
        4 => (b"PITCH I " as &[u8], tuning.rate_gains.pitch.i),
        5 => (b"PITCH D " as &[u8], tuning.rate_gains.pitch.d),
        6 => (b"YAW P " as &[u8], tuning.rate_gains.yaw.p),
        7 => (b"YAW I " as &[u8], tuning.rate_gains.yaw.i),
        8 => (b"YAW D " as &[u8], tuning.rate_gains.yaw.d),
        9 => (b"IMU LPF " as &[u8], tuning.imu_lpf_alpha),
        _ => (b"" as &[u8], 0.0),
    };

    copy_text(&mut output[2..], label);
    let value_offset = 2 + label.len();
    write_f32_x10(&mut output[value_offset..], value);
}

fn write_f32_x10(output: &mut [u8], value: f32) {
    let value_x10 = if !value.is_finite() {
        0
    } else {
        (value * 10.0 + 0.5) as u16
    };
    let whole = value_x10 / 10;
    let frac = value_x10 % 10;
    let mut pos = write_u16(output, whole);

    if pos + 2 <= output.len() {
        output[pos] = b'.';
        pos += 1;
        output[pos] = b'0' + frac as u8;
    }
}

fn copy_text(output: &mut [u8], text: &[u8]) {
    let len = output.len().min(text.len());
    output[..len].copy_from_slice(&text[..len]);
}

fn trim_end(text: &[u8]) -> &[u8] {
    let mut len = text.len();
    while len > 0 && text[len - 1] == b' ' {
        len -= 1;
    }
    &text[..len]
}

fn write_vbat(output: &mut [u8], voltage_v10: u8) {
    let whole = voltage_v10 / 10;
    let frac = voltage_v10 % 10;
    let mut pos = write_u16(output, whole as u16);
    if pos + 2 <= output.len() {
        output[pos] = b'.';
        pos += 1;
        output[pos] = b'0' + frac;
    }
}

fn write_cell_voltage(output: &mut [u8], cell_count: u8, voltage_v100: u16) {
    if cell_count == 0 || voltage_v100 == 0 {
        copy_text(output, b"---");
        return;
    }

    let whole = voltage_v100 / 100;
    let frac = voltage_v100 % 100;
    let mut pos = write_u16(output, whole);
    if pos + 3 <= output.len() {
        output[pos] = b'.';
        pos += 1;
        output[pos] = b'0' + (frac / 10) as u8;
        pos += 1;
        output[pos] = b'0' + (frac % 10) as u8;
    }
}

fn write_current(output: &mut [u8], amperage_ca: i16) {
    if amperage_ca < 0 {
        copy_text(output, b"---");
        return;
    }

    let amperage_ca = amperage_ca as u16;
    let whole = amperage_ca / 100;
    let frac = (amperage_ca % 100) / 10;
    let mut pos = write_u16(output, whole);
    if pos + 3 <= output.len() {
        output[pos] = b'.';
        pos += 1;
        output[pos] = b'0' + frac as u8;
        pos += 1;
        output[pos] = b'A';
    }
}

fn write_u16(output: &mut [u8], value: u16) -> usize {
    let mut digits = [0u8; 5];
    let mut value = value;
    let mut count = 0;

    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 || count == digits.len() {
            break;
        }
    }

    let len = output.len().min(count);
    for i in 0..len {
        output[i] = digits[count - 1 - i];
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc_values_pack_to_msp_rc_range() {
        assert_eq!(map_throttle_to_msp_rc(0), 1000);
        assert_eq!(map_throttle_to_msp_rc(1000), 1500);
        assert_eq!(map_throttle_to_msp_rc(2500), 2000);

        assert_eq!(map_rate_to_msp_rc(0), 1500);
        assert_eq!(map_rate_to_msp_rc(1000), 2000);
        assert_eq!(map_rate_to_msp_rc(-1000), 1000);
    }

    #[test]
    fn adc_values_pack_to_osd_units() {
        assert_eq!(pack_millivolts_to_cell_centivolts(25_200, 6), 420);
        assert_eq!(pack_millivolts_to_cell_centivolts(0, 0), 0);
        assert_eq!(current_sample_to_centiamps(700, 70), 1000);
        assert_eq!(current_sample_to_centiamps(700, 0), 0);
    }

    #[test]
    fn tx_frame_queue_drops_oldest_when_full() {
        let mut queue = Queue::<OsdTxFrame, OSD_TX_QUEUE_CAP>::new();
        let usable_capacity = OSD_TX_QUEUE_CAP - 1;

        for value in 0..usable_capacity {
            enqueue_tx_frame(&mut queue, &[value as u8]);
        }
        enqueue_tx_frame(&mut queue, &[0xaa]);

        assert_eq!(queue.len(), usable_capacity);
        assert_eq!(queue.dequeue().unwrap().bytes[0], 1);

        let mut last = 0;
        while let Some(frame) = queue.dequeue() {
            last = frame.bytes[0];
        }
        assert_eq!(last, 0xaa);
    }
}
