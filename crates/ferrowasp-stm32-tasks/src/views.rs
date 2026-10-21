//! What the command lines report, built from the observer's ledger: the
//! `live`, `esc` and `rc` commands and the USB status line. Each reader
//! copies what it needs out of the ledger under its lock and formats it
//! outside.

use crate::prelude::*;
use ferrowasp_cli::usb_debug::{EscLaneSnapshot, LiveSnapshot};
#[cfg(not(feature = "mspv2_configurator"))]
use ferrowasp_cli::usb_debug::{ImuKind, StatusSnapshot};

/// Tenths of a degree, saturating; non-finite angles report as zero.
pub fn deg10(angle_deg: f32) -> i16 {
    if angle_deg.is_finite() {
        (angle_deg * 10.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16
    } else {
        0
    }
}

/// What the `live` command reports.
pub fn live(ledger: &Ledger) -> LiveSnapshot {
    LiveSnapshot {
        armed: ledger.safety().value.armed,
        arm_switch: ledger.rc().value.arm_high,
        attitude_deg10: ledger.control().value.attitude_deg.map(deg10),
        active_motor_lanes: ledger.output().value.active_lanes,
    }
}

/// What the `esc` command reports, by physical output, at `now_ms` in
/// wrapping milliseconds.
pub fn esc(ledger: &Ledger, now_ms: u32) -> [EscLaneSnapshot; 4] {
    let esc = ledger.esc().value;
    let output = ledger.output().value;
    core::array::from_fn(|index| EscLaneSnapshot {
        uart_erpm_div100: esc.erpm_div100[index].map(|erpm| erpm as u16),
        uart_age_ms: now_ms.wrapping_sub(esc.seen_ms[index]),
        reply_erpm_div100: output.reply_erpm_div100[index],
        replies_valid: output.replies_valid[index],
        replies_invalid: output.replies_invalid[index],
    })
}

/// Every channel of the last accepted receiver frame, microseconds.
pub fn rc_channels(ledger: &Ledger) -> [u16; 16] {
    ledger.rc().value.channels_us
}

/// The RC link as valid and armable at `now_us`: what RC input last
/// reported, and only while its last report is younger than the link's
/// timeout, so a receiver gone quiet reads as lost.
#[cfg(not(feature = "mspv2_configurator"))]
pub fn rc_link(ledger: &Ledger, now_us: u64) -> (bool, bool) {
    let rc = ledger.rc();
    let fresh =
        rc.seq != 0 && now_us.saturating_sub(rc.at.0) <= u64::from(safety::RC_LINK_TIMEOUT_US);
    (fresh && rc.value.valid, fresh && rc.value.armable)
}

/// The detected IMU, as the USB status line names it.
#[cfg(not(feature = "mspv2_configurator"))]
fn imu_kind(kind: u8) -> ImuKind {
    match crate::Spi1ImuKind::from_discriminant(kind) {
        Some(crate::Spi1ImuKind::Mpu6500) => ImuKind::Mpu6500,
        Some(crate::Spi1ImuKind::Icm42688P) => ImuKind::Icm42688P,
        Some(crate::Spi1ImuKind::Mpu6000) => ImuKind::Mpu6000,
        None => ImuKind::None,
    }
}

/// The USB status line.
#[cfg(not(feature = "mspv2_configurator"))]
pub fn status(
    ledger: &Ledger,
    uptime_ms: u32,
    control_loop_hz: u32,
    now_us: u64,
) -> StatusSnapshot {
    let control = ledger.control().value;
    let imu = ledger.imu().value;
    let rc = ledger.rc().value;
    let battery = ledger.battery().value;
    let (rc_valid, rc_armable) = rc_link(ledger, now_us);
    StatusSnapshot {
        uptime_ms,
        imu_kind: imu_kind(ledger.identity().imu_kind),
        imu_ready: ledger.identity().imu_ready,
        imu_sequence: imu.sample_seq,
        gyro_raw: imu.gyro_raw.map(i32::from),
        imu_stale: control.imu_stale,
        control_sequence: control.control_seq,
        control_loop_hz,
        rc_valid,
        rc_armable,
        rc_throttle: rc.throttle,
        rc_arm_high: rc.arm_high,
        system_armed: ledger.safety().value.armed,
        battery_voltage_decivolts: u32::from(battery.voltage_dv),
        battery_current_centiamps: i32::from(battery.current_ca),
        adc_voltage_mv: battery.voltage_pin_mv,
        adc_current_mv: battery.current_pin_mv,
        rc_channels_us: rc.channels_us,
    }
}
