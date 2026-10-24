//! The config owner (design §14): the image the craft flies, the image the
//! next boot will load, and the one validation both pass. A change to a live
//! key applies to both; a change to a boot-only key, such as the RC protocol
//! or a port binding, waits in the next-boot image for a reboot. The
//! coordinator runs it, so every change passes the same checks the next boot
//! will apply, and a change it accepts is never rejected at boot.
//!
//! Requirements: CONFIG-001, CONFIG-006, CONFIG-015, CONFIG-016, CONFIG-026
//! Safety-critical.

use ferrowasp_control::calibration::accel_level::AccelLevel;
use ferrowasp_serial::{LogicalSerialPort, SerialBindings, SerialFunction};

use crate::keys::ConfigKey;
use crate::stored::StoredConfig;

/// Whether a configuration holds only values its keys accept. A value is
/// checked by setting it again: the same range and cross-field checks a
/// change passes.
pub fn validate(config: &StoredConfig) -> bool {
    ConfigKey::ALL.iter().all(|key| {
        let mut candidate = *config;
        candidate.set(*key, config.get(*key)) && candidate == *config
    })
}

/// When a key's change takes effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyClass {
    /// At once, under an arm inhibit.
    Live,
    /// After the next reboot: it sets how a port is opened.
    BootOnly,
}

pub const fn class(key: ConfigKey) -> KeyClass {
    match key {
        ConfigKey::RcProtocol => KeyClass::BootOnly,
        _ => KeyClass::Live,
    }
}

/// A validated change, before it is committed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    pub next_boot: StoredConfig,
    /// The active image after the change; `None` when it waits for a reboot.
    pub active: Option<StoredConfig>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigRefusal {
    /// A value outside its key's range, or a combination the checks refuse.
    OutOfRange,
}

pub struct ConfigOwner {
    active: StoredConfig,
    next_boot: StoredConfig,
    /// Advances with every committed change, so a reader sees one.
    sequence: u32,
}

impl ConfigOwner {
    /// Start from the configuration the board booted with.
    pub const fn new(boot: StoredConfig) -> Self {
        Self {
            active: boot,
            next_boot: boot,
            sequence: 1,
        }
    }

    pub const fn active(&self) -> StoredConfig {
        self.active
    }

    pub const fn next_boot(&self) -> StoredConfig {
        self.next_boot
    }

    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    /// Check setting `key` to `value`; nothing changes until
    /// [`Self::commit`].
    pub fn propose_set(&self, key: ConfigKey, value: f32) -> Result<Change, ConfigRefusal> {
        let mut next_boot = self.next_boot;
        if !next_boot.set(key, value) {
            return Err(ConfigRefusal::OutOfRange);
        }
        let active = match class(key) {
            KeyClass::BootOnly => None,
            KeyClass::Live => {
                let mut active = self.active;
                if !active.set(key, value) {
                    return Err(ConfigRefusal::OutOfRange);
                }
                Some(active)
            }
        };
        Ok(Change { next_boot, active })
    }

    /// Check binding `port` to `function`, which applies after a reboot.
    pub fn propose_binding(
        &self,
        port: LogicalSerialPort,
        function: SerialFunction,
        defaults: SerialBindings,
    ) -> Change {
        let mut next_boot = self.next_boot;
        let mut bindings = next_boot.serial_bindings.unwrap_or(defaults);
        bindings.set(port, function);
        next_boot.serial_bindings = Some(bindings);
        Change {
            next_boot,
            active: None,
        }
    }

    /// Check replacing the whole configuration, as a configurator's commit
    /// or restoring defaults does: live keys apply at once, boot-only ones
    /// wait, and the accelerometer level, which belongs to the craft, stays.
    pub fn propose_replace(&self, config: StoredConfig) -> Result<Change, ConfigRefusal> {
        if !validate(&config) {
            return Err(ConfigRefusal::OutOfRange);
        }
        let next_boot = StoredConfig {
            accel_level: self.next_boot.accel_level,
            ..config
        };
        let active = StoredConfig {
            tuning: config.tuning,
            log_rate_divisor: config.log_rate_divisor,
            ..self.active
        };
        Ok(Change {
            next_boot,
            active: Some(active),
        })
    }

    /// A new accelerometer level, from a calibration that already checked
    /// it: it applies at once and is saved with the next save.
    pub fn propose_level(&self, level: AccelLevel) -> Change {
        Change {
            next_boot: StoredConfig {
                accel_level: level,
                ..self.next_boot
            },
            active: Some(StoredConfig {
                accel_level: level,
                ..self.active
            }),
        }
    }

    /// Adopt a change that was checked against the current images.
    pub fn commit(&mut self, change: Change) {
        self.next_boot = change.next_boot;
        if let Some(active) = change.active {
            self.active = active;
        }
        self.sequence = self.sequence.wrapping_add(1).max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_serial::RcProtocol;

    fn owner() -> ConfigOwner {
        ConfigOwner::new(StoredConfig::first_hop_default())
    }

    #[test]
    fn the_board_default_is_valid_and_a_value_out_of_range_is_not() {
        assert!(validate(&StoredConfig::first_hop_default()));
        let mut config = StoredConfig::first_hop_default();
        config.tuning.rate_gains.pitch.d = -1.0;
        assert!(!validate(&config));
    }

    #[test]
    fn a_live_key_changes_both_images_once_committed() {
        let mut owner = owner();
        let change = owner.propose_set(ConfigKey::RollP, 0.07).unwrap();
        assert_eq!(
            owner.active().get(ConfigKey::RollP),
            StoredConfig::first_hop_default().get(ConfigKey::RollP)
        );
        owner.commit(change);
        assert_eq!(owner.active().get(ConfigKey::RollP), 0.07);
        assert_eq!(owner.next_boot().get(ConfigKey::RollP), 0.07);
        assert_eq!(owner.sequence(), 2);
    }

    #[test]
    fn a_boot_only_key_waits_for_a_reboot() {
        let mut owner = owner();
        let change = owner
            .propose_set(ConfigKey::RcProtocol, RcProtocol::Crsf as u8 as f32)
            .unwrap();
        assert_eq!(change.active, None);
        owner.commit(change);
        assert_eq!(owner.active().rc_protocol, RcProtocol::Sbus);
        assert_eq!(owner.next_boot().rc_protocol, RcProtocol::Crsf);
    }

    #[test]
    fn a_refused_change_changes_nothing() {
        let owner = owner();
        assert_eq!(
            owner.propose_set(ConfigKey::RollP, f32::NAN),
            Err(ConfigRefusal::OutOfRange)
        );
        assert_eq!(owner.active(), StoredConfig::first_hop_default());
        assert_eq!(owner.sequence(), 1);
    }

    #[test]
    fn a_replacement_keeps_the_crafts_level_and_waits_for_boot_only_keys() {
        let mut level = StoredConfig::first_hop_default();
        level.accel_level.offset_mg = [10, 0, 0];
        let mut owner = ConfigOwner::new(level);
        let mut replacement = StoredConfig::first_hop_default();
        replacement.tuning.rate_gains.yaw.p = 0.08;
        replacement.rc_protocol = RcProtocol::Crsf;
        let change = owner.propose_replace(replacement).unwrap();
        owner.commit(change);
        assert_eq!(owner.active().tuning.rate_gains.yaw.p, 0.08);
        assert_eq!(owner.active().rc_protocol, RcProtocol::Sbus);
        assert_eq!(owner.next_boot().rc_protocol, RcProtocol::Crsf);
        assert_eq!(owner.next_boot().accel_level.offset_mg, [10, 0, 0]);
    }
}
