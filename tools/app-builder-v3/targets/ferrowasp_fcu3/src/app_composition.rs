use crate::app::{
    AppDeclaration, InitDeclaration, SoftwareResourceDeclaration, SoftwareResourcesDeclaration,
};
use crate::{
    component::{ComponentConfiguration, ComponentDeclaration},
    components::{COMMAND_INPUT_COMPONENT, COMPORT_COMPONENT, SERIAL_PORT_COMPONENT},
    task::{TaskDeclaration, parameter, resource},
    tasks,
};
use ferrowasp_io_core::serial::SerialProtocol;
use fugit::MillisDurationU32;

pub const UART2: ComponentDeclaration = ComponentDeclaration {
    id: "uart2",
    definition: &SERIAL_PORT_COMPONENT,
    configuration: ComponentConfiguration::SerialPort(SerialProtocol::Sbus),
    bindings: &[resource("endpoint").to_hw("uart2")],
};

pub const COMMAND_INPUT: ComponentDeclaration = ComponentDeclaration {
    id: "command_input",
    definition: &COMMAND_INPUT_COMPONENT,
    configuration: ComponentConfiguration::None,
    bindings: &[resource("rx").to_component("uart2", "rx")],
};

pub const UART4: ComponentDeclaration = ComponentDeclaration {
    id: "uart4",
    definition: &SERIAL_PORT_COMPONENT,
    configuration: ComponentConfiguration::SerialPort(SerialProtocol::Raw),
    bindings: &[resource("endpoint").to_hw("uart4")],
};

pub const COMPORT: ComponentDeclaration = ComponentDeclaration {
    id: "comport",
    definition: &COMPORT_COMPONENT,
    configuration: ComponentConfiguration::None,
    bindings: &[resource("rx").to_component("uart4", "rx")],
};

pub const BLINK_LED: TaskDeclaration = tasks::BLINK
    .spawned_as("blink_led")
    .priority(1)
    .with_parameters(&[parameter("toggle_interval").duration(MillisDurationU32::millis(1_000))])
    .with_local(&[resource("led").to_hw("green_led")])
    .with_shared(&[resource("enabled").to_sw("blink_enabled")]);

pub const REPORT_BLINK_TASK: TaskDeclaration =
    tasks::REPORT_BLINK.spawned_as("report_blink").priority(1);

pub const RC_HEARTBEAT_TASK: TaskDeclaration = tasks::RC_HEARTBEAT
    .spawned_as("rc_heartbeat")
    .priority(1)
    .with_parameters(&[parameter("report_interval").duration(MillisDurationU32::millis(1_000))])
    .with_shared(&[resource("rc_observer").to_component("command_input", "rc_input")]);

pub const APP: AppDeclaration = AppDeclaration {
    init: InitDeclaration {
        spawns: &[BLINK_LED, RC_HEARTBEAT_TASK],
    },
    tasks: &[BLINK_LED, REPORT_BLINK_TASK, RC_HEARTBEAT_TASK],
    components: &[UART2, COMMAND_INPUT, UART4, COMPORT],
    software_resources: SoftwareResourcesDeclaration {
        shared: &[SoftwareResourceDeclaration::bool("blink_enabled", true)],
        local: &[],
    },
};
