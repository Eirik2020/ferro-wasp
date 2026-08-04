use crate::app::{
    AppDeclaration, InitDeclaration, SoftwareResourceDeclaration, SoftwareResourcesDeclaration,
};
use crate::{
    component::{ComponentConfiguration, ComponentDeclaration},
    serial_port::SERIAL_PORT_COMPONENT,
    task::{TaskDeclaration, parameter, resource},
    tasks,
};
use ferrowasp_io_core::serial::{RcProtocol, SerialPortAssignment};
use fugit::MillisDurationU32;

pub const UART2: ComponentDeclaration = ComponentDeclaration {
    id: "uart2",
    definition: &SERIAL_PORT_COMPONENT,
    configuration: ComponentConfiguration::SerialPort(SerialPortAssignment::Rc(RcProtocol::Sbus)),
    bindings: &[resource("endpoint").to_hw("uart2_rc_endpoint")],
};

pub const UART4: ComponentDeclaration = ComponentDeclaration {
    id: "uart4",
    definition: &SERIAL_PORT_COMPONENT,
    configuration: ComponentConfiguration::SerialPort(SerialPortAssignment::ComPort),
    bindings: &[resource("endpoint").to_hw("uart4_comport_endpoint")],
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
    .with_shared(&[resource("rc_input").to_sw("uart2_rc_input")]);

pub const APP: AppDeclaration = AppDeclaration {
    init: InitDeclaration {
        spawns: &[BLINK_LED, RC_HEARTBEAT_TASK],
    },
    tasks: &[BLINK_LED, REPORT_BLINK_TASK, RC_HEARTBEAT_TASK],
    components: &[UART2, UART4],
    software_resources: SoftwareResourcesDeclaration {
        shared: &[SoftwareResourceDeclaration::bool("blink_enabled", true)],
        local: &[],
    },
};
