use crate::task::{TaskArgument, TaskDeclaration, TaskTrigger};

pub const REPORT_BLINK: TaskDeclaration = TaskDeclaration {
    id: "report_blink",
    priority: 1,
    trigger: TaskTrigger::Spawned,
    args: &[TaskArgument {
        name: "count",
        rust_type: "u32",
    }],
    local_resources: &[],
    shared_resources: &[],
};
