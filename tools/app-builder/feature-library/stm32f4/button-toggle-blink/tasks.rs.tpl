#[task(
    binds = {{BUTTON_INTERRUPT}},
    priority = {{TASK_PRIORITY}},
    shared = [{{BUTTON_RESOURCE_NAME}}, {{EXTI_RESOURCE_NAME}}, {{BUTTON_FAULT_RESOURCE}}]
)]
fn button_toggle(cx: button_toggle::Context) {
    (
        cx.shared.{{BUTTON_RESOURCE_NAME}},
        cx.shared.{{EXTI_RESOURCE_NAME}},
        cx.shared.{{BUTTON_FAULT_RESOURCE}},
    )
        .lock(|button, exti, faults| {
            button.clear_interrupt_pending_bit();
            button.disable_interrupt(exti);
            if button_toggle_debounce::spawn().is_err() {
                *faults = faults.saturating_add(1);
            }
        });
}

#[task(
    priority = {{TASK_PRIORITY}},
    shared = [
        {{BUTTON_RESOURCE_NAME}},
        {{EXTI_RESOURCE_NAME}},
        {{LED_RESOURCE_NAME}},
        {{BLINKER_ENABLED_NAME}}
    ]
)]
async fn button_toggle_debounce(cx: button_toggle_debounce::Context) {
    Mono::delay({{DEBOUNCE_MS}}.millis()).await;
    let pressed = (cx.shared.{{BUTTON_RESOURCE_NAME}}, cx.shared.{{EXTI_RESOURCE_NAME}})
        .lock(|button, exti| {
            let pressed = button.is_low();
            button.clear_interrupt_pending_bit();
            button.enable_interrupt(exti);
            pressed
        });

    if pressed {
        (cx.shared.{{BLINKER_ENABLED_NAME}}, cx.shared.{{LED_RESOURCE_NAME}})
            .lock(|enabled, led| {
                *enabled = !*enabled;
                if !*enabled {
                    let _ = led.set_low();
                }
            });
    }
}
