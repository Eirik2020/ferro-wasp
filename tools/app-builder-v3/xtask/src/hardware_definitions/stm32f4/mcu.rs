//! Minimal STM32F4 MCU and clock declarations.

/// STM32F4 device selected by a board declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mcu {
    /// STM32F405 device family.
    Stm32f405,
}

/// Selected MCU and its clock configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct McuDeclaration {
    /// MCU device used by the board.
    pub device: Mcu,

    /// Clock source and resulting system frequency.
    pub clock: ClockDeclaration,
}

impl McuDeclaration {
    /// Creates an MCU declaration with its selected clock configuration.
    pub const fn new(device: Mcu, clock: ClockDeclaration) -> Self {
        Self { device, clock }
    }
}

/// Clock source and resulting system clock frequency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockDeclaration {
    /// Oscillator source used to derive the system clock.
    pub source: ClockSource,

    /// Resulting system clock frequency in hertz.
    pub system_frequency_hz: u32,
}

impl ClockDeclaration {
    /// Selects the internal high-speed oscillator.
    pub const fn hsi(system_frequency_hz: u32) -> Self {
        Self {
            source: ClockSource::Hsi,
            system_frequency_hz,
        }
    }

    /// Selects an external high-speed oscillator.
    pub const fn hse(external_frequency_hz: u32, system_frequency_hz: u32) -> Self {
        Self {
            source: ClockSource::Hse {
                frequency_hz: external_frequency_hz,
            },
            system_frequency_hz,
        }
    }
}

/// Oscillator source selected by the board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockSource {
    /// Internal high-speed oscillator.
    Hsi,

    /// External high-speed oscillator.
    Hse {
        /// External oscillator frequency in hertz.
        frequency_hz: u32,
    },
}
