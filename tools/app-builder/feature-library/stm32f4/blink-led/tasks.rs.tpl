#[task(
    priority = {{TASK_PRIORITY}},
    shared = [
        {{LED_RESOURCE_NAME}},
        {{BLINKER_ENABLED_NAME}}
    ]
)]
async fn blink_led(mut cx: blink_led::Context) {
    loop {
        Mono::delay({{BLINK_PERIOD_MS}}.millis()).await;
        (&mut cx.shared.{{BLINKER_ENABLED_NAME}}, &mut cx.shared.{{LED_RESOURCE_NAME}})
            .lock(|enabled, led| {
                if *enabled {
                    let _ = led.toggle();
                }
            });
    }
}
