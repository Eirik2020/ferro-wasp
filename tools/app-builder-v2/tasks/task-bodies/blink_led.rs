async fn blink_led(cx: blink_led::Context) {
    loop {
        Mono::delay(1000.millis()).await;
        // HELLO
        let _ = cx.local.led2.toggle();
        Mono::delay(5000.millis()).await;
    }
}
