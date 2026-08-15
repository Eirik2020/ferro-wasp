//! SBUS command-input functional component.

use crate::{
    component::{
        ComponentActivation, ComponentConfigurationKind, ComponentDefinition, ComponentLayer,
        ComponentResource, ComponentResourceKind, ComponentResourceVisibility,
        ComponentSoftwareResource, ComponentSoftwareResourceInitializer,
        ComponentSoftwareResourceOwnership, ComponentTask, ComponentTaskBinding,
        ComponentTaskTrigger, observer_output,
    },
    task::{
        SOFTWARE_RC_INPUT_SNAPSHOT, SOFTWARE_SBUS_CONSUMER, SOFTWARE_SERIAL_RX,
        TaskResourceCapability, TaskSafetyClass,
    },
    tasks,
};

/// SBUS command-input parser and its exposed latest RC sample.
pub const COMMAND_INPUT_COMPONENT: ComponentDefinition = ComponentDefinition {
    id: "command_input",
    layer: ComponentLayer::Functional,
    configuration_kind: ComponentConfigurationKind::None,
    tasks: &[ComponentTask {
        definition: tasks::COMMAND_INPUT,
        safety_class: TaskSafetyClass::NonSafetyCritical,
        id: "consumer",
        priority: 2,
        trigger: ComponentTaskTrigger::Spawned,
        parameters: &[],
        local_resources: &[
            ComponentTaskBinding::new("decoder", "decoder"),
            ComponentTaskBinding::observer_publisher("rc_observer", "rc_input"),
        ],
        shared_resources: &[ComponentTaskBinding::new("rx", "rx")],
        init_spawn: true,
        activation: ComponentActivation::Always,
    }],
    resources: &[
        ComponentResource {
            id: "rx",
            kind: ComponentResourceKind::ExternalSoftware {
                capability: TaskResourceCapability::Software(SOFTWARE_SERIAL_RX),
            },
            activation: ComponentActivation::Always,
        },
        ComponentResource {
            id: "decoder",
            kind: ComponentResourceKind::InternalSoftware {
                resource: ComponentSoftwareResource {
                    type_id: SOFTWARE_SBUS_CONSUMER,
                    rust_type: "SbusConsumer",
                    ownership: ComponentSoftwareResourceOwnership::Local,
                    initializer: ComponentSoftwareResourceInitializer::Expression(
                        "SbusConsumer::new()",
                    ),
                },
                visibility: ComponentResourceVisibility::Private,
            },
            activation: ComponentActivation::Always,
        },
        observer_output("rc_input", SOFTWARE_RC_INPUT_SNAPSHOT, "RcInputSnapshot"),
    ],
};
