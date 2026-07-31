async fn report_blink(cx: report_blink::Context, count: u32) {
    let _ = cx;
    defmt::info!("Blink {}", count);
}
