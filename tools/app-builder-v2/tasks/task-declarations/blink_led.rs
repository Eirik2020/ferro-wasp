use crate::task::{TaskDeclaration, TaskTrigger};

pub const BLINK_LED: TaskDeclaration = TaskDeclaration {
    id: "blink_led",
    priority: 1,
    trigger: TaskTrigger::Spawned,
    args: &[],
    local_resources: &["led3"],
    shared_resources: &["blink_enabled"],
};
