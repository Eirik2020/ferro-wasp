//! Application timing declarations used by generated RTIC apps.

/// Monotonic timer selected by an application composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonotonicDeclaration {
    /// Cortex-M SysTick monotonic exposed to task bodies as `Mono`.
    SysTick {
        /// Number of monotonic ticks per second.
        tick_hz: u32,
    },
}

impl MonotonicDeclaration {
    /// Selects a SysTick monotonic with the supplied tick frequency.
    pub const fn systick(tick_hz: u32) -> Self {
        Self::SysTick { tick_hz }
    }

    /// Returns the configured monotonic tick frequency.
    pub const fn tick_hz(self) -> u32 {
        match self {
            Self::SysTick { tick_hz } => tick_hz,
        }
    }
}

pub(crate) fn validate(monotonic: MonotonicDeclaration) -> Result<(), String> {
    if monotonic.tick_hz() == 0 {
        return Err("monotonic tick frequency must be nonzero".to_owned());
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
}
