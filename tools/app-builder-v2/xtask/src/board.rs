use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mcu {
    Stm32F401RE,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockSource {
    Hsi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockDeclaration {
    pub source: ClockSource,
    pub sysclk_hz: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonotonicDeclaration {
    SysTick { id: &'static str, clock_hz: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceId(&'static str);

impl ResourceId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalPin(&'static str);

impl PhysicalPin {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputDrive {
    PushPull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum LogicLevel {
    Low,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveLevel {
    Low,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputPull {
    Up,
    #[allow(dead_code)]
    Down,
    #[allow(dead_code)]
    Floating,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptEdge {
    #[allow(dead_code)]
    Rising,
    Falling,
    #[allow(dead_code)]
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareDeclaration {
    DigitalOutput {
        id: ResourceId,
        pin: PhysicalPin,
        drive: OutputDrive,
        initial: LogicLevel,
        active: ActiveLevel,
    },
    DigitalInput {
        id: ResourceId,
        pin: PhysicalPin,
        pull: InputPull,
        active: ActiveLevel,
        interrupt: Option<InterruptEdge>,
    },
}

impl HardwareDeclaration {
    pub const fn digital_output(id: ResourceId, pin: PhysicalPin) -> Self {
        Self::DigitalOutput {
            id,
            pin,
            drive: OutputDrive::PushPull,
            initial: LogicLevel::Low,
            active: ActiveLevel::High,
        }
    }

    /// Declares an active-low, pull-up digital input with a falling-edge EXTI.
    pub const fn exti_input(id: ResourceId, pin: PhysicalPin) -> Self {
        Self::DigitalInput {
            id,
            pin,
            pull: InputPull::Up,
            active: ActiveLevel::Low,
            interrupt: Some(InterruptEdge::Falling),
        }
    }

    #[allow(dead_code)]
    pub const fn with_drive(self, drive: OutputDrive) -> Self {
        match self {
            Self::DigitalOutput {
                id,
                pin,
                initial,
                active,
                ..
            } => Self::DigitalOutput {
                id,
                pin,
                drive,
                initial,
                active,
            },
            other => other,
        }
    }

    #[allow(dead_code)]
    pub const fn with_initial(self, initial: LogicLevel) -> Self {
        match self {
            Self::DigitalOutput {
                id,
                pin,
                drive,
                active,
                ..
            } => Self::DigitalOutput {
                id,
                pin,
                drive,
                initial,
                active,
            },
            other => other,
        }
    }

    #[allow(dead_code)]
    pub const fn with_active_level(self, active: ActiveLevel) -> Self {
        match self {
            Self::DigitalOutput {
                id,
                pin,
                drive,
                initial,
                ..
            } => Self::DigitalOutput {
                id,
                pin,
                drive,
                initial,
                active,
            },
            Self::DigitalInput {
                id,
                pin,
                pull,
                interrupt,
                ..
            } => Self::DigitalInput {
                id,
                pin,
                pull,
                active,
                interrupt,
            },
        }
    }

    pub const fn id(&self) -> &'static str {
        match self {
            Self::DigitalOutput { id, .. } | Self::DigitalInput { id, .. } => id.as_str(),
        }
    }

    pub const fn pin(&self) -> PhysicalPin {
        match self {
            Self::DigitalOutput { pin, .. } | Self::DigitalInput { pin, .. } => *pin,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDeclaration {
    pub id: &'static str,
    pub mcu: Mcu,
    pub clocks: ClockDeclaration,
    pub monotonic: MonotonicDeclaration,
    pub hardware: &'static [HardwareDeclaration],
}

pub(crate) fn validate(board: &BoardDeclaration) -> Result<()> {
    syn::parse_str::<syn::Ident>(board.id)
        .with_context(|| format!("board ID `{}` is not a Rust identifier", board.id))?;
    if board.clocks.sysclk_hz == 0 {
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
        if hardware.pin().as_str().is_empty() {
            bail!(
                "board `{}` hardware resource `{id}` has an empty physical pin",
                board.id
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LED2: HardwareDeclaration =
        HardwareDeclaration::digital_output(ResourceId::new("led2"), PhysicalPin::new("PA5"));
    const BOARD: BoardDeclaration = BoardDeclaration {
        id: "nucleo_f401re",
        mcu: Mcu::Stm32F401RE,
        clocks: ClockDeclaration {
            source: ClockSource::Hsi,
            sysclk_hz: 84_000_000,
        },
        monotonic: MonotonicDeclaration::SysTick {
            id: "Mono",
            clock_hz: 84_000_000,
        },
        hardware: &[LED2],
    };

    #[test]
    fn represents_pa5_without_a_hal_pin_type() {
        assert_eq!(LED2.pin(), PhysicalPin::new("PA5"));
        validate(&BOARD).unwrap();
    }

    #[test]
    fn constructor_applies_digital_output_defaults() {
        let HardwareDeclaration::DigitalOutput {
            drive,
            initial,
            active,
            ..
        } = LED2
        else {
            panic!("expected digital output")
        };
        assert_eq!(drive, OutputDrive::PushPull);
        assert_eq!(initial, LogicLevel::Low);
        assert_eq!(active, ActiveLevel::High);
    }

    #[test]
    fn exti_input_constructor_applies_button_defaults() {
        const BUTTON: HardwareDeclaration = HardwareDeclaration::exti_input(
            ResourceId::new("user_button"),
            PhysicalPin::new("PC13"),
        );
        let HardwareDeclaration::DigitalInput {
            pull,
            active,
            interrupt,
            ..
        } = BUTTON
        else {
            panic!("expected digital input")
        };
        assert_eq!(pull, InputPull::Up);
        assert_eq!(active, ActiveLevel::Low);
        assert_eq!(interrupt, Some(InterruptEdge::Falling));
    }

    #[test]
    fn rejects_empty_physical_pin_names() {
        const INVALID: BoardDeclaration = BoardDeclaration {
            hardware: &[HardwareDeclaration::digital_output(
                ResourceId::new("invalid"),
                PhysicalPin::new(""),
            )],
            ..BOARD
        };

        assert!(
            validate(&INVALID)
                .unwrap_err()
                .to_string()
                .contains("empty physical pin")
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
