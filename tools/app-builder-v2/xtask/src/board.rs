//! HAL-independent board declaration and basic declaration validation.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};

use crate::hw_resources::{HardwareResource, Target};

/// RTIC monotonic configuration retained during the hardware-model migration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonotonicDeclaration {
    /// Cortex-M SysTick monotonic clock.
    SysTick {
        /// Rust identifier used by generated task bodies.
        id: &'static str,

        /// Input clock frequency in hertz.
        clock_hz: u32,
    },
}

/// Hardware target and physical resources available to an application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDeclaration {
    /// Stable board identifier used by the generator.
    pub id: &'static str,

    /// MCU and clock configuration for the board.
    pub target: Target,

    /// RTIC monotonic configuration.
    pub monotonic: MonotonicDeclaration,

    /// Physical hardware resources declared by the board.
    pub hardware: &'static [HardwareResource],
}

/// Validates generic board identifiers, clock invariants, and resource IDs.
pub(crate) fn validate(board: &BoardDeclaration) -> Result<()> {
    syn::parse_str::<syn::Ident>(board.id)
        .with_context(|| format!("board ID `{}` is not a Rust identifier", board.id))?;
    if board.target.clock.sysclk_hz == 0 {
        bail!(
            "board `{}` system clock must be greater than zero",
            board.id
        );
    }

    let mut hardware_ids = BTreeSet::new();
    for hardware in board.hardware {
        let id = hardware.id();
        syn::parse_str::<syn::Ident>(id)
            .with_context(|| format!("board hardware ID `{id}` is not a Rust identifier"))?;
        if !hardware_ids.insert(id) {
            bail!("board `{}` repeats hardware resource `{id}`", board.id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hw_resources::{
        Clock, ClockSource, Drive, Gpio, GpioMode, InterruptEdge, Level, Mcu, PinId, Pull,
    };

    const LED2: HardwareResource =
        HardwareResource::Gpio(Gpio::output("led2", PinId::new(0, 5), Level::Low));
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "nucleo_f401re",
        target: Target {
            mcu: Mcu::Stm32F401,
            clock: Clock {
                source: ClockSource::InternalHighSpeed,
                sysclk_hz: 84_000_000,
            },
        },
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[LED2],
    };

    #[test]
    fn represents_port_zero_pin_five_without_a_hal_pin_type() {
        assert_eq!(LED2.pin(), PinId::new(0, 5));
        validate(&BOARD).unwrap();
    }

    #[test]
    fn output_constructor_applies_gpio_defaults() {
        let gpio = LED2.gpio().unwrap();
        assert_eq!(gpio.pull, Pull::None);
        assert_eq!(
            gpio.mode,
            GpioMode::Output {
                drive: Drive::PushPull,
                initial_level: Level::Low,
            }
        );
    }

    #[test]
    fn input_modifiers_apply_button_configuration() {
        const BUTTON: HardwareResource = HardwareResource::Gpio(
            Gpio::input("user_button", PinId::new(2, 13))
                .pull_up()
                .interrupt_on(InterruptEdge::Falling),
        );
        let gpio = BUTTON.gpio().unwrap();
        assert_eq!(gpio.pull, Pull::Up);
        assert_eq!(
            gpio.mode,
            GpioMode::Input {
                interrupt: Some(crate::hw_resources::ExternalInterrupt {
                    edge: InterruptEdge::Falling,
                }),
            }
        );
    }

    #[test]
    fn rejects_zero_system_clock() {
        const INVALID: BoardDeclaration = BoardDeclaration {
            target: Target {
                clock: Clock {
                    sysclk_hz: 0,
                    ..BOARD.target.clock
                },
                ..BOARD.target
            },
            ..BOARD
        };

        assert!(
            validate(&INVALID)
                .unwrap_err()
                .to_string()
                .contains("greater than zero")
        );
    }

    #[test]
    fn rejects_duplicate_hardware_ids() {
        const DUPLICATE: BoardDeclaration = BoardDeclaration {
            hardware: &[LED2, LED2],
            ..BOARD
        };

        assert!(
            validate(&DUPLICATE)
                .unwrap_err()
                .to_string()
                .contains("repeats hardware resource")
        );
    }
}
