//! Firmware-owned public configuration key metadata shared with host tools.

use core::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigKey {
    RollP,
    RollI,
    RollD,
    PitchP,
    PitchI,
    PitchD,
    YawP,
    YawI,
    YawD,
    ImuLpfAlpha,
    LogRateDivisor,
    RcDeadband,
    RollCenterRate,
    RollMaxRate,
    RollExpo,
    PitchCenterRate,
    PitchMaxRate,
    PitchExpo,
    YawCenterRate,
    YawMaxRate,
    YawExpo,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfigValueSpec {
    pub minimum: f32,
    pub maximum: f32,
    pub integer: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownConfigKey;

impl fmt::Display for UnknownConfigKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown FerroWasp configuration key")
    }
}

impl core::error::Error for UnknownConfigKey {}

impl ConfigValueSpec {
    pub fn accepts(self, value: f32) -> bool {
        value.is_finite()
            && (self.minimum..=self.maximum).contains(&value)
            && (!self.integer || value == value as u32 as f32)
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for ConfigKey {
    type Err = UnknownConfigKey;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|key| {
                value.len() == key.name().len()
                    && value
                        .bytes()
                        .zip(key.name().bytes())
                        .all(|(candidate, expected)| {
                            let candidate = match candidate {
                                b'.' | b'-' => b'_',
                                other => other.to_ascii_lowercase(),
                            };
                            candidate == expected
                        })
            })
            .ok_or(UnknownConfigKey)
    }
}

impl ConfigKey {
    pub const ALL: [Self; 21] = [
        Self::RollP,
        Self::RollI,
        Self::RollD,
        Self::PitchP,
        Self::PitchI,
        Self::PitchD,
        Self::YawP,
        Self::YawI,
        Self::YawD,
        Self::ImuLpfAlpha,
        Self::LogRateDivisor,
        Self::RcDeadband,
        Self::RollCenterRate,
        Self::RollMaxRate,
        Self::RollExpo,
        Self::PitchCenterRate,
        Self::PitchMaxRate,
        Self::PitchExpo,
        Self::YawCenterRate,
        Self::YawMaxRate,
        Self::YawExpo,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::RollP => "roll_p",
            Self::RollI => "roll_i",
            Self::RollD => "roll_d",
            Self::PitchP => "pitch_p",
            Self::PitchI => "pitch_i",
            Self::PitchD => "pitch_d",
            Self::YawP => "yaw_p",
            Self::YawI => "yaw_i",
            Self::YawD => "yaw_d",
            Self::ImuLpfAlpha => "imu_lpf_alpha",
            Self::LogRateDivisor => "log_rate_divisor",
            Self::RcDeadband => "rc_deadband",
            Self::RollCenterRate => "roll_center_rate",
            Self::RollMaxRate => "roll_max_rate",
            Self::RollExpo => "roll_expo",
            Self::PitchCenterRate => "pitch_center_rate",
            Self::PitchMaxRate => "pitch_max_rate",
            Self::PitchExpo => "pitch_expo",
            Self::YawCenterRate => "yaw_center_rate",
            Self::YawMaxRate => "yaw_max_rate",
            Self::YawExpo => "yaw_expo",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.name() == value)
    }

    pub const fn value_spec(self) -> ConfigValueSpec {
        match self {
            Self::RollP
            | Self::RollI
            | Self::RollD
            | Self::PitchP
            | Self::PitchI
            | Self::PitchD
            | Self::YawP
            | Self::YawI
            | Self::YawD => ConfigValueSpec {
                minimum: 0.0,
                maximum: 20.0,
                integer: false,
            },
            Self::ImuLpfAlpha | Self::RollExpo | Self::PitchExpo | Self::YawExpo => {
                ConfigValueSpec {
                    minimum: 0.0,
                    maximum: 1.0,
                    integer: false,
                }
            }
            Self::LogRateDivisor => ConfigValueSpec {
                minimum: 1.0,
                maximum: 16.0,
                integer: true,
            },
            Self::RcDeadband => ConfigValueSpec {
                minimum: 0.0,
                maximum: 100.0,
                integer: true,
            },
            Self::RollCenterRate | Self::PitchCenterRate | Self::YawCenterRate => ConfigValueSpec {
                minimum: 10.0,
                maximum: 500.0,
                integer: false,
            },
            Self::RollMaxRate | Self::PitchMaxRate | Self::YawMaxRate => ConfigValueSpec {
                minimum: 10.0,
                maximum: 1200.0,
                integer: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_are_unique() {
        for (index, key) in ConfigKey::ALL.into_iter().enumerate() {
            assert_eq!(ConfigKey::parse(key.name()), Some(key));
            assert!(
                ConfigKey::ALL[..index]
                    .iter()
                    .all(|other| other.name() != key.name())
            );
        }
    }

    #[test]
    fn scalar_ranges_match_the_public_usb_contract() {
        assert!(ConfigKey::RollP.value_spec().accepts(20.0));
        assert!(!ConfigKey::RollP.value_spec().accepts(20.01));
        assert!(ConfigKey::RcDeadband.value_spec().accepts(8.0));
        assert!(!ConfigKey::RcDeadband.value_spec().accepts(8.5));
        assert!(ConfigKey::RollCenterRate.value_spec().accepts(500.0));
        assert!(!ConfigKey::RollCenterRate.value_spec().accepts(501.0));
        assert!(ConfigKey::RollMaxRate.value_spec().accepts(1200.0));
        assert!(!ConfigKey::RollExpo.value_spec().accepts(1.1));
    }
}
