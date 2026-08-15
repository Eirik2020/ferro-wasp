use crate::task::{TaskDefinition, boolean, digital_output, duration};

crate::app_task! {
    pub const BLINK: TaskDefinition = TaskDefinition::asynchronous("blink")
        .with_parameters(&[duration("toggle_interval")])
        .with_local(&[digital_output("led")])
        .with_shared(&[boolean("enabled")]);

    async fn blink(mut cx: blink::Context) {
        let mut blink_count = 0_u32;
        loop {
            Mono::delay(cx.config.toggle_interval).await;
            let enabled = cx.shared.enabled.lock(|enabled| *enabled);
            if enabled {
                let _ = StatefulOutputPin::toggle(cx.local.led);
                blink_count = blink_count.wrapping_add(1);
                report_blink::spawn(blink_count)
                    .expect("blink report task queue must have capacity");
            } else {
                let _ = OutputPin::set_low(cx.local.led);
            }
        }
    }
}
