//! STM32F4 timer identifiers used by hardware declarations.

/// Identifies one STM32F4 general-purpose timer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TimerPeripheral {
    /// Timer 2.
    Tim2,
    /// Timer 4.
    Tim4,
    /// Timer 5.
    Tim5,
    /// Timer 6, whose update vector is shared with the DAC.
    Tim6,
}

impl TimerPeripheral {
    /// PAC interrupt vector owned by this timer's update event.
    pub const fn update_interrupt(self) -> &'static str {
        match self {
            Self::Tim2 => "TIM2",
            Self::Tim4 => "TIM4",
            Self::Tim5 => "TIM5",
            Self::Tim6 => "TIM6_DAC",
        }
    }
}

/// One named timer peripheral physically available on a board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerHardwareDeclaration {
    /// Stable physical identifier consumed by application bindings.
    pub id: &'static str,

    /// Physical STM32F4 timer peripheral reserved by this declaration.
    pub peripheral: TimerPeripheral,
}

impl TimerHardwareDeclaration {
    /// Creates a named physical timer declaration.
    pub const fn new(id: &'static str, peripheral: TimerPeripheral) -> Self {
        Self { id, peripheral }
    }
}
