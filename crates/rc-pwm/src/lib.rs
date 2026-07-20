#![forbid(unsafe_code)]
#![no_std]

//! Utilities for generating RC servo / ESC pulse-width commands from an input command.
//!
//! This crate maps a configured command range to a pulse-width range and then
//! converts that pulse width into a PWM duty value for an
//! [`embedded_hal::pwm::SetDutyCycle`] implementation.
//!
//! Typical use cases:
//! - RC servos using a 50 Hz update rate and roughly 500-2000 µs pulses
//! - ESCs that accept a servo-style pulse-width command
//!
//! # Important
//!
//! The PWM peripheral itself must already be configured to the same frequency as
//! [`PwmConfig`]. This crate does **not** configure the timer peripheral; it only
//! computes the correct duty value.

use embedded_hal::pwm::SetDutyCycle;

/// Default PWM frequency in hertz.
///
/// This is the classic 50 Hz update rate commonly used for RC servos.
pub const DEFAULT_PWM_FREQUENCY_HZ: u16 = 50;

/// Default minimum pulse width in microseconds.
pub const DEFAULT_PULSE_MIN_US: u32 = 1000;

/// Default maximum pulse width in microseconds.
pub const DEFAULT_PULSE_MAX_US: u32 = 2_000;

/// Default minimum command value.
pub const DEFAULT_CMD_MIN: u16 = 0;

/// Default maximum command value.
pub const DEFAULT_CMD_MAX: u16 = 2_000;

/// Errors related to configuration validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The PWM frequency was outside the supported range.
    InvalidPwmFrequency,

    /// The pulse range was invalid, for example `min_us >= max_us`.
    InvalidPulseRange,

    /// The configured maximum pulse width does not fit inside a single PWM period.
    PulseTooWideForFrequency,

    /// The command range was invalid, for example `min >= max`.
    InvalidCmdRange,

    /// The provided command value was outside the configured command range.
    InvalidCmdValue,
}

/// Errors returned by [`PwmController`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerError<E> {
    /// A configuration or input validation error.
    Config(ConfigError),

    /// An error returned by the underlying PWM peripheral driver.
    Pwm(E),
}

impl<E> From<ConfigError> for ControllerError<E> {
    fn from(err: ConfigError) -> Self {
        Self::Config(err)
    }
}

/// A validated PWM frequency in hertz.
///
/// Valid values are restricted to `50..=500` Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PwmFrequency(u16);

impl PwmFrequency {
    /// Minimum supported PWM frequency in hertz.
    pub const MIN_HZ: u16 = 50;

    /// Maximum supported PWM frequency in hertz.
    pub const MAX_HZ: u16 = 500;

    /// Creates a validated PWM frequency.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidPwmFrequency`] if `hz` is outside `50..=500`.
    pub const fn try_new(hz: u16) -> Result<Self, ConfigError> {
        if hz >= Self::MIN_HZ && hz <= Self::MAX_HZ {
            Ok(Self(hz))
        } else {
            Err(ConfigError::InvalidPwmFrequency)
        }
    }

    /// Creates a validated PWM frequency in a const context.
    ///
    /// Invalid values cause a compile-time failure when used in a `const` context.
    pub const fn new_const(hz: u16) -> Self {
        match Self::try_new(hz) {
            Ok(v) => v,
            Err(_) => panic!("PWM frequency must be within 50-500 Hz"),
        }
    }

    /// Returns the frequency in hertz.
    pub const fn hz(self) -> u16 {
        self.0
    }
}

/// A validated pulse-width range in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseRange {
    min_us: u32,
    max_us: u32,
}

impl PulseRange {
    /// Creates a validated pulse range.
    ///
    /// The range must satisfy:
    /// - `min_us < max_us`
    /// - `max_us` must fit within one PWM period at `frequency`
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`ConfigError::InvalidPulseRange`] if `min_us >= max_us`
    /// - [`ConfigError::PulseTooWideForFrequency`] if `max_us` is longer than one PWM period
    pub const fn try_new(
        min_us: u32,
        max_us: u32,
        frequency: PwmFrequency,
    ) -> Result<Self, ConfigError> {
        if min_us >= max_us {
            return Err(ConfigError::InvalidPulseRange);
        }

        if (max_us as u64) * (frequency.hz() as u64) > 1_000_000 {
            return Err(ConfigError::PulseTooWideForFrequency);
        }

        Ok(Self { min_us, max_us })
    }

    /// Creates a validated pulse range in a const context.
    ///
    /// Invalid values cause a compile-time failure when used in a `const` context.
    pub const fn new_const(min_us: u32, max_us: u32, frequency: PwmFrequency) -> Self {
        match Self::try_new(min_us, max_us, frequency) {
            Ok(v) => v,
            Err(ConfigError::InvalidPulseRange) => {
                panic!("pulse range must satisfy min_us < max_us")
            }
            Err(ConfigError::PulseTooWideForFrequency) => {
                panic!("max_us does not fit inside one PWM period")
            }
            Err(_) => panic!("invalid pulse range"),
        }
    }

    /// Returns the minimum pulse width in microseconds.
    pub const fn min_us(self) -> u32 {
        self.min_us
    }

    /// Returns the maximum pulse width in microseconds.
    pub const fn max_us(self) -> u32 {
        self.max_us
    }
}

/// A validated command input range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmdRange {
    min: u16,
    max: u16,
}

impl CmdRange {
    /// Creates a validated command range.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidCmdRange`] if `min >= max`.
    pub const fn try_new(min: u16, max: u16) -> Result<Self, ConfigError> {
        if min < max {
            Ok(Self { min, max })
        } else {
            Err(ConfigError::InvalidCmdRange)
        }
    }

    /// Creates a validated command range in a const context.
    ///
    /// Invalid values cause a compile-time failure when used in a `const` context.
    pub const fn new_const(min: u16, max: u16) -> Self {
        match Self::try_new(min, max) {
            Ok(v) => v,
            Err(_) => panic!("command range must satisfy min < max"),
        }
    }

    /// Returns `true` if `value` is inside the configured command range.
    pub const fn contains(self, value: u16) -> bool {
        value >= self.min && value <= self.max
    }

    /// Returns the minimum command value.
    pub const fn min(self) -> u16 {
        self.min
    }

    /// Returns the maximum command value.
    pub const fn max(self) -> u16 {
        self.max
    }
}

/// Validated PWM configuration.
///
/// This type stores:
/// - the PWM frequency
/// - the pulse-width range in microseconds
/// - the input command range
///
/// The PWM peripheral must be configured to the same frequency as stored here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PwmConfig {
    frequency: PwmFrequency,
    pulse_range: PulseRange,
    cmd_range: CmdRange,
}

impl PwmConfig {
    /// Creates a fully validated PWM configuration.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] if any part of the configuration is invalid.
    pub const fn try_new(
        frequency_hz: u16,
        min_us: u32,
        max_us: u32,
        min_cmd: u16,
        max_cmd: u16,
    ) -> Result<Self, ConfigError> {
        let frequency = match PwmFrequency::try_new(frequency_hz) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        let pulse_range = match PulseRange::try_new(min_us, max_us, frequency) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        let cmd_range = match CmdRange::try_new(min_cmd, max_cmd) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        Ok(Self {
            frequency,
            pulse_range,
            cmd_range,
        })
    }

    /// Creates a fully validated PWM configuration in a const context.
    ///
    /// Invalid values cause a compile-time failure when used in a `const` context.
    pub const fn new_const(
        frequency_hz: u16,
        min_us: u32,
        max_us: u32,
        min_cmd: u16,
        max_cmd: u16,
    ) -> Self {
        match Self::try_new(frequency_hz, min_us, max_us, min_cmd, max_cmd) {
            Ok(v) => v,
            Err(ConfigError::InvalidPwmFrequency) => {
                panic!("PWM frequency must be within 50-500 Hz")
            }
            Err(ConfigError::InvalidPulseRange) => {
                panic!("pulse range must satisfy min_us < max_us")
            }
            Err(ConfigError::PulseTooWideForFrequency) => {
                panic!("max_us does not fit inside one PWM period")
            }
            Err(ConfigError::InvalidCmdRange) => {
                panic!("command range must satisfy min < max")
            }
            Err(_) => panic!("invalid PWM config"),
        }
    }

    /// Returns the default configuration as a const value.
    ///
    /// Defaults:
    /// - frequency: [`DEFAULT_PWM_FREQUENCY_HZ`]
    /// - pulse range: [`DEFAULT_PULSE_MIN_US`]..=[`DEFAULT_PULSE_MAX_US`]
    /// - command range: [`DEFAULT_CMD_MIN`]..=[`DEFAULT_CMD_MAX`]
    pub const fn default_const() -> Self {
        Self::new_const(
            DEFAULT_PWM_FREQUENCY_HZ,
            DEFAULT_PULSE_MIN_US,
            DEFAULT_PULSE_MAX_US,
            DEFAULT_CMD_MIN,
            DEFAULT_CMD_MAX,
        )
    }

    /// Returns a new configuration with an updated PWM frequency.
    ///
    /// The existing pulse range is revalidated against the new frequency.
    ///
    /// # Errors
    ///
    /// Returns an error if `hz` is invalid or if the current pulse range no longer
    /// fits inside one PWM period at the new frequency.
    pub fn frequency(mut self, hz: u16) -> Result<Self, ConfigError> {
        let frequency = PwmFrequency::try_new(hz)?;
        self.pulse_range = PulseRange::try_new(
            self.pulse_range.min_us(),
            self.pulse_range.max_us(),
            frequency,
        )?;
        self.frequency = frequency;
        Ok(self)
    }

    /// Returns a new configuration with an updated pulse-width range.
    ///
    /// # Errors
    ///
    /// Returns an error if the new pulse range is invalid or does not fit inside
    /// one PWM period at the current PWM frequency.
    pub fn pulse_range(mut self, min_us: u32, max_us: u32) -> Result<Self, ConfigError> {
        self.pulse_range = PulseRange::try_new(min_us, max_us, self.frequency)?;
        Ok(self)
    }

    /// Returns a new configuration with an updated command range.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidCmdRange`] if `min >= max`.
    pub fn command_range(mut self, min: u16, max: u16) -> Result<Self, ConfigError> {
        self.cmd_range = CmdRange::try_new(min, max)?;
        Ok(self)
    }

    /// Returns a new configuration with an updated PWM frequency.
    ///
    /// This method is intended for compile-time checked configuration. Invalid
    /// values cause a compile-time failure when used in a `const` context.
    ///
    /// # Important
    ///
    /// The PWM peripheral must also be configured to this same frequency.
    pub const fn with_frequency_const(mut self, hz: u16) -> Self {
        let frequency = PwmFrequency::new_const(hz);
        self.pulse_range = PulseRange::new_const(
            self.pulse_range.min_us(),
            self.pulse_range.max_us(),
            frequency,
        );
        self.frequency = frequency;
        self
    }

    /// Returns a new configuration with an updated pulse-width range.
    ///
    /// This method is intended for compile-time checked configuration. Invalid
    /// values cause a compile-time failure when used in a `const` context.
    pub const fn with_pulse_range_const(mut self, min_us: u32, max_us: u32) -> Self {
        self.pulse_range = PulseRange::new_const(min_us, max_us, self.frequency);
        self
    }

    /// Returns a new configuration with an updated command range.
    ///
    /// This method is intended for compile-time checked configuration. Invalid
    /// values cause a compile-time failure when used in a `const` context.
    pub const fn with_command_range_const(mut self, min: u16, max: u16) -> Self {
        self.cmd_range = CmdRange::new_const(min, max);
        self
    }

    /// Returns the configured PWM frequency.
    pub const fn pwm_frequency(self) -> PwmFrequency {
        self.frequency
    }

    /// Returns the configured pulse-width range.
    pub const fn pulse(self) -> PulseRange {
        self.pulse_range
    }

    /// Returns the configured command range.
    pub const fn command(self) -> CmdRange {
        self.cmd_range
    }
}

impl Default for PwmConfig {
    /// Returns the default PWM configuration.
    fn default() -> Self {
        Self::default_const()
    }
}

/// Controller that maps command values to PWM duty commands.
pub struct PwmController<P> {
    pwm_channel: P,
    cfg: PwmConfig,
    last_command: Option<u16>,
    last_pulse_width_us: Option<u32>,
}

impl<P: SetDutyCycle> PwmController<P> {
    /// Creates a new PWM controller using an already configured PWM channel.
    ///
    /// The PWM channel is driven fully off during initialization.
    ///
    /// # Errors
    ///
    /// Returns [`ControllerError::Pwm`] if the PWM driver fails while setting the
    /// channel fully off.
    pub fn new(mut pwm_channel: P, cfg: PwmConfig) -> Result<Self, ControllerError<P::Error>> {
        pwm_channel
            .set_duty_cycle_fully_off()
            .map_err(ControllerError::Pwm)?;

        Ok(Self {
            pwm_channel,
            cfg,
            last_command: None,
            last_pulse_width_us: None,
        })
    }

    /// Returns the active configuration.
    pub fn config(&self) -> PwmConfig {
        self.cfg
    }

    /// Returns the most recent command value that was successfully written.
    pub fn last_command(&self) -> Option<u16> {
        self.last_command
    }

    /// Returns the pulse width, in microseconds, for the most recent command
    /// that was successfully written.
    pub fn last_pulse_width_us(&self) -> Option<u32> {
        self.last_pulse_width_us
    }

    /// Sets the command value.
    ///
    /// The input value is linearly mapped from the configured command range to the
    /// configured pulse-width range, then converted into a PWM duty value.
    ///
    /// # Important
    ///
    /// The underlying PWM peripheral must already be configured to the same
    /// frequency as [`PwmConfig::pwm_frequency`].
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`ControllerError::Config`] if `value` is outside the configured command range
    /// - [`ControllerError::Pwm`] if the underlying PWM driver fails
    pub fn set_command(&mut self, value: u16) -> Result<(), ControllerError<P::Error>> {
        let command = self.cfg.command();

        if !command.contains(value) {
            return Err(ControllerError::Config(ConfigError::InvalidCmdValue));
        }

        let command_span = (command.max() - command.min()) as u32;
        let pulse = self.cfg.pulse();

        let pulse_span = pulse.max_us() - pulse.min_us();
        let command_offset = (value - command.min()) as u32;

        let pulse_width_us = pulse.min_us() + (command_offset * pulse_span) / command_span;

        let max_duty = self.pwm_channel.max_duty_cycle() as u32;

        let duty_ticks =
            ((pulse_width_us as u64 * max_duty as u64 * self.cfg.pwm_frequency().hz() as u64)
                / 1_000_000) as u16;

        self.pwm_channel
            .set_duty_cycle(duty_ticks)
            .map_err(ControllerError::Pwm)?;

        self.last_command = Some(value);
        self.last_pulse_width_us = Some(pulse_width_us);

        Ok(())
    }

    /// ESC-friendly alias for [`PwmController::set_command`].
    pub fn set_throttle(&mut self, value: u16) -> Result<(), ControllerError<P::Error>> {
        self.set_command(value)
    }

    /// Drives the PWM output fully off.
    ///
    /// # Errors
    ///
    /// Returns an error from the underlying PWM driver if disabling the channel fails.
    pub fn stop(&mut self) -> Result<(), P::Error> {
        self.pwm_channel.set_duty_cycle_fully_off()
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::pwm::{ErrorType, SetDutyCycle};
    use std::vec::Vec;

    #[derive(Debug, Default)]
    struct MockPwm {
        max_duty: u16,
        writes: Vec<u16>,
    }

    impl MockPwm {
        fn new(max_duty: u16) -> Self {
            Self {
                max_duty,
                writes: Vec::new(),
            }
        }
    }

    impl ErrorType for MockPwm {
        type Error = Infallible;
    }

    impl SetDutyCycle for MockPwm {
        fn max_duty_cycle(&self) -> u16 {
            self.max_duty
        }

        fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
            self.writes.push(duty);
            Ok(())
        }
    }

    #[test]
    fn rejects_invalid_pwm_configurations() {
        assert_eq!(
            PwmConfig::try_new(49, 1000, 2000, 0, 2000),
            Err(ConfigError::InvalidPwmFrequency)
        );
        assert_eq!(
            PwmConfig::try_new(400, 2000, 1000, 0, 2000),
            Err(ConfigError::InvalidPulseRange)
        );
        assert_eq!(
            PwmConfig::try_new(500, 1000, 2500, 0, 2000),
            Err(ConfigError::PulseTooWideForFrequency)
        );
        assert_eq!(
            PwmConfig::try_new(400, 1000, 2000, 2000, 0),
            Err(ConfigError::InvalidCmdRange)
        );
    }

    #[test]
    fn maps_command_range_to_pwm_duty_ticks() {
        let cfg = PwmConfig::try_new(400, 1000, 2000, 0, 2000).unwrap();
        let mut controller = PwmController::new(MockPwm::new(u16::MAX), cfg).unwrap();

        controller.set_command(0).unwrap();
        controller.set_command(1000).unwrap();
        controller.set_command(2000).unwrap();

        assert_eq!(controller.pwm_channel.writes, [0, 26214, 39321, 52428]);
        assert_eq!(controller.last_command(), Some(2000));
        assert_eq!(controller.last_pulse_width_us(), Some(2000));
    }

    #[test]
    fn rejects_out_of_range_commands_without_touching_pwm() {
        let cfg = PwmConfig::try_new(400, 1000, 2000, 100, 200).unwrap();
        let mut controller = PwmController::new(MockPwm::new(10_000), cfg).unwrap();

        let err = controller.set_command(99).unwrap_err();

        assert_eq!(err, ControllerError::Config(ConfigError::InvalidCmdValue));
        assert_eq!(controller.pwm_channel.writes, [0]);
        assert_eq!(controller.last_command(), None);
        assert_eq!(controller.last_pulse_width_us(), None);
    }
}
