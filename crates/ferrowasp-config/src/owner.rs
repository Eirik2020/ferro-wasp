//! The config owner (design §14): the image the craft flies, the image the
//! next boot will load, and the one validation both pass. A change to a live
//! key applies to both; a change to a boot-only key, such as the RC protocol
//! or a port binding, waits in the next-boot image for a reboot. The
//! coordinator runs it, so every change passes the same checks the next boot
//! will apply, and a change it accepts is never rejected at boot.
//!
//! Requirements: CONFIG-001, CONFIG-006, CONFIG-015, CONFIG-016, CONFIG-026, CONFIG-033, CONFIG-034
//! Safety-critical.

use ferrowasp_control::calibration::accel_level::AccelLevel;
use ferrowasp_serial::{
    LogicalSerialPort, SerialBindings, SerialFunction, SerialRoute, resolve_bindings,
};

use crate::keys::ConfigKey;
use crate::stored::StoredConfig;

/// The board's facts a configuration is checked against.
#[derive(Clone, Copy, Debug)]
pub struct BoardRules {
    /// Every UART the board routes, with what each can carry.
    pub routes: &'static [SerialRoute],
    /// The bindings of a configuration that saved none.
    pub default_bindings: SerialBindings,
}

/// Whether a configuration holds only values its keys accept and bindings
/// the board can carry. A value is checked by setting it again: the same
/// range and cross-field checks a change passes. Every bound function must
/// suit its port, in the configured RC protocol, no function may be on two
/// ports, and RC input must be bound, or the craft could never arm.
pub fn validate(config: &StoredConfig, board: &BoardRules) -> bool {
    let values = ConfigKey::ALL.iter().all(|key| {
        let mut candidate = *config;
        candidate.set(*key, config.get(*key)) && candidate == *config
    });
    let bindings = config.serial_bindings.unwrap_or(board.default_bindings);
    let rc_bound = LogicalSerialPort::ALL
        .iter()
        .any(|port| bindings.function(*port) == SerialFunction::RcInput);
    values
        && rc_bound
        && resolve_bindings(bindings, board.routes, config.rc_protocol)
            .issues()
            .is_empty()
}

/// When a key's change takes effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyClass {
    /// At once, under an arm inhibit.
    Live,
    /// After the next reboot: it sets how a port is opened.
    BootOnly,
}

/// Whether the next boot differs from what flies in a setting read only at
/// boot, so a reboot is needed for all of it to apply.
pub fn reboot_required(active: &StoredConfig, next_boot: &StoredConfig) -> bool {
    active.rc_protocol != next_boot.rc_protocol
        || active.serial_bindings != next_boot.serial_bindings
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
    /// A value outside its key's range, or a combination the checks or the
    /// board refuse.
    OutOfRange,
}

pub struct ConfigOwner {
    board: BoardRules,
    active: StoredConfig,
    next_boot: StoredConfig,
    /// Advances with every committed change, so a reader sees one.
    sequence: u32,
}

impl ConfigOwner {
    /// Start from the configuration the board booted with.
    pub const fn new(boot: StoredConfig, board: BoardRules) -> Self {
        Self {
            board,
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
        if !validate(&next_boot, &self.board) {
            return Err(ConfigRefusal::OutOfRange);
        }
        Ok(Change { next_boot, active })
    }

    /// Check binding `port` to `function`, which applies after a reboot. A
    /// function serves one port, so binding it moves it from the port it
    /// was on; unbinding RC input is refused.
    pub fn propose_binding(
        &self,
        port: LogicalSerialPort,
        function: SerialFunction,
    ) -> Result<Change, ConfigRefusal> {
        let mut next_boot = self.next_boot;
        let mut bindings = next_boot
            .serial_bindings
            .unwrap_or(self.board.default_bindings);
        if function != SerialFunction::None {
            for other in LogicalSerialPort::ALL {
                if bindings.function(other) == function {
                    bindings.set(other, SerialFunction::None);
                }
            }
        }
        bindings.set(port, function);
        next_boot.serial_bindings = Some(bindings);
        if !validate(&next_boot, &self.board) {
            return Err(ConfigRefusal::OutOfRange);
        }
        Ok(Change {
            next_boot,
            active: None,
        })
    }

    /// Check replacing the whole configuration, as a configurator's commit
    /// or restoring defaults does: live keys apply at once, boot-only ones
    /// wait, and the accelerometer level, which belongs to the craft, stays.
    pub fn propose_replace(&self, config: StoredConfig) -> Result<Change, ConfigRefusal> {
        if !validate(&config, &self.board) {
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
pub(crate) mod tests {
    use super::*;
    use ferrowasp_serial::{RcProtocol, SerialCapabilities, SerialProfile};

    const fn route(logical: LogicalSerialPort, sbus: bool) -> SerialRoute {
        SerialRoute {
            logical,
            peripheral: "test",
            tx_pin: "test",
            rx_pin: "test",
            profile: SerialProfile::disabled(),
            rx_dma: "test",
            tx_dma: Some("test"),
            capabilities: SerialCapabilities {
                sbus,
                crsf: true,
                mavlink: false,
                msp: true,
                esc_telemetry: true,
                cli: true,
                tx: true,
            },
        }
    }

    /// UART2 takes SBUS; UART3 only the plain protocols.
    pub(crate) const BOARD: BoardRules = BoardRules {
        routes: &[
            route(LogicalSerialPort::Uart2, true),
            route(LogicalSerialPort::Uart3, false),
        ],
        default_bindings: SerialBindings::none()
            .with(LogicalSerialPort::Uart2, SerialFunction::RcInput)
            .with(LogicalSerialPort::Uart3, SerialFunction::Configurator),
    };

    fn owner() -> ConfigOwner {
        ConfigOwner::new(StoredConfig::first_hop_default(), BOARD)
    }

    #[test]
    fn the_board_default_is_valid_and_a_value_out_of_range_is_not() {
        assert!(validate(&StoredConfig::first_hop_default(), &BOARD));
        let mut config = StoredConfig::first_hop_default();
        config.tuning.rate_gains.pitch.d = -1.0;
        assert!(!validate(&config, &BOARD));
    }

    #[test]
    fn bindings_the_board_cannot_carry_are_invalid() {
        let with = |bindings: SerialBindings| StoredConfig {
            serial_bindings: Some(bindings),
            ..StoredConfig::first_hop_default()
        };
        let none = SerialBindings::none();
        assert!(!validate(&with(none), &BOARD), "RC unbound");
        assert!(
            !validate(
                &with(none.with(LogicalSerialPort::Uart3, SerialFunction::RcInput)),
                &BOARD
            ),
            "SBUS on a port without an inverter"
        );
        assert!(
            !validate(
                &with(
                    none.with(LogicalSerialPort::Uart2, SerialFunction::RcInput)
                        .with(LogicalSerialPort::Uart3, SerialFunction::RcInput)
                ),
                &BOARD
            ),
            "one function on two ports"
        );
        assert!(
            !validate(
                &with(none.with(LogicalSerialPort::Uart6, SerialFunction::RcInput)),
                &BOARD
            ),
            "a port the board does not route"
        );
    }

    #[test]
    fn a_bind_moves_its_function_and_rc_cannot_be_unbound() {
        let mut owner = owner();
        let crsf = owner
            .propose_set(ConfigKey::RcProtocol, RcProtocol::Crsf as u8 as f32)
            .unwrap();
        owner.commit(crsf);
        let moved = owner
            .propose_binding(LogicalSerialPort::Uart3, SerialFunction::RcInput)
            .unwrap();
        let bindings = moved.next_boot.serial_bindings.unwrap();
        assert_eq!(
            bindings.function(LogicalSerialPort::Uart3),
            SerialFunction::RcInput
        );
        assert_eq!(
            bindings.function(LogicalSerialPort::Uart2),
            SerialFunction::None,
            "moved, not duplicated"
        );
        assert_eq!(
            owner.propose_binding(LogicalSerialPort::Uart2, SerialFunction::None),
            Err(ConfigRefusal::OutOfRange),
            "RC stays bound"
        );
        assert_eq!(
            ConfigOwner::new(StoredConfig::first_hop_default(), BOARD)
                .propose_binding(LogicalSerialPort::Uart3, SerialFunction::RcInput),
            Err(ConfigRefusal::OutOfRange),
            "SBUS cannot move to UART3"
        );
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
        assert!(reboot_required(&owner.active(), &owner.next_boot()));
        let mut fresh = ConfigOwner::new(StoredConfig::first_hop_default(), BOARD);
        let live = fresh.propose_set(ConfigKey::RollP, 0.07).unwrap();
        fresh.commit(live);
        assert!(!reboot_required(&fresh.active(), &fresh.next_boot()));
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
        let mut owner = ConfigOwner::new(level, BOARD);
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
