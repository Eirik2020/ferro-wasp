async fn blink_led(mut cx: blink_led::Context) {
    let mut blink_count = 0_u32;
    loop {
        Mono::delay(1000.millis()).await;
        let enabled = cx.shared.blink_enabled.lock(|enabled| *enabled);
        if enabled {
            let _ = StatefulOutputPin::toggle(cx.local.led3);
            blink_count = blink_count.wrapping_add(1);
            report_blink::spawn(blink_count).expect("blink report task queue must have capacity");
        } else {
            let _ = OutputPin::set_low(cx.local.led3);
        }
        Mono::delay(5000.millis()).await;
    }
}
