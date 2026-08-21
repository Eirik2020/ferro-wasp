{{LED_INIT_EXPRESSION}}
if blink_led::spawn().is_err() {
    panic!("failed to start the divergent blink task during initialization");
}
