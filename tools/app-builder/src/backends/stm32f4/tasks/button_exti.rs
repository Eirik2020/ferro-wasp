use stm32f4xx_hal::gpio::ExtiPin;

crate::reusable_task! {
    contract {
        local {
            /// Interrupt-capable button owned by this task instance.
            button: dyn ExtiPin,
        }
        shared {
            /// Application state toggled when the button interrupt is pending.
            enabled: bool,
        }
        config {
            /// Selects whether a button event toggles the shared state.
            toggle_on_press: bool,
        }
        spawns {
            /// Announces the resulting enabled state to a composed software task.
            button_changed(enabled: bool),
        }
    }

    /// Handles one STM32F4 EXTI button interrupt.
    pub fn button_exti(mut cx: button_exti::Context<'_>) {
        if !cx.local.button.check_interrupt() {
            return;
        }

        cx.local.button.clear_interrupt_pending_bit();

        let toggle_on_press = cx.config.toggle_on_press;
        let enabled = cx.shared.enabled.lock(|enabled| {
            if toggle_on_press {
                *enabled = !*enabled;
            }
            *enabled
        });

        let _ = button_changed::spawn(enabled);
    }
}

#[cfg(test)]
mod tests {
    use stm32f4xx_hal::{
        gpio::{Edge, ExtiPin},
        pac::EXTI,
        syscfg::SysCfg,
    };

    use super::*;

    struct FakeButton {
        pending: bool,
    }

    impl ExtiPin for FakeButton {
        fn make_interrupt_source(&mut self, _syscfg: &mut SysCfg) {}

        fn trigger_on_edge(&mut self, _exti: &mut EXTI, _level: Edge) {}

        fn enable_interrupt(&mut self, _exti: &mut EXTI) {}

        fn disable_interrupt(&mut self, _exti: &mut EXTI) {}

        fn clear_interrupt_pending_bit(&mut self) {
            self.pending = false;
        }

        fn check_interrupt(&self) -> bool {
            self.pending
        }
    }

    #[test]
    fn pending_interrupt_is_cleared_and_shared_state_is_toggled() {
        let mut button = FakeButton { pending: true };
        let mut enabled = false;
        let context = button_exti::Context::new(
            button_exti::Local::new(&mut button),
            button_exti::Shared::new(&mut enabled),
            button_exti::Config::new(true),
        );

        button_exti(context);

        assert!(!button.pending);
        assert!(enabled);
    }

    #[test]
    fn spurious_entry_leaves_resources_unchanged() {
        let mut button = FakeButton { pending: false };
        let mut enabled = false;
        let context = button_exti::Context::new(
            button_exti::Local::new(&mut button),
            button_exti::Shared::new(&mut enabled),
            button_exti::Config::new(true),
        );

        button_exti(context);

        assert!(!button.pending);
        assert!(!enabled);
    }
}
