use crate::task::TaskDeclaration;

pub const BLINK_LED: TaskDeclaration = TaskDeclaration {
    id: "blink_led",
    priority: 1,
    args: &[],
    local_resources: &["led2"],
    shared_resources: &[],
};
