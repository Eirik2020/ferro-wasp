use crate::task::{TaskArgument, TaskDefinition};

crate::app_task! {
    pub const REPORT_BLINK: TaskDefinition = TaskDefinition::asynchronous("report_blink")
        .with_args(&[TaskArgument::new("count", "u32")]);

    async fn report_blink(cx: report_blink::Context, count: u32) {
        let _ = cx;
        defmt::info!("Blink {}", count);
    }
}
