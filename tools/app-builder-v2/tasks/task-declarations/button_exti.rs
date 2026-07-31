use crate::task::{TaskDeclaration, TaskTrigger};

pub const BUTTON_EXTI: TaskDeclaration = TaskDeclaration {
    id: "button_exti",
    priority: 2,
    trigger: TaskTrigger::Interrupt { binds: "EXTI15_10" },
    args: &[],
    local_resources: &["user_button"],
    shared_resources: &["blink_enabled"],
};
