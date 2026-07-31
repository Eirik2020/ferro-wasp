fn button_exti(mut cx: button_exti::Context) {
    cx.local.user_button.clear_interrupt_pending_bit();
    let enabled = cx.shared.blink_enabled.lock(|enabled| {
        *enabled = !*enabled;
        *enabled
    });
    defmt::info!("Blink enabled: {}", enabled);
}
