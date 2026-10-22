//! The stored configuration: its schema versions, encoding, decoding and the
//! migration of every older payload.
//!
//! Requirements: CONFIG-001, CONFIG-002, CONFIG-003, CONFIG-005
//! Safety-critical.

use ferrowasp_control::calibration::accel_level::AccelLevel;
use ferrowasp_control::filter::gyro_lpf_corner_hz;
use ferrowasp_flight_control::actuator::MotorOutputMap;
use ferrowasp_flight_control::drone_toolbox::{
    ActualRateAxis, PidGains, RC_RATE_PROFILE, RateControllerGains, RcRateProfile, TuningProfile,
};
use ferrowasp_input::channel_map::RcChannelMap;
use ferrowasp_serial::{RcProtocol, SERIAL_PORT_SLOTS, SerialBindings};

use crate::keys::ConfigKey;

pub const LEGACY_STORED_CONFIG_LEN: usize = 44;
/// Versions 2 and 3.
pub const V3_STORED_CONFIG_LEN: usize = 84;
/// Version 4.
pub const V4_STORED_CONFIG_LEN: usize = V3_STORED_CONFIG_LEN + SERIAL_PORT_SLOTS;
/// Where version 5's receiver fields start: stick order (two bytes), arm
/// channel, and RC protocol.
const RC_FIELDS_OFFSET: usize = V4_STORED_CONFIG_LEN;
/// Version 5.
pub const V5_STORED_CONFIG_LEN: usize = RC_FIELDS_OFFSET + 4;
/// Where version 6's motor order starts (two bytes).
const MOTOR_MAP_OFFSET: usize = V5_STORED_CONFIG_LEN;
/// Version 6.
pub const V6_STORED_CONFIG_LEN: usize = MOTOR_MAP_OFFSET + 2;
/// Where version 7's accelerometer level offsets start: three `i16` milli-g.
const ACCEL_LEVEL_OFFSET: usize = V6_STORED_CONFIG_LEN;
pub const STORED_CONFIG_LEN: usize = ACCEL_LEVEL_OFFSET + 6;
/// Version 5 adds the RC channel map and RC protocol after the version 4
/// payload. Every earlier payload decodes with [`RcChannelMap::AETR_ARM_CH9`]
/// and SBUS, the fixed map and protocol firmware used before, so updating
/// never moves a pilot's arm switch.
///
/// Version 4 adds the serial port bindings after the version 3 payload.
/// Version 3 stores the gyro filter as a corner in hertz. Versions 1 and 2
/// stored a one-pole smoothing factor, which only means what it is meant to
/// mean at one loop rate. A version 2 payload is still read: its coefficient
/// is converted at the rate it was authored for, so a tune saved before this
/// change keeps the filter it had.
/// Version 6 adds the motor order after the version 5 payload; earlier
/// payloads decode with [`MotorOutputMap::IDENTITY`], the board's wiring.
const STORED_CONFIG_SCHEMA_VERSION: u16 = 7;
const STORED_CONFIG_SCHEMA_VERSION_MOTOR_MAP: u16 = 6;
const STORED_CONFIG_SCHEMA_VERSION_RC: u16 = 5;
const STORED_CONFIG_SCHEMA_VERSION_BINDINGS: u16 = 4;
const STORED_CONFIG_SCHEMA_VERSION_CORNER: u16 = 3;
const STORED_CONFIG_SCHEMA_VERSION_ALPHA: u16 = 2;
/// Bindings bytes meaning "never set": the board's defaults apply.
const SERIAL_BINDINGS_UNSET: [u8; SERIAL_PORT_SLOTS] = [0xff; SERIAL_PORT_SLOTS];

/// The loop rate every version 1 and 2 coefficient was authored for.
const LEGACY_LPF_SAMPLE_RATE_HZ: f32 = 400.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredConfig {
    pub tuning: TuningProfile,
    pub log_rate_divisor: u16,
    /// Which function each serial port serves; `None` until a pilot sets
    /// one, and then the board's own defaults apply. Read once at boot.
    pub serial_bindings: Option<SerialBindings>,
    /// The receiver protocol on the port bound to RC input. Read once at
    /// boot, since it sets that port's line settings.
    pub rc_protocol: RcProtocol,
    /// Where the accelerometer reads level, set by `imu calibrate`. Not a
    /// [`ConfigKey`]: it belongs to this craft, not to a tune copied to
    /// another.
    pub accel_level: AccelLevel,
}

impl StoredConfig {
    pub const fn first_hop_default() -> Self {
        Self {
            tuning: TuningProfile::default_first_hop(),
            log_rate_divisor: 1,
            serial_bindings: None,
            rc_protocol: RcProtocol::Sbus,
            accel_level: AccelLevel::ZERO,
        }
    }

    pub fn set(&mut self, key: ConfigKey, value: f32) -> bool {
        if !key.value_spec().accepts(value) {
            return false;
        }
        let mut candidate = *self;
        match key {
            ConfigKey::RollP => candidate.tuning.rate_gains.roll.p = value,
            ConfigKey::RollI => candidate.tuning.rate_gains.roll.i = value,
            ConfigKey::RollD => candidate.tuning.rate_gains.roll.d = value,
            ConfigKey::PitchP => candidate.tuning.rate_gains.pitch.p = value,
            ConfigKey::PitchI => candidate.tuning.rate_gains.pitch.i = value,
            ConfigKey::PitchD => candidate.tuning.rate_gains.pitch.d = value,
            ConfigKey::YawP => candidate.tuning.rate_gains.yaw.p = value,
            ConfigKey::YawI => candidate.tuning.rate_gains.yaw.i = value,
            ConfigKey::YawD => candidate.tuning.rate_gains.yaw.d = value,
            ConfigKey::ImuLpfHz => candidate.tuning.imu_lpf_hz = value,
            ConfigKey::LogRateDivisor => {
                let integer = value as u16;
                if !(1.0..=16.0).contains(&value) || integer as f32 != value {
                    return false;
                }
                candidate.log_rate_divisor = integer;
            }
            ConfigKey::RcDeadband => {
                let integer = value as u16;
                if !(0.0..=100.0).contains(&value) || integer as f32 != value {
                    return false;
                }
                candidate.tuning.rc_rates.deadband = integer;
            }
            ConfigKey::RollCenterRate => {
                candidate.tuning.rc_rates.roll.center_sensitivity_dps = value
            }
            ConfigKey::RollMaxRate => candidate.tuning.rc_rates.roll.max_rate_dps = value,
            ConfigKey::RollExpo => candidate.tuning.rc_rates.roll.expo = value,
            ConfigKey::PitchCenterRate => {
                candidate.tuning.rc_rates.pitch.center_sensitivity_dps = value
            }
            ConfigKey::PitchMaxRate => candidate.tuning.rc_rates.pitch.max_rate_dps = value,
            ConfigKey::PitchExpo => candidate.tuning.rc_rates.pitch.expo = value,
            ConfigKey::YawCenterRate => {
                candidate.tuning.rc_rates.yaw.center_sensitivity_dps = value
            }
            ConfigKey::YawMaxRate => candidate.tuning.rc_rates.yaw.max_rate_dps = value,
            ConfigKey::YawExpo => candidate.tuning.rc_rates.yaw.expo = value,
            ConfigKey::RcMap => {
                match RcChannelMap::from_config(value as u16, candidate.tuning.rc_map.arm_channel())
                {
                    Some(map) => candidate.tuning.rc_map = map,
                    None => return false,
                }
            }
            ConfigKey::RcArmChannel => {
                match RcChannelMap::from_config(candidate.tuning.rc_map.stick_order(), value as u8)
                {
                    Some(map) => candidate.tuning.rc_map = map,
                    None => return false,
                }
            }
            ConfigKey::RcProtocol => match RcProtocol::from_u8(value as u8) {
                Some(protocol) => candidate.rc_protocol = protocol,
                None => return false,
            },
            ConfigKey::MotorMap => match MotorOutputMap::from_config(value as u16) {
                Some(map) => candidate.tuning.motor_map = map,
                None => return false,
            },
        }
        if candidate.tuning.sanitized() != candidate.tuning {
            return false;
        }
        *self = candidate;
        true
    }

    pub fn get(self, key: ConfigKey) -> f32 {
        match key {
            ConfigKey::RollP => self.tuning.rate_gains.roll.p,
            ConfigKey::RollI => self.tuning.rate_gains.roll.i,
            ConfigKey::RollD => self.tuning.rate_gains.roll.d,
            ConfigKey::PitchP => self.tuning.rate_gains.pitch.p,
            ConfigKey::PitchI => self.tuning.rate_gains.pitch.i,
            ConfigKey::PitchD => self.tuning.rate_gains.pitch.d,
            ConfigKey::YawP => self.tuning.rate_gains.yaw.p,
            ConfigKey::YawI => self.tuning.rate_gains.yaw.i,
            ConfigKey::YawD => self.tuning.rate_gains.yaw.d,
            ConfigKey::ImuLpfHz => self.tuning.imu_lpf_hz,
            ConfigKey::LogRateDivisor => self.log_rate_divisor as f32,
            ConfigKey::RcDeadband => self.tuning.rc_rates.deadband as f32,
            ConfigKey::RollCenterRate => self.tuning.rc_rates.roll.center_sensitivity_dps,
            ConfigKey::RollMaxRate => self.tuning.rc_rates.roll.max_rate_dps,
            ConfigKey::RollExpo => self.tuning.rc_rates.roll.expo,
            ConfigKey::PitchCenterRate => self.tuning.rc_rates.pitch.center_sensitivity_dps,
            ConfigKey::PitchMaxRate => self.tuning.rc_rates.pitch.max_rate_dps,
            ConfigKey::PitchExpo => self.tuning.rc_rates.pitch.expo,
            ConfigKey::YawCenterRate => self.tuning.rc_rates.yaw.center_sensitivity_dps,
            ConfigKey::YawMaxRate => self.tuning.rc_rates.yaw.max_rate_dps,
            ConfigKey::YawExpo => self.tuning.rc_rates.yaw.expo,
            ConfigKey::RcMap => self.tuning.rc_map.stick_order() as f32,
            ConfigKey::RcArmChannel => self.tuning.rc_map.arm_channel() as f32,
            ConfigKey::RcProtocol => self.rc_protocol as u8 as f32,
            ConfigKey::MotorMap => self.tuning.motor_map.to_config() as f32,
        }
    }

    pub fn encode(self) -> [u8; STORED_CONFIG_LEN] {
        let values = [
            self.tuning.rate_gains.roll.p,
            self.tuning.rate_gains.roll.i,
            self.tuning.rate_gains.roll.d,
            self.tuning.rate_gains.pitch.p,
            self.tuning.rate_gains.pitch.i,
            self.tuning.rate_gains.pitch.d,
            self.tuning.rate_gains.yaw.p,
            self.tuning.rate_gains.yaw.i,
            self.tuning.rate_gains.yaw.d,
            self.tuning.imu_lpf_hz,
        ];
        let mut output = [0u8; STORED_CONFIG_LEN];
        for (index, value) in values.iter().enumerate() {
            let start = index * 4;
            output[start..start + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        output[40..42].copy_from_slice(&self.log_rate_divisor.to_le_bytes());
        output[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION.to_le_bytes());
        let rate_values = [
            self.tuning.rc_rates.roll.center_sensitivity_dps,
            self.tuning.rc_rates.roll.max_rate_dps,
            self.tuning.rc_rates.roll.expo,
            self.tuning.rc_rates.pitch.center_sensitivity_dps,
            self.tuning.rc_rates.pitch.max_rate_dps,
            self.tuning.rc_rates.pitch.expo,
            self.tuning.rc_rates.yaw.center_sensitivity_dps,
            self.tuning.rc_rates.yaw.max_rate_dps,
            self.tuning.rc_rates.yaw.expo,
        ];
        for (index, value) in rate_values.iter().enumerate() {
            let start = 44 + index * 4;
            output[start..start + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        output[80..82].copy_from_slice(&self.tuning.rc_rates.deadband.to_le_bytes());
        output[V3_STORED_CONFIG_LEN..V4_STORED_CONFIG_LEN].copy_from_slice(
            &self
                .serial_bindings
                .map_or(SERIAL_BINDINGS_UNSET, |bindings| bindings.encode()),
        );
        output[RC_FIELDS_OFFSET..RC_FIELDS_OFFSET + 2]
            .copy_from_slice(&self.tuning.rc_map.stick_order().to_le_bytes());
        output[RC_FIELDS_OFFSET + 2] = self.tuning.rc_map.arm_channel();
        output[RC_FIELDS_OFFSET + 3] = self.rc_protocol as u8;
        output[MOTOR_MAP_OFFSET..V6_STORED_CONFIG_LEN]
            .copy_from_slice(&self.tuning.motor_map.to_config().to_le_bytes());
        for (axis, offset) in self.accel_level.offset_mg.iter().enumerate() {
            let start = ACCEL_LEVEL_OFFSET + axis * 2;
            output[start..start + 2].copy_from_slice(&offset.to_le_bytes());
        }
        output
    }

    pub fn decode(input: &[u8]) -> Option<Self> {
        if ![
            LEGACY_STORED_CONFIG_LEN,
            V3_STORED_CONFIG_LEN,
            V4_STORED_CONFIG_LEN,
            V5_STORED_CONFIG_LEN,
            V6_STORED_CONFIG_LEN,
            STORED_CONFIG_LEN,
        ]
        .contains(&input.len())
        {
            return None;
        }
        let mut values = [0.0f32; 10];
        for (index, value) in values.iter_mut().enumerate() {
            let start = index * 4;
            *value = f32::from_bits(u32::from_le_bytes([
                input[start],
                input[start + 1],
                input[start + 2],
                input[start + 3],
            ]));
        }
        // Version 1 payloads carry no version field; they are coefficients.
        let mut schema_version = STORED_CONFIG_SCHEMA_VERSION_ALPHA;
        let rc_rates = if input.len() == LEGACY_STORED_CONFIG_LEN {
            RC_RATE_PROFILE
        } else {
            let stored_version = u16::from_le_bytes([input[42], input[43]]);
            let expected_len = match stored_version {
                STORED_CONFIG_SCHEMA_VERSION => STORED_CONFIG_LEN,
                STORED_CONFIG_SCHEMA_VERSION_MOTOR_MAP => V6_STORED_CONFIG_LEN,
                STORED_CONFIG_SCHEMA_VERSION_RC => V5_STORED_CONFIG_LEN,
                STORED_CONFIG_SCHEMA_VERSION_BINDINGS => V4_STORED_CONFIG_LEN,
                STORED_CONFIG_SCHEMA_VERSION_CORNER | STORED_CONFIG_SCHEMA_VERSION_ALPHA => {
                    V3_STORED_CONFIG_LEN
                }
                _ => return None,
            };
            if input.len() != expected_len {
                return None;
            }
            schema_version = stored_version;
            let mut rate_values = [0.0f32; 9];
            for (index, value) in rate_values.iter_mut().enumerate() {
                let start = 44 + index * 4;
                *value = f32::from_bits(u32::from_le_bytes([
                    input[start],
                    input[start + 1],
                    input[start + 2],
                    input[start + 3],
                ]));
            }
            RcRateProfile {
                roll: ActualRateAxis::new(rate_values[0], rate_values[1], rate_values[2]),
                pitch: ActualRateAxis::new(rate_values[3], rate_values[4], rate_values[5]),
                yaw: ActualRateAxis::new(rate_values[6], rate_values[7], rate_values[8]),
                deadband: u16::from_le_bytes([input[80], input[81]]),
            }
        };
        let (rc_map, rc_protocol) = if schema_version >= STORED_CONFIG_SCHEMA_VERSION_RC {
            let fields = &input[RC_FIELDS_OFFSET..V5_STORED_CONFIG_LEN];
            (
                RcChannelMap::from_config(u16::from_le_bytes([fields[0], fields[1]]), fields[2])?,
                RcProtocol::from_u8(fields[3])?,
            )
        } else {
            (RcChannelMap::AETR_ARM_CH9, RcProtocol::Sbus)
        };
        let motor_map = if schema_version >= STORED_CONFIG_SCHEMA_VERSION_MOTOR_MAP {
            MotorOutputMap::from_config(u16::from_le_bytes([
                input[MOTOR_MAP_OFFSET],
                input[MOTOR_MAP_OFFSET + 1],
            ]))?
        } else {
            MotorOutputMap::IDENTITY
        };
        // Older payloads predate calibration: the accelerometer is uncorrected.
        let accel_level = if schema_version == STORED_CONFIG_SCHEMA_VERSION {
            let axis = |index: usize| {
                let start = ACCEL_LEVEL_OFFSET + index * 2;
                i16::from_le_bytes([input[start], input[start + 1]])
            };
            AccelLevel::new([axis(0), axis(1), axis(2)])?
        } else {
            AccelLevel::ZERO
        };
        let candidate = Self {
            tuning: TuningProfile {
                rate_gains: RateControllerGains {
                    roll: PidGains {
                        p: values[0],
                        i: values[1],
                        d: values[2],
                    },
                    pitch: PidGains {
                        p: values[3],
                        i: values[4],
                        d: values[5],
                    },
                    yaw: PidGains {
                        p: values[6],
                        i: values[7],
                        d: values[8],
                    },
                },
                // Older payloads stored a one-pole coefficient. Convert it at
                // the rate it was authored for, so the filter a stored tune
                // had is the filter it keeps.
                imu_lpf_hz: if schema_version != STORED_CONFIG_SCHEMA_VERSION_ALPHA {
                    values[9]
                } else {
                    gyro_lpf_corner_hz(values[9], LEGACY_LPF_SAMPLE_RATE_HZ)
                },
                rc_rates,
                rc_map,
                motor_map,
            },
            log_rate_divisor: u16::from_le_bytes([input[40], input[41]]),
            rc_protocol,
            accel_level,
            serial_bindings: match input.get(V3_STORED_CONFIG_LEN..V4_STORED_CONFIG_LEN) {
                Some(bytes) if bytes.len() == SERIAL_PORT_SLOTS => {
                    let bytes: &[u8; SERIAL_PORT_SLOTS] = bytes.try_into().ok()?;
                    if *bytes == SERIAL_BINDINGS_UNSET {
                        None
                    } else {
                        // A function this firmware does not know rejects the
                        // whole payload, like any other invalid field.
                        Some(SerialBindings::decode(bytes)?)
                    }
                }
                _ => None,
            },
        };
        if candidate.log_rate_divisor == 0
            || candidate.log_rate_divisor > 16
            || candidate.tuning.sanitized() != candidate.tuning
        {
            None
        } else {
            Some(candidate)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_control::filter::gyro_lpf_alpha;
    use ferrowasp_serial::{LogicalSerialPort, SerialFunction};

    #[test]
    fn stored_config_round_trips_and_rejects_unsafe_values() {
        let mut config = StoredConfig::first_hop_default();
        assert!(config.set(ConfigKey::PitchP, 0.30));
        assert!(config.set(ConfigKey::LogRateDivisor, 2.0));
        assert!(config.set(ConfigKey::RcDeadband, 12.0));
        assert!(config.set(ConfigKey::RollMaxRate, 600.0));
        assert!(config.set(ConfigKey::RollCenterRate, 120.0));
        assert!(config.set(ConfigKey::RollExpo, 0.6));
        assert!(!config.set(ConfigKey::RollP, 100.0));
        assert!(!config.set(ConfigKey::RcDeadband, 12.5));
        assert!(!config.set(ConfigKey::RollCenterRate, 700.0));
        assert!(!config.set(ConfigKey::RollMaxRate, 100.0));
        assert!(!config.set(ConfigKey::RollExpo, 1.1));
        let encoded = config.encode();
        let decoded = StoredConfig::decode(&encoded).unwrap();
        assert_eq!(decoded.get(ConfigKey::PitchP), 0.30);
        assert_eq!(decoded.log_rate_divisor, 2);
        assert_eq!(decoded.get(ConfigKey::RcDeadband), 12.0);
        assert_eq!(decoded.get(ConfigKey::RollCenterRate), 120.0);
        assert_eq!(decoded.get(ConfigKey::RollMaxRate), 600.0);
        assert_eq!(decoded.get(ConfigKey::RollExpo), 0.6);

        let mut corrupt = encoded;
        corrupt[40] = 0;
        corrupt[41] = 0;
        assert_eq!(StoredConfig::decode(&corrupt), None);
    }

    /// A tune saved by the previous firmware must keep the filter it had.
    ///
    /// Version 2 stored a one-pole coefficient; version 3 stores the corner.
    /// Reading a version 2 payload converts at the rate the coefficient was
    /// authored for, or every saved tune silently changes.
    #[test]
    fn a_version_two_coefficient_survives_as_the_same_filter() {
        let mut payload = [0u8; V3_STORED_CONFIG_LEN];
        payload
            .copy_from_slice(&StoredConfig::first_hop_default().encode()[..V3_STORED_CONFIG_LEN]);
        payload[36..40].copy_from_slice(&0.55f32.to_bits().to_le_bytes());
        payload[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_ALPHA.to_le_bytes());

        let decoded = StoredConfig::decode(&payload).expect("version 2 must still be readable");
        let corner = decoded.tuning.imu_lpf_hz;
        assert!(
            (corner - 50.8).abs() < 0.2,
            "0.55 at 400 Hz is a 50.8 Hz corner, decoded {corner}"
        );
        let alpha = gyro_lpf_alpha(corner, LEGACY_LPF_SAMPLE_RATE_HZ);
        assert!(
            (alpha - 0.55).abs() < 0.005,
            "round trip should return 0.55, got {alpha}"
        );
    }

    /// The point of storing a frequency: it means the same filter at any rate.
    #[test]
    fn a_stored_corner_holds_its_meaning_across_loop_rates() {
        let corner = 50.8;
        let at_400 = gyro_lpf_alpha(corner, 400.0);
        let at_1000 = gyro_lpf_alpha(corner, 1000.0);
        assert!(
            at_1000 < at_400,
            "a faster loop needs a smaller coefficient"
        );
        for (alpha, rate) in [(at_400, 400.0), (at_1000, 1000.0)] {
            let back = gyro_lpf_corner_hz(alpha, rate);
            assert!((back - corner).abs() < 0.1, "{rate} Hz -> {back}");
        }
    }

    #[test]
    fn serial_bindings_round_trip_and_older_payloads_use_board_defaults() {
        let mut config = StoredConfig::first_hop_default();
        let encoded = config.encode();
        assert_eq!(
            StoredConfig::decode(&encoded).unwrap().serial_bindings,
            None
        );

        config.serial_bindings = Some(
            SerialBindings::none()
                .with(LogicalSerialPort::Uart2, SerialFunction::RcInput)
                .with(LogicalSerialPort::Uart4, SerialFunction::EscTelemetry),
        );
        let encoded = config.encode();
        assert_eq!(StoredConfig::decode(&encoded), Some(config));

        let mut version_three = [0u8; V3_STORED_CONFIG_LEN];
        version_three.copy_from_slice(&encoded[..V3_STORED_CONFIG_LEN]);
        version_three[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_CORNER.to_le_bytes());
        let decoded =
            StoredConfig::decode(&version_three).expect("version 3 must still be readable");
        assert_eq!(decoded.serial_bindings, None);
        assert_eq!(decoded.tuning, config.tuning);

        let mut unknown_function = encoded;
        unknown_function[V3_STORED_CONFIG_LEN] = 0x40;
        assert_eq!(StoredConfig::decode(&unknown_function), None);
        // A version 4 header on a version 3 length is not trusted.
        assert_eq!(StoredConfig::decode(&encoded[..V3_STORED_CONFIG_LEN]), None);
    }

    /// A configuration saved before the map was configurable armed on
    /// channel 9 over SBUS. Reading it must keep both: moving the arm switch
    /// under a pilot who has not changed their radio could arm from another
    /// switch.
    #[test]
    fn a_version_four_config_keeps_its_bindings_the_channel_nine_arm_switch_and_sbus() {
        let mut config = StoredConfig::first_hop_default();
        config.serial_bindings =
            Some(SerialBindings::none().with(LogicalSerialPort::Uart2, SerialFunction::RcInput));
        let mut version_four = [0u8; V4_STORED_CONFIG_LEN];
        version_four.copy_from_slice(&config.encode()[..V4_STORED_CONFIG_LEN]);
        version_four[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_BINDINGS.to_le_bytes());

        let decoded =
            StoredConfig::decode(&version_four).expect("version 4 must still be readable");
        assert_eq!(decoded.serial_bindings, config.serial_bindings);
        assert_eq!(decoded.tuning.rc_map, RcChannelMap::AETR_ARM_CH9);
        assert_eq!(decoded.get(ConfigKey::RcArmChannel), 9.0);
        assert_eq!(decoded.rc_protocol, RcProtocol::Sbus);
        // A version 5 header on a version 4 length is not trusted.
        let mut short = version_four;
        short[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION.to_le_bytes());
        assert_eq!(StoredConfig::decode(&short), None);
    }

    #[test]
    fn rc_map_and_protocol_are_configured_saved_and_validated() {
        let mut config = StoredConfig::first_hop_default();
        assert_eq!(config.get(ConfigKey::RcMap), 1234.0);
        assert_eq!(config.get(ConfigKey::RcArmChannel), 9.0);
        assert_eq!(config.get(ConfigKey::RcProtocol), 0.0);
        assert!(config.set(ConfigKey::RcMap, 2314.0));
        assert!(config.set(ConfigKey::RcArmChannel, 5.0));
        assert!(config.set(ConfigKey::RcProtocol, 1.0));
        assert!(!config.set(ConfigKey::RcMap, 1224.0));
        assert!(!config.set(ConfigKey::RcMap, 1234.5));
        assert!(!config.set(ConfigKey::RcArmChannel, 4.0));
        assert!(!config.set(ConfigKey::RcArmChannel, 17.0));
        assert!(!config.set(ConfigKey::RcProtocol, 2.0));

        let encoded = config.encode();
        let decoded = StoredConfig::decode(&encoded).unwrap();
        assert_eq!(decoded, config);
        assert_eq!(decoded.rc_protocol, RcProtocol::Crsf);
        assert_eq!(decoded.tuning.rc_map.stick_order(), 2314);

        let mut bad_protocol = encoded;
        bad_protocol[V6_STORED_CONFIG_LEN - 1] = 7;
        assert_eq!(StoredConfig::decode(&bad_protocol), None);
        let mut bad_map = encoded;
        bad_map[RC_FIELDS_OFFSET..RC_FIELDS_OFFSET + 2].copy_from_slice(&1224u16.to_le_bytes());
        assert_eq!(StoredConfig::decode(&bad_map), None);
    }

    #[test]
    fn motor_order_is_saved_and_older_payloads_keep_the_board_wiring() {
        let mut config = StoredConfig::first_hop_default();
        assert_eq!(config.get(ConfigKey::MotorMap), 1234.0);
        assert!(config.set(ConfigKey::MotorMap, 2143.0));
        assert!(!config.set(ConfigKey::MotorMap, 1123.0));
        assert!(!config.set(ConfigKey::MotorMap, 1235.0));
        let encoded = config.encode();
        assert_eq!(StoredConfig::decode(&encoded), Some(config));

        let mut version_five = [0u8; V5_STORED_CONFIG_LEN];
        version_five.copy_from_slice(&encoded[..V5_STORED_CONFIG_LEN]);
        version_five[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_RC.to_le_bytes());
        let decoded =
            StoredConfig::decode(&version_five).expect("version 5 must still be readable");
        assert_eq!(decoded.tuning.motor_map, MotorOutputMap::IDENTITY);
        assert_eq!(decoded.tuning.rc_map, config.tuning.rc_map);

        let mut bad = encoded;
        bad[MOTOR_MAP_OFFSET..V6_STORED_CONFIG_LEN].copy_from_slice(&1123u16.to_le_bytes());
        assert_eq!(StoredConfig::decode(&bad), None);
    }

    #[test]
    fn accelerometer_level_is_saved_and_older_payloads_are_uncorrected() {
        let mut config = StoredConfig::first_hop_default();
        config.accel_level = AccelLevel::new([12, -52, 3]).unwrap();
        let encoded = config.encode();
        assert_eq!(StoredConfig::decode(&encoded), Some(config));

        let mut version_six = [0u8; V6_STORED_CONFIG_LEN];
        version_six.copy_from_slice(&encoded[..V6_STORED_CONFIG_LEN]);
        version_six[42..44].copy_from_slice(&STORED_CONFIG_SCHEMA_VERSION_MOTOR_MAP.to_le_bytes());
        let decoded = StoredConfig::decode(&version_six).expect("version 6 must still be readable");
        assert_eq!(decoded.accel_level, AccelLevel::ZERO);
        assert_eq!(decoded.tuning, config.tuning);

        let mut out_of_range = encoded;
        out_of_range[ACCEL_LEVEL_OFFSET..ACCEL_LEVEL_OFFSET + 2]
            .copy_from_slice(&900i16.to_le_bytes());
        assert_eq!(StoredConfig::decode(&out_of_range), None);
    }

    #[test]
    fn legacy_stored_config_migrates_with_current_rc_defaults() {
        let current = StoredConfig::first_hop_default().encode();
        let mut legacy = [0u8; LEGACY_STORED_CONFIG_LEN];
        legacy.copy_from_slice(&current[..LEGACY_STORED_CONFIG_LEN]);
        legacy[42..44].fill(0);
        // A real legacy payload holds a one-pole coefficient here, not the
        // corner the current default encodes.
        legacy[36..40].copy_from_slice(&0.55f32.to_bits().to_le_bytes());

        let migrated = StoredConfig::decode(&legacy).unwrap();

        assert_eq!(migrated.tuning.rc_rates, RC_RATE_PROFILE);
        assert_eq!(migrated.log_rate_divisor, 1);
        // The coefficient becomes the corner it always described.
        assert!((migrated.tuning.imu_lpf_hz - 50.8).abs() < 0.2);
    }
}
