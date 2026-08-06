crate::reusable_task! {
    contract {
        local {}
        shared {}
        config {}
        spawns {}
    }

    /// Reports the enabled state produced by the button interrupt task.
    pub async fn observe_button_change(
        _cx: observe_button_change::Context<'_>,
        enabled: bool,
    ) {
        if enabled {
            defmt::info!("button enabled LED blinking");
        } else {
            defmt::info!("button disabled LED blinking");
        }
    }
}
