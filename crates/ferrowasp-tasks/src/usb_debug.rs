use core::fmt::{self, Write};

use heapless::String;

/// Bounds one status frame.
///
/// The worst case - every numeric field at its extreme - is 270 bytes with the
/// terminator, which the test below pins. Only this crate's stack buffer is
/// sized from it, so the headroom is cheap.
pub const STATUS_LINE_CAPACITY: usize = 400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImuKind {
    None,
    Mpu6500,
    Icm42688P,
    Mpu6000,
}

impl ImuKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Mpu6500 => "mpu6500",
            Self::Icm42688P => "icm42688p",
            Self::Mpu6000 => "mpu6000",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusSnapshot {
    pub uptime_ms: u32,
    pub imu_kind: ImuKind,
    pub imu_ready: bool,
    pub imu_sequence: u32,
    pub gyro_raw: [i32; 3],
    pub imu_stale: bool,
    pub control_sequence: u32,
    /// The control loop rate the firmware is built for, in hertz.
    ///
    /// Reported so a host does not have to keep its own copy, which has
    /// already been wrong once. Comparing it against the rate implied by
    /// `control_sequence` over `uptime_ms` says whether the loop is keeping
    /// up - the measurement a loop-rate change needs.
    pub control_loop_hz: u32,
    pub rc_valid: bool,
    pub rc_armable: bool,
    pub rc_throttle: u32,
    pub rc_arm_high: bool,
    pub system_armed: bool,
    pub battery_voltage_decivolts: u32,
    pub battery_current_centiamps: i32,
    pub adc_voltage_mv: u32,
    pub adc_current_mv: u32,
    /// The last accepted receiver frame's channels, in microseconds.
    pub rc_channels_us: [u16; 16],
}

pub fn format_status(snapshot: StatusSnapshot) -> Result<String<STATUS_LINE_CAPACITY>, fmt::Error> {
    let mut line = String::new();
    write!(
        line,
        "FWDBG1 ms={} imu={} ready={} seq={} gyro={},{},{} stale={} ctl={} ctl_hz={} rc={} armable={} thr={} arm_sw={} armed={} vbat_dV={} current_cA={} adc_v_mV={} adc_i_mV={} ch=",
        snapshot.uptime_ms,
        snapshot.imu_kind.as_str(),
        u8::from(snapshot.imu_ready),
        snapshot.imu_sequence,
        snapshot.gyro_raw[0],
        snapshot.gyro_raw[1],
        snapshot.gyro_raw[2],
        u8::from(snapshot.imu_stale),
        snapshot.control_sequence,
        snapshot.control_loop_hz,
        u8::from(snapshot.rc_valid),
        u8::from(snapshot.rc_armable),
        snapshot.rc_throttle,
        u8::from(snapshot.rc_arm_high),
        u8::from(snapshot.system_armed),
        snapshot.battery_voltage_decivolts,
        snapshot.battery_current_centiamps,
        snapshot.adc_voltage_mv,
        snapshot.adc_current_mv,
    )?;
    for (index, channel) in snapshot.rc_channels_us.iter().enumerate() {
        let separator = if index == 0 { "" } else { "," };
        write!(line, "{separator}{channel}")?;
    }
    line.push_str("\r\n").map_err(|_| fmt::Error)?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn representative_snapshot() -> StatusSnapshot {
        StatusSnapshot {
            uptime_ms: 12_345,
            imu_kind: ImuKind::Icm42688P,
            imu_ready: true,
            imu_sequence: 9_876,
            gyro_raw: [-17, 4, -70],
            imu_stale: false,
            control_sequence: 4_938,
            control_loop_hz: 1_000,
            rc_valid: true,
            rc_armable: true,
            rc_throttle: 1_000,
            rc_arm_high: false,
            system_armed: false,
            battery_voltage_decivolts: 230,
            battery_current_centiamps: -12,
            adc_voltage_mv: 2_091,
            adc_current_mv: 1_234,
            rc_channels_us: [
                1500, 1500, 988, 1500, 2012, 988, 988, 988, 988, 988, 988, 988, 988, 988, 988, 988,
            ],
        }
    }

    #[test]
    fn status_frame_is_versioned_self_describing_ascii() {
        let line = format_status(representative_snapshot()).unwrap();

        assert_eq!(
            line.as_str(),
            "FWDBG1 ms=12345 imu=icm42688p ready=1 seq=9876 gyro=-17,4,-70 stale=0 ctl=4938 ctl_hz=1000 rc=1 armable=1 thr=1000 arm_sw=0 armed=0 vbat_dV=230 current_cA=-12 adc_v_mV=2091 adc_i_mV=1234 ch=1500,1500,988,1500,2012,988,988,988,988,988,988,988,988,988,988,988\r\n"
        );
        assert!(line.is_ascii());
    }

    #[test]
    fn every_imu_kind_has_a_stable_wire_name() {
        let mut snapshot = representative_snapshot();

        for (kind, expected) in [
            (ImuKind::None, "imu=none"),
            (ImuKind::Mpu6500, "imu=mpu6500"),
            (ImuKind::Icm42688P, "imu=icm42688p"),
            (ImuKind::Mpu6000, "imu=mpu6000"),
        ] {
            snapshot.imu_kind = kind;
            assert!(format_status(snapshot).unwrap().contains(expected));
        }
    }

    #[test]
    fn worst_case_numeric_values_fit_the_bounded_frame() {
        let line = format_status(StatusSnapshot {
            uptime_ms: u32::MAX,
            imu_kind: ImuKind::Icm42688P,
            imu_ready: true,
            imu_sequence: u32::MAX,
            gyro_raw: [i32::MIN, i32::MIN, i32::MIN],
            imu_stale: true,
            control_sequence: u32::MAX,
            control_loop_hz: u32::MAX,
            rc_valid: true,
            rc_armable: true,
            rc_throttle: u32::MAX,
            rc_arm_high: true,
            system_armed: true,
            battery_voltage_decivolts: u32::MAX,
            battery_current_centiamps: i32::MIN,
            adc_voltage_mv: u32::MAX,
            adc_current_mv: u32::MAX,
            rc_channels_us: [u16::MAX; 16],
        })
        .unwrap();

        assert!(line.len() <= STATUS_LINE_CAPACITY);
        assert!(line.ends_with("\r\n"));
    }
}
