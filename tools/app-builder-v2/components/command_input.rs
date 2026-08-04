//! SBUS command-input functional component.

use crate::{
    component::{
        ComponentActivation, ComponentConfigurationKind, ComponentDefinition,
        ComponentInitLocalResource, ComponentLayer, ComponentResource, ComponentResourceKind,
        ComponentResourceVisibility, ComponentSoftwareResource,
        ComponentSoftwareResourceInitializer, ComponentSoftwareResourceOwnership, ComponentTask,
        ComponentTaskBinding, ComponentTaskTrigger,
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
            ComponentTaskBinding::new("rc_observer", "rc_input_publisher"),
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
        ComponentResource {
            id: "rc_input_channel",
            kind: ComponentResourceKind::InitLocal(ComponentInitLocalResource::ObserverChannel {
                type_id: SOFTWARE_RC_INPUT_SNAPSHOT,
                rust_type: "RcInputSnapshot",
                publisher: "rc_input_publisher",
                reader: "rc_input_reader",
            }),
            activation: ComponentActivation::Always,
        },
        ComponentResource {
            id: "rc_input_publisher",
            kind: ComponentResourceKind::InternalSoftware {
                resource: ComponentSoftwareResource {
                    type_id: SOFTWARE_RC_INPUT_SNAPSHOT,
                    rust_type: "ObserverPublisher<'static, RcInputSnapshot>",
                    ownership: ComponentSoftwareResourceOwnership::Local,
                    initializer: ComponentSoftwareResourceInitializer::ObserverPublisher {
                        channel: "rc_input_channel",
                    },
                },
                visibility: ComponentResourceVisibility::Private,
            },
            activation: ComponentActivation::Always,
        },
        ComponentResource {
            id: "rc_input_reader",
            kind: ComponentResourceKind::InternalSoftware {
                resource: ComponentSoftwareResource {
                    type_id: SOFTWARE_RC_INPUT_SNAPSHOT,
                    rust_type: "ObserverReader<'static, RcInputSnapshot>",
                    ownership: ComponentSoftwareResourceOwnership::Shared,
                    initializer: ComponentSoftwareResourceInitializer::ObserverReader {
                        channel: "rc_input_channel",
                    },
                },
                visibility: ComponentResourceVisibility::Exposed,
            },
            activation: ComponentActivation::Always,
        },
    ],
};
