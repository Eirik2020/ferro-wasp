use std::{fmt, fs, path::Path};

pub use ferrowasp_core::config::ConfigKey;
use serde::{Deserialize, Serialize};

use crate::error::{FerroError, Result};

pub const CONFIG_SCHEMA_VERSION: u16 = 2;
pub const LEGACY_CONFIG_SCHEMA_VERSION: u16 = 1;
const MAX_ROUND_TRIP_DELTA: f32 = 0.000_11;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisPid {
    pub p: f32,
    pub i: f32,
    pub d: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FerroConfig {
    #[serde(default = "schema_version")]
    pub schema_version: u16,
    pub roll: AxisPid,
    pub pitch: AxisPid,
    pub yaw: AxisPid,
    pub imu_lpf_alpha: f32,
    pub log_rate_divisor: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rc_deadband: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll_center_rate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll_max_rate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll_expo: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_center_rate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_max_rate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_expo: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw_center_rate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw_max_rate: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw_expo: Option<f32>,
}

const fn schema_version() -> u16 {
    CONFIG_SCHEMA_VERSION
}

impl Default for FerroConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            roll: AxisPid {
                p: 2.5,
                i: 0.0,
                d: 0.0,
            },
            pitch: AxisPid {
                p: 2.5,
                i: 0.0,
                d: 0.0,
            },
            yaw: AxisPid {
                p: 2.0,
                i: 0.0,
                d: 0.0,
            },
            imu_lpf_alpha: 0.55,
            log_rate_divisor: 1,
            rc_deadband: Some(8),
            roll_center_rate: Some(70.0),
            roll_max_rate: Some(300.0),
            roll_expo: Some(0.5),
            pitch_center_rate: Some(70.0),
            pitch_max_rate: Some(300.0),
            pitch_expo: Some(0.5),
            yaw_center_rate: Some(70.0),
            yaw_max_rate: Some(200.0),
            yaw_expo: Some(0.5),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationError {
    pub field: &'static str,
    pub reason: String,
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.field, self.reason)
    }
}

impl FerroConfig {
    pub fn validate(&self) -> std::result::Result<(), Vec<ConfigValidationError>> {
        let mut errors = Vec::new();
        if self.schema_version != CONFIG_SCHEMA_VERSION
            && self.schema_version != LEGACY_CONFIG_SCHEMA_VERSION
        {
            errors.push(ConfigValidationError {
                field: "schema_version",
                reason: format!(
                    "is {}, but this application supports only {} and legacy {}",
                    self.schema_version, CONFIG_SCHEMA_VERSION, LEGACY_CONFIG_SCHEMA_VERSION
                ),
            });
        }

        for key in ConfigKey::ALL {
            match self.get(key) {
                Some(value) if !key.value_spec().accepts(value) => {
                    let spec = key.value_spec();
                    let kind = if spec.integer {
                        "a whole number"
                    } else {
                        "a finite number"
                    };
                    errors.push(ConfigValidationError {
                        field: key.name(),
                        reason: format!(
                            "must be {kind} from {} through {}",
                            spec.minimum, spec.maximum
                        ),
                    });
                }
                None if self.schema_version == CONFIG_SCHEMA_VERSION => {
                    errors.push(ConfigValidationError {
                        field: key.name(),
                        reason: "is required by configuration schema v2".to_owned(),
                    });
                }
                _ => {}
            }
        }

        validate_rate_axis(
            "roll_max_rate",
            self.roll_center_rate,
            self.roll_max_rate,
            &mut errors,
        );
        validate_rate_axis(
            "pitch_max_rate",
            self.pitch_center_rate,
            self.pitch_max_rate,
            &mut errors,
        );
        validate_rate_axis(
            "yaw_max_rate",
            self.yaw_center_rate,
            self.yaw_max_rate,
            &mut errors,
        );

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn from_toml_file(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|error| FerroError::ReadConfig {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
        let config: Self = toml::from_str(&text).map_err(|error| FerroError::ReadConfig {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
        config.ensure_valid()?;
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String> {
        self.ensure_valid()?;
        toml::to_string_pretty(self)
            .map_err(|error| FerroError::InvalidConfiguration(error.to_string()))
    }

    pub fn write_toml_file(&self, path: &Path, force: bool) -> Result<()> {
        if path.exists() && !force {
            return Err(FerroError::WriteConfig {
                path: path.to_path_buf(),
                reason: "the file already exists; pass --force to replace it".to_owned(),
            });
        }
        let text = format!(
            "# FerroWasp configuration exported by FerroConfigurator.\n# Changes are validated again by firmware before persistence.\n{}",
            self.to_toml()?
        );
        fs::write(path, text).map_err(|error| FerroError::WriteConfig {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
    }

    pub fn ensure_valid(&self) -> Result<()> {
        self.validate().map_err(|errors| {
            FerroError::InvalidConfiguration(
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
    }

    pub fn ensure_complete(&self) -> Result<()> {
        self.ensure_valid()?;
        if self.schema_version != CONFIG_SCHEMA_VERSION
            || ConfigKey::ALL
                .into_iter()
                .any(|key| self.get(key).is_none())
        {
            return Err(FerroError::InvalidConfiguration(
                "configuration must be merged with a live schema-v2 device snapshot before staging"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    pub fn get(&self, key: ConfigKey) -> Option<f32> {
        match key {
            ConfigKey::RollP => Some(self.roll.p),
            ConfigKey::RollI => Some(self.roll.i),
            ConfigKey::RollD => Some(self.roll.d),
            ConfigKey::PitchP => Some(self.pitch.p),
            ConfigKey::PitchI => Some(self.pitch.i),
            ConfigKey::PitchD => Some(self.pitch.d),
            ConfigKey::YawP => Some(self.yaw.p),
            ConfigKey::YawI => Some(self.yaw.i),
            ConfigKey::YawD => Some(self.yaw.d),
            ConfigKey::ImuLpfAlpha => Some(self.imu_lpf_alpha),
            ConfigKey::LogRateDivisor => Some(self.log_rate_divisor as f32),
            ConfigKey::RcDeadband => self.rc_deadband.map(|value| value as f32),
            ConfigKey::RollCenterRate => self.roll_center_rate,
            ConfigKey::RollMaxRate => self.roll_max_rate,
            ConfigKey::RollExpo => self.roll_expo,
            ConfigKey::PitchCenterRate => self.pitch_center_rate,
            ConfigKey::PitchMaxRate => self.pitch_max_rate,
            ConfigKey::PitchExpo => self.pitch_expo,
            ConfigKey::YawCenterRate => self.yaw_center_rate,
            ConfigKey::YawMaxRate => self.yaw_max_rate,
            ConfigKey::YawExpo => self.yaw_expo,
        }
    }

    pub fn set_from_str(&mut self, key: ConfigKey, value: &str) -> Result<()> {
        let parsed = value.parse::<f32>().map_err(|_| {
            FerroError::InvalidConfiguration(format!("{} must be a number", key.name()))
        })?;
        let mut candidate = self.clone();
        candidate.set(key, parsed)?;
        candidate.ensure_valid()?;
        *self = candidate;
        Ok(())
    }

    pub fn set(&mut self, key: ConfigKey, value: f32) -> Result<()> {
        if !key.value_spec().accepts(value) {
            return Err(FerroError::InvalidConfiguration(format!(
                "{} is outside the firmware-accepted range",
                key.name()
            )));
        }
        match key {
            ConfigKey::RollP => self.roll.p = value,
            ConfigKey::RollI => self.roll.i = value,
            ConfigKey::RollD => self.roll.d = value,
            ConfigKey::PitchP => self.pitch.p = value,
            ConfigKey::PitchI => self.pitch.i = value,
            ConfigKey::PitchD => self.pitch.d = value,
            ConfigKey::YawP => self.yaw.p = value,
            ConfigKey::YawI => self.yaw.i = value,
            ConfigKey::YawD => self.yaw.d = value,
            ConfigKey::ImuLpfAlpha => self.imu_lpf_alpha = value,
            ConfigKey::LogRateDivisor => self.log_rate_divisor = value as u16,
            ConfigKey::RcDeadband => self.rc_deadband = Some(value as u16),
            ConfigKey::RollCenterRate => self.roll_center_rate = Some(value),
            ConfigKey::RollMaxRate => self.roll_max_rate = Some(value),
            ConfigKey::RollExpo => self.roll_expo = Some(value),
            ConfigKey::PitchCenterRate => self.pitch_center_rate = Some(value),
            ConfigKey::PitchMaxRate => self.pitch_max_rate = Some(value),
            ConfigKey::PitchExpo => self.pitch_expo = Some(value),
            ConfigKey::YawCenterRate => self.yaw_center_rate = Some(value),
            ConfigKey::YawMaxRate => self.yaw_max_rate = Some(value),
            ConfigKey::YawExpo => self.yaw_expo = Some(value),
        }
        Ok(())
    }

    pub fn merged_over(&self, base: &Self) -> Result<Self> {
        base.ensure_complete()?;
        self.ensure_valid()?;
        let mut merged = base.clone();
        for key in ConfigKey::ALL {
            if let Some(value) = self.get(key) {
                merged.set(key, value)?;
            }
        }
        merged.schema_version = CONFIG_SCHEMA_VERSION;
        merged.ensure_complete()?;
        Ok(merged)
    }

    pub fn differences(&self, other: &Self) -> Vec<String> {
        ConfigKey::ALL
            .into_iter()
            .filter_map(|key| match (self.get(key), other.get(key)) {
                (Some(expected), Some(actual))
                    if (expected - actual).abs() > MAX_ROUND_TRIP_DELTA =>
                {
                    Some(format!(
                        "{} expected {:.4}, read {:.4}",
                        key.name(),
                        expected,
                        actual
                    ))
                }
                (Some(_), None) => Some(format!("{} was absent from readback", key.name())),
                _ => None,
            })
            .collect()
    }
}

fn validate_rate_axis(
    maximum_field: &'static str,
    center: Option<f32>,
    maximum: Option<f32>,
    errors: &mut Vec<ConfigValidationError>,
) {
    if let (Some(center), Some(maximum)) = (center, maximum)
        && maximum < center
    {
        errors.push(ConfigValidationError {
            field: maximum_field,
            reason: "must be greater than or equal to the corresponding center rate".to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_approved_foxeer_f405_v2_baseline() {
        let config = FerroConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.roll.p, 2.5);
        assert_eq!(config.pitch.p, 2.5);
        assert_eq!(config.yaw.p, 2.0);
        assert_eq!(config.roll.i, 0.0);
        assert_eq!(config.pitch.i, 0.0);
        assert_eq!(config.yaw.i, 0.0);
    }

    #[test]
    fn schema_v2_toml_round_trip_is_lossless() {
        let original = FerroConfig::default();
        let encoded = original.to_toml().unwrap();
        let decoded: FerroConfig = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn legacy_schema_overlays_without_changing_rc_rates() {
        let text = r#"
schema_version = 1
imu_lpf_alpha = 0.55
log_rate_divisor = 2

[roll]
p = 1.0
i = 0.0
d = 0.0

[pitch]
p = 1.1
i = 0.0
d = 0.0

[yaw]
p = 1.2
i = 0.0
d = 0.0
"#;
        let legacy: FerroConfig = toml::from_str(text).unwrap();
        legacy.ensure_valid().unwrap();
        assert_eq!(legacy.get(ConfigKey::RcDeadband), None);

        let live = FerroConfig {
            rc_deadband: Some(12),
            roll_max_rate: Some(450.0),
            ..FerroConfig::default()
        };
        let merged = legacy.merged_over(&live).unwrap();
        assert_eq!(merged.roll.p, 1.0);
        assert_eq!(merged.log_rate_divisor, 2);
        assert_eq!(merged.rc_deadband, Some(12));
        assert_eq!(merged.roll_max_rate, Some(450.0));
        assert_eq!(merged.schema_version, CONFIG_SCHEMA_VERSION);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let text = FerroConfig::default().to_toml().unwrap() + "\nmotor_authority = 1\n";
        assert!(toml::from_str::<FerroConfig>(&text).is_err());
    }

    #[test]
    fn validates_scalar_and_cross_field_firmware_ranges() {
        let mut config = FerroConfig::default();
        config.roll.p = 20.01;
        config.imu_lpf_alpha = f32::NAN;
        config.log_rate_divisor = 17;
        config.roll_center_rate = Some(400.0);
        config.roll_max_rate = Some(300.0);
        assert_eq!(config.validate().unwrap_err().len(), 4);
    }

    #[test]
    fn every_key_can_be_read_and_changed() {
        let mut config = FerroConfig::default();
        for key in ConfigKey::ALL {
            let value = match key {
                ConfigKey::LogRateDivisor => "2",
                ConfigKey::RcDeadband => "10",
                ConfigKey::RollCenterRate
                | ConfigKey::PitchCenterRate
                | ConfigKey::YawCenterRate => "50",
                ConfigKey::RollMaxRate | ConfigKey::PitchMaxRate | ConfigKey::YawMaxRate => "300",
                _ => "0.5",
            };
            config.set_from_str(key, value).unwrap();
            assert!(config.get(key).is_some());
        }
    }

    #[test]
    fn dotted_and_dashed_keys_are_friendly_aliases() {
        assert_eq!("roll.p".parse::<ConfigKey>().unwrap(), ConfigKey::RollP);
        assert_eq!(
            "imu-lpf-alpha".parse::<ConfigKey>().unwrap(),
            ConfigKey::ImuLpfAlpha
        );
    }
}
