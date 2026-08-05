use crate::task::{TaskDefinition, boolean, interrupt_input};

crate::app_task! {
    pub const BUTTON_EXTI: TaskDefinition = TaskDefinition::synchronous("button_exti")
        .with_local(&[interrupt_input("button")])
        .with_shared(&[boolean("enabled")]);

    fn button_exti(mut cx: button_exti::Context) {
        cx.local.button.clear_interrupt_pending_bit();
        let enabled = cx.shared.enabled.lock(|enabled| {
            *enabled = !*enabled;
            *enabled
        });
        defmt::info!("Blink enabled: {}", enabled);
    }
}
