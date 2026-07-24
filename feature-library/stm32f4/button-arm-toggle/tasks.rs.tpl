#[task(binds = {{BUTTON_INTERRUPT}}, priority = {{TASK_PRIORITY}}, shared = [{{BUTTON_RESOURCE_NAME}}, {{EXTI_RESOURCE_NAME}}, {{OSD_FAULT_RESOURCE}}])]
fn button_arm_toggle(mut cx: button_arm_toggle::Context) {
    let spawn_failed =
        (cx.shared.{{BUTTON_RESOURCE_NAME}}, cx.shared.{{EXTI_RESOURCE_NAME}}).lock(|button, exti| {
            button.clear_interrupt_pending_bit();
            button.disable_interrupt(exti);
            button_arm_toggle_debounce::spawn().is_err()
        });
    if spawn_failed {
        cx.shared
            .{{OSD_FAULT_RESOURCE}}
            .lock(|faults| faults.record(OsdFaultId::DebounceSpawnFailed));
    }
}

#[task(priority = {{TASK_PRIORITY}}, shared = [{{BUTTON_RESOURCE_NAME}}, {{EXTI_RESOURCE_NAME}}, {{OSD_TELEMETRY_RESOURCE}}])]
async fn button_arm_toggle_debounce(mut cx: button_arm_toggle_debounce::Context) {
    Mono::delay({{DEBOUNCE_MS}}.millis()).await;
    let pressed = (cx.shared.{{BUTTON_RESOURCE_NAME}}, cx.shared.{{EXTI_RESOURCE_NAME}}).lock(|button, exti| {
        let pressed = button.is_low();
        button.clear_interrupt_pending_bit();
        button.enable_interrupt(exti);
        pressed
    });
    if pressed {
        cx.shared.{{OSD_TELEMETRY_RESOURCE}}.lock(|telemetry| telemetry.toggle_armed());
    }
}
