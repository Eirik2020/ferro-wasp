use crate::app::{
    AppDeclaration, InitDeclaration, SoftwareResourceDeclaration, SoftwareResourcesDeclaration,
};
use crate::task_declarations::*;

pub const APP: AppDeclaration = AppDeclaration {
    init: InitDeclaration {
        spawns: &[BLINK_LED],
    },
    tasks: &[BLINK_LED, REPORT_BLINK, BUTTON_EXTI],
    software_resources: SoftwareResourcesDeclaration {
        shared: &[SoftwareResourceDeclaration::bool("blink_enabled", true)],
        local: &[],
    },
};
