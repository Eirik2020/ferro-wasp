use crate::app::{AppDeclaration, InitDeclaration};

#[path = "../tasks/task-declarations/blink_led.rs"]
mod blink_led;

pub const APP: AppDeclaration = AppDeclaration {
    init: InitDeclaration::EMPTY,
    tasks: &[blink_led::BLINK_LED],
};
