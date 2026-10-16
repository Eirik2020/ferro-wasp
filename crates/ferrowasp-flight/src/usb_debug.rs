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

/// What the `live` command reports: arming, attitude and motor activity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LiveSnapshot {
    pub armed: bool,
    pub arm_switch: bool,
    /// Roll, pitch and yaw in tenths of a degree.
    pub attitude_deg10: [i16; 3],
    /// Bit per physical motor output commanded above zero.
    pub active_motor_lanes: u8,
}

/// The `live` answer, one response frame long at any value.
pub fn format_live(
    snapshot: LiveSnapshot,
) -> String<{ crate::flash_storage::USB_RESPONSE_CAPACITY }> {
    let mut line = String::new();
    let [roll, pitch, yaw] = snapshot.attitude_deg10;
    // At most 59 bytes, so the write cannot fail.
    let _ = write!(
        line,
        "OK live armed={} arm_sw={} att={},{},{} mot={}\r\n",
        u8::from(snapshot.armed),
        u8::from(snapshot.arm_switch),
        roll,
        pitch,
        yaw,
        snapshot.active_motor_lanes,
    );
    line
}

/// One physical ESC output's telemetry, as the `esc` command reports it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EscLaneSnapshot {
    /// The latest legacy UART eRPM/100, if one has arrived.
    pub uart_erpm_div100: Option<u16>,
    /// How long ago it arrived.
    pub uart_age_ms: u32,
    /// The latest valid bidirectional reply eRPM/100, if one has arrived.
    pub reply_erpm_div100: Option<u32>,
    pub replies_valid: u32,
    pub replies_invalid: u32,
}

/// The `esc` answer: one line per physical output, 1 to 4, each within one
/// response frame.
///
/// ```text
/// OK esc1 u=66 a=12 b=66 v=48211 x=3
/// ```
///
/// `u` is the UART eRPM/100 and `a` its age in milliseconds (at most 65535),
/// `b` the bidirectional reply eRPM/100, and `v` and `x` the valid and
/// invalid reply counts. A value never received is `-`.
pub fn format_esc(
    lanes: [EscLaneSnapshot; 4],
) -> [String<{ crate::flash_storage::USB_RESPONSE_CAPACITY }>; 4] {
    core::array::from_fn(|index| {
        let lane = lanes[index];
        let mut line = String::new();
        // At most 60 bytes, so the writes cannot fail.
        let _ = write!(line, "OK esc{} u=", index + 1);
        let _ = match lane.uart_erpm_div100 {
            Some(erpm) => write!(line, "{erpm} a={}", lane.uart_age_ms.min(65_535)),
            None => write!(line, "- a=-"),
        };
        let _ = match lane.reply_erpm_div100 {
            // The shortest reply period, 1 us, is 600,000.
            Some(erpm) => write!(line, " b={}", erpm.min(999_999)),
            None => write!(line, " b=-"),
        };
        let _ = write!(
            line,
            " v={} x={}\r\n",
            lane.replies_valid, lane.replies_invalid
        );
        line
    })
}

/// What the firmware was built from: its commit, whether the tree had
/// uncommitted changes, and its enabled features other than the board's.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildIdentity {
    pub revision: &'static str,
    pub dirty: bool,
    /// Comma-separated feature names.
    pub features: &'static str,
}

/// Features `format_build` reports; more are counted but not listed.
pub const BUILD_FEATURE_LINES: usize = 8;

/// The `build` answer: a summary line, then one line per feature, each
/// within one response frame.
///
/// ```text
/// OK build rev=d1f1bc3 dirty=0 features=2
/// OK feature bench_actuator_validation
/// OK feature bench_equal_motors
/// ```
pub fn format_build(
    identity: BuildIdentity,
) -> heapless::Vec<
    String<{ crate::flash_storage::USB_RESPONSE_CAPACITY }>,
    { BUILD_FEATURE_LINES + 1 },
> {
    let features = || identity.features.split(',').filter(|name| !name.is_empty());
    let listed = features().count().min(BUILD_FEATURE_LINES);
    let mut lines = heapless::Vec::new();
    let mut summary = String::new();
    let revision = identity.revision.get(..12).unwrap_or(identity.revision);
    let _ = write!(
        summary,
        "OK build rev={revision} dirty={} features={listed}\r\n",
        u8::from(identity.dirty)
    );
    let _ = lines.push(summary);
    for name in features().take(listed) {
        let mut line = String::new();
        // A name too long for one frame is cut, never dropped, so the count
        // still matches.
        let room = crate::flash_storage::USB_RESPONSE_CAPACITY - "OK feature \r\n".len();
        let _ = write!(line, "OK feature {}\r\n", name.get(..room).unwrap_or(name));
        let _ = lines.push(line);
    }
    lines
}

/// The `rc` answer: channels 1-8, then 9-16, in microseconds, each line
/// within one response frame.
///
/// ```text
/// OK rc1 1500,1500,988,1500,988,988,988,988
/// OK rc2 988,988,988,988,988,988,988,988
/// ```
pub fn format_rc(
    channels: [u16; 16],
) -> [String<{ crate::flash_storage::USB_RESPONSE_CAPACITY }>; 2] {
    core::array::from_fn(|half| {
        let mut line = String::new();
        // At most 59 bytes, so the writes cannot fail.
        let _ = write!(line, "OK rc{} ", half + 1);
        for (index, value) in channels[half * 8..half * 8 + 8].iter().enumerate() {
            let separator = if index == 0 { "" } else { "," };
            let _ = write!(line, "{separator}{value}");
        }
        let _ = line.push_str("\r\n");
        line
    })
}

#[cfg(test)]
mod tests {
    extern crate std;

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

    #[test]
    fn the_widest_live_line_fits_one_response_frame() {
        let line = format_live(LiveSnapshot {
            armed: true,
            arm_switch: true,
            attitude_deg10: [i16::MIN; 3],
            active_motor_lanes: u8::MAX,
        });
        assert!(line.ends_with("mot=255\r\n"), "{line}");
    }

    #[test]
    fn rc_lines_carry_every_channel_within_a_response_frame() {
        let mut channels = [988; 16];
        channels[0] = 1500;
        channels[15] = 2011;
        let [first, second] = format_rc(channels);
        assert_eq!(
            first.as_str(),
            "OK rc1 1500,988,988,988,988,988,988,988\r\n"
        );
        assert_eq!(
            second.as_str(),
            "OK rc2 988,988,988,988,988,988,988,2011\r\n"
        );
        let [widest, _] = format_rc([u16::MAX; 16]);
        assert!(widest.ends_with("65535\r\n"), "{widest}");
    }

    #[test]
    fn esc_lines_report_each_output_and_mark_what_never_arrived() {
        let lanes = [
            EscLaneSnapshot {
                uart_erpm_div100: Some(66),
                uart_age_ms: 12,
                reply_erpm_div100: Some(67),
                replies_valid: 48_211,
                replies_invalid: 3,
            },
            EscLaneSnapshot::default(),
            EscLaneSnapshot {
                reply_erpm_div100: Some(0),
                replies_valid: 9,
                ..EscLaneSnapshot::default()
            },
            EscLaneSnapshot {
                uart_erpm_div100: Some(0),
                uart_age_ms: 900_000,
                ..EscLaneSnapshot::default()
            },
        ];
        let [one, two, three, four] = format_esc(lanes);
        assert_eq!(one.as_str(), "OK esc1 u=66 a=12 b=67 v=48211 x=3\r\n");
        assert_eq!(two.as_str(), "OK esc2 u=- a=- b=- v=0 x=0\r\n");
        assert_eq!(three.as_str(), "OK esc3 u=- a=- b=0 v=9 x=0\r\n");
        assert_eq!(four.as_str(), "OK esc4 u=0 a=65535 b=- v=0 x=0\r\n");
    }

    #[test]
    fn the_widest_esc_line_fits_one_response_frame() {
        let widest = EscLaneSnapshot {
            uart_erpm_div100: Some(u16::MAX),
            uart_age_ms: u32::MAX,
            reply_erpm_div100: Some(u32::MAX),
            replies_valid: u32::MAX,
            replies_invalid: u32::MAX,
        };
        for line in format_esc([widest; 4]) {
            assert!(line.ends_with(" x=4294967295\r\n"), "{line}");
        }
    }

    #[test]
    fn build_lines_list_the_revision_and_each_feature() {
        let lines = format_build(BuildIdentity {
            revision: "d1f1bc3",
            dirty: false,
            features: "bench_actuator_validation,bench_equal_motors",
        });
        let lines: std::vec::Vec<&str> = lines.iter().map(|line| line.as_str()).collect();
        assert_eq!(
            lines,
            [
                "OK build rev=d1f1bc3 dirty=0 features=2\r\n",
                "OK feature bench_actuator_validation\r\n",
                "OK feature bench_equal_motors\r\n",
            ]
        );
    }

    #[test]
    fn a_plain_build_lists_no_features() {
        let lines = format_build(BuildIdentity {
            revision: "unknown",
            dirty: true,
            features: "",
        });
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].as_str(),
            "OK build rev=unknown dirty=1 features=0\r\n"
        );
    }

    #[test]
    fn build_lines_stay_within_a_frame_and_the_count_matches() {
        let long = "a".repeat(100);
        let many = std::vec![long.as_str(); 12].join(",");
        let lines = format_build(BuildIdentity {
            revision: "0123456789abcdef0123",
            dirty: false,
            features: std::boxed::Box::leak(many.into_boxed_str()),
        });
        assert_eq!(lines.len(), BUILD_FEATURE_LINES + 1);
        assert!(lines[0].starts_with("OK build rev=0123456789ab dirty=0 features=8"));
        for line in &lines[1..] {
            assert!(line.ends_with("\r\n"), "{line}");
        }
    }
}
