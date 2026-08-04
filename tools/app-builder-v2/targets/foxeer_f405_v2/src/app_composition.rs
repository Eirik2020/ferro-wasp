use crate::app::{AppDeclaration, InitDeclaration, SoftwareResourcesDeclaration};
use crate::{
    component::{ComponentConfiguration, ComponentDeclaration},
    components::{COMMAND_INPUT_COMPONENT, SERIAL_PORT_COMPONENT},
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
    bindings: &[resource("rx").to_sw("uart2_rx")],
};

pub const RC_HEARTBEAT_TASK: TaskDeclaration = tasks::RC_HEARTBEAT
    .spawned_as("rc_heartbeat")
    .priority(1)
    .with_parameters(&[parameter("report_interval").duration(MillisDurationU32::millis(1_000))])
    .with_shared(&[resource("rc_observer").to_sw("command_input_rc_input_reader")]);

pub const APP: AppDeclaration = AppDeclaration {
    init: InitDeclaration {
        spawns: &[RC_HEARTBEAT_TASK],
    },
    tasks: &[RC_HEARTBEAT_TASK],
    components: &[UART2, COMMAND_INPUT],
    software_resources: SoftwareResourcesDeclaration::EMPTY,
};
