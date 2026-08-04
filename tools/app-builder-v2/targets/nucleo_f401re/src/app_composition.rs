use crate::app::{
    AppDeclaration, InitDeclaration, SoftwareResourceDeclaration, SoftwareResourcesDeclaration,
};
use crate::{
    component::{ComponentConfiguration, ComponentDeclaration},
    components::{COMPORT_COMPONENT, SERIAL_PORT_COMPONENT},
    task::{TaskDeclaration, parameter, resource},
    tasks,
};
use ferrowasp_io_core::serial::SerialProtocol;
use fugit::MillisDurationU32;

pub const UART2: ComponentDeclaration = ComponentDeclaration {
    id: "uart2",
    definition: &SERIAL_PORT_COMPONENT,
    configuration: ComponentConfiguration::SerialPort(SerialProtocol::Raw),
    bindings: &[resource("endpoint").to_hw("uart2")],
};

pub const COMPORT: ComponentDeclaration = ComponentDeclaration {
    id: "comport",
    definition: &COMPORT_COMPONENT,
    configuration: ComponentConfiguration::None,
    bindings: &[resource("rx").to_sw("uart2_rx")],
};

pub const BLINK_LED: TaskDeclaration = tasks::BLINK
    .spawned_as("blink_led")
    .priority(1)
    .with_parameters(&[parameter("toggle_interval").duration(MillisDurationU32::millis(1_000))])
    .with_local(&[resource("led").to_hw("led3")])
    .with_shared(&[resource("enabled").to_sw("blink_enabled")]);

pub const REPORT_BLINK_TASK: TaskDeclaration =
    tasks::REPORT_BLINK.spawned_as("report_blink").priority(1);

pub const BUTTON_EXTI_TASK: TaskDeclaration = tasks::BUTTON_EXTI
    .interrupt_as("button_exti", "button")
    .priority(2)
    .with_local(&[resource("button").to_hw("user_button")])
    .with_shared(&[resource("enabled").to_sw("blink_enabled")]);

pub const APP: AppDeclaration = AppDeclaration {
    init: InitDeclaration {
        spawns: &[BLINK_LED],
    },
    tasks: &[BLINK_LED, REPORT_BLINK_TASK, BUTTON_EXTI_TASK],
    components: &[UART2, COMPORT],
    software_resources: SoftwareResourcesDeclaration {
        shared: &[SoftwareResourceDeclaration::bool("blink_enabled", true)],
        local: &[],
    },
};
