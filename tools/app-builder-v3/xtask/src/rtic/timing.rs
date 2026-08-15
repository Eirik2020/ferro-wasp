//! Application timing declarations used by generated RTIC apps.

/// Monotonic timer selected by an application composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonotonicDeclaration {
    /// Cortex-M SysTick monotonic exposed to task bodies as `Mono`.
    SysTick {
        /// Number of monotonic ticks per second.
        tick_hz: u32,
    },

    /// Board-declared timer exposed to task bodies as `Mono`.
    Timer {
        /// Stable board timer identifier.
        hardware_id: &'static str,

        /// Number of monotonic ticks per second.
        tick_hz: u32,
    },
}

impl MonotonicDeclaration {
    /// Selects a SysTick monotonic with the supplied tick frequency.
    pub const fn systick(tick_hz: u32) -> Self {
        Self::SysTick { tick_hz }
    }

    /// Selects a named board timer as the application-wide monotonic.
    pub const fn timer(hardware_id: &'static str, tick_hz: u32) -> Self {
        Self::Timer {
            hardware_id,
            tick_hz,
        }
    }

    /// Returns the configured monotonic tick frequency.
    pub const fn tick_hz(self) -> u32 {
        match self {
            Self::SysTick { tick_hz } | Self::Timer { tick_hz, .. } => tick_hz,
        }
    }

    /// Returns the named board timer consumed by this declaration, if any.
    pub const fn hardware_id(self) -> Option<&'static str> {
        match self {
            Self::SysTick { .. } => None,
            Self::Timer { hardware_id, .. } => Some(hardware_id),
        }
    }
}

/// Board timer made available as a general synchronous initialization delay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitDelayDeclaration {
    /// Stable board timer identifier.
    pub hardware_id: &'static str,

    /// Number of delay timer ticks per second.
    pub tick_hz: u32,
}

impl InitDelayDeclaration {
    /// Selects a named board timer for synchronous initialization delays.
    pub const fn timer(hardware_id: &'static str, tick_hz: u32) -> Self {
        Self {
            hardware_id,
            tick_hz,
        }
    }
}

pub(crate) fn validate(monotonic: MonotonicDeclaration) -> Result<(), String> {
    if monotonic.tick_hz() == 0 {
        return Err("monotonic tick frequency must be nonzero".to_owned());
    }
    Ok(())
}

pub(crate) fn validate_init_delay(delay: InitDelayDeclaration) -> Result<(), String> {
    if delay.tick_hz == 0 {
        return Err("initialization delay tick frequency must be nonzero".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_systick_frequency_is_accepted() {
        validate(MonotonicDeclaration::systick(1_000)).unwrap();
    }

    #[test]
    fn zero_systick_frequency_is_rejected() {
        let error = validate(MonotonicDeclaration::systick(0)).unwrap_err();
        assert_eq!(error, "monotonic tick frequency must be nonzero");
    }

    #[test]
    fn valid_timer_microsecond_frequency_is_accepted() {
        validate(MonotonicDeclaration::timer("tim2", 1_000_000)).unwrap();
    }

    #[test]
    fn zero_timer_frequency_is_rejected() {
        let error = validate(MonotonicDeclaration::timer("tim2", 0)).unwrap_err();
        assert_eq!(error, "monotonic tick frequency must be nonzero");
    }

    #[test]
    fn zero_initialization_delay_frequency_is_rejected() {
        let error = validate_init_delay(InitDelayDeclaration::timer("tim5", 0)).unwrap_err();
        assert_eq!(error, "initialization delay tick frequency must be nonzero");
    }
}
