//! Raw line-oriented COMPORT functional component.

use crate::{
    component::{
        ComponentActivation, ComponentConfigurationKind, ComponentDefinition, ComponentLayer,
        ComponentResource, ComponentResourceKind, ComponentResourceVisibility,
        ComponentSoftwareResource, ComponentSoftwareResourceInitializer,
        ComponentSoftwareResourceOwnership, ComponentTask, ComponentTaskBinding,
        ComponentTaskTrigger,
    },
    task::{SOFTWARE_LINE_CONSUMER, SOFTWARE_SERIAL_RX, TaskResourceCapability, TaskSafetyClass},
    tasks,
};

/// Raw line-oriented COMPORT consumer and logging behavior.
pub const COMPORT_COMPONENT: ComponentDefinition = ComponentDefinition {
    id: "comport",
    layer: ComponentLayer::Functional,
    configuration_kind: ComponentConfigurationKind::None,
    tasks: &[ComponentTask {
        definition: tasks::COMPORT,
        safety_class: TaskSafetyClass::NonSafetyCritical,
        id: "consumer",
        priority: 2,
        trigger: ComponentTaskTrigger::Spawned,
        parameters: &[],
        local_resources: &[ComponentTaskBinding::new("decoder", "decoder")],
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
                    type_id: SOFTWARE_LINE_CONSUMER,
                    rust_type: "LineConsumer",
                    ownership: ComponentSoftwareResourceOwnership::Local,
                    initializer: ComponentSoftwareResourceInitializer::Expression(
                        "LineConsumer::new()",
                    ),
                },
                visibility: ComponentResourceVisibility::Private,
            },
            activation: ComponentActivation::Always,
        },
    ],
};
