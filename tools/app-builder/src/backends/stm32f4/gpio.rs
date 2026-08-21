//! Declarative STM32F4 GPIO configuration.
//!
//! These types record board-owned electrical facts without embedding concrete
//! `stm32f4xx-hal` pin types in the board declaration.

use super::pins::PinId;

/// Configures the internal pull resistor for a GPIO pin.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Pull {
    /// No internal pull resistor.
    #[default]
    None,

    /// Enable the internal pull-up resistor.
    Up,

    /// Enable the internal pull-down resistor.
    Down,
}

/// Configures the electrical output-driver mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Drive {
    /// Actively drives both high and low output levels.
    #[default]
    PushPull,
}

/// Represents a digital GPIO logic level.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Level {
    /// Logic-low electrical level.
    #[default]
    Low,

    /// Logic-high electrical level.
    High,
}

/// Selects the electrical edge that triggers a GPIO interrupt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptEdge {
    /// Trigger when the pin transitions from low to high.
    Rising,

    /// Trigger when the pin transitions from high to low.
    Falling,

    /// Trigger on either a rising or falling transition.
    Both,
}

/// Describes external-interrupt configuration for a GPIO input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalInterrupt {
    /// Electrical edge that triggers the interrupt.
    pub edge: InterruptEdge,
}

/// Describes whether a GPIO resource is configured as an input or output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpioMode {
    /// Configure the pin as a digital input.
    Input {
        /// Optional external-interrupt configuration.
        interrupt: Option<ExternalInterrupt>,
    },

    /// Configure the pin as a digital output.
    Output {
        /// Electrical output-driver mode.
        drive: Drive,

        /// Output level applied during initialization.
        initial_level: Level,
    },
}

/// One named GPIO resource physically present on a board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpioHardwareDeclaration {
    /// Stable identifier used by task-local composition bindings.
    pub id: &'static str,

    /// Physical pin assigned to this resource.
    pub pin: PinId,

    /// Internal pull-resistor configuration.
    pub pull: Pull,

    /// Input or output configuration applied during initialization.
    pub mode: GpioMode,
}

impl GpioHardwareDeclaration {
    /// Creates a digital input without a pull resistor or interrupt.
    pub const fn input(id: &'static str, pin: PinId) -> Self {
        Self {
            id,
            pin,
            pull: Pull::None,
            mode: GpioMode::Input { interrupt: None },
        }
    }

    /// Creates a push-pull digital output with the supplied initial level.
    pub const fn output(id: &'static str, pin: PinId, initial_level: Level) -> Self {
        Self {
            id,
            pin,
            pull: Pull::None,
            mode: GpioMode::Output {
                drive: Drive::PushPull,
                initial_level,
            },
        }
    }

    /// Enables the internal pull-up resistor.
    pub const fn pull_up(mut self) -> Self {
        self.pull = Pull::Up;
        self
    }

    /// Enables the internal pull-down resistor.
    pub const fn pull_down(mut self) -> Self {
        self.pull = Pull::Down;
        self
    }

    /// Enables an external interrupt on an input resource.
    ///
    /// # Panics
    ///
    /// Panics when called on an output resource. Board declarations are
    /// expected to be constants, so this is normally a compile-time error.
    pub const fn interrupt_on(mut self, edge: InterruptEdge) -> Self {
        self.mode = match self.mode {
            GpioMode::Input { .. } => GpioMode::Input {
                interrupt: Some(ExternalInterrupt { edge }),
            },
            GpioMode::Output { .. } => {
                panic!("GPIO interrupts can only be configured on input resources")
            }
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::stm32f4::pins::GpioPort;

    #[test]
    fn button_modifiers_preserve_electrical_configuration() {
        const BUTTON: GpioHardwareDeclaration =
            GpioHardwareDeclaration::input("user_button", PinId::new(GpioPort::C, 13))
                .pull_up()
                .interrupt_on(InterruptEdge::Falling);

        assert_eq!(BUTTON.pull, Pull::Up);
        assert_eq!(
            BUTTON.mode,
            GpioMode::Input {
                interrupt: Some(ExternalInterrupt {
                    edge: InterruptEdge::Falling,
                }),
            }
        );
    }
}
