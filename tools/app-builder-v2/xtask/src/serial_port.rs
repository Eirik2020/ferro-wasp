//! Reusable receive-only UART DMA component definition.

use crate::{
    component::{
        ComponentActivation, ComponentConfigurationKind, ComponentDefinition,
        ComponentInitLocalResource, ComponentResource, ComponentResourceKind,
        ComponentResourceVisibility, ComponentSoftwareResource, ComponentTask,
        ComponentTaskBinding, ComponentTaskTrigger,
    },
    task::{HardwareInterrupt, TaskResourceCapability},
    tasks,
};

/// UART DMA, IDLE detection, and boot-selected bounded consumer component.
pub const SERIAL_PORT_COMPONENT: ComponentDefinition = ComponentDefinition {
    id: "serial_port",
    configuration_kind: ComponentConfigurationKind::SerialPort,
    tasks: &[
        ComponentTask {
            definition: tasks::UART_RX_DMA_IRQ,
            id: "dma_irq",
            priority: 3,
            trigger: ComponentTaskTrigger::Interrupt {
                resource: "endpoint",
                interrupt: HardwareInterrupt::DmaRx,
            },
            parameters: &[],
            local_resources: &[],
            shared_resources: &[ComponentTaskBinding::new("rx", "endpoint")],
            init_spawn: false,
            activation: ComponentActivation::SerialEnabled,
        },
        ComponentTask {
            definition: tasks::UART_RX_IDLE_IRQ,
            id: "idle_irq",
            priority: 3,
            trigger: ComponentTaskTrigger::Interrupt {
                resource: "endpoint",
                interrupt: HardwareInterrupt::Peripheral,
            },
            parameters: &[],
            local_resources: &[],
            shared_resources: &[ComponentTaskBinding::new("rx", "endpoint")],
            init_spawn: false,
            activation: ComponentActivation::SerialEnabled,
        },
        ComponentTask {
            definition: tasks::SERIAL_CONSUMER,
            id: "consumer",
            priority: 2,
            trigger: ComponentTaskTrigger::Spawned,
            parameters: &[],
            local_resources: &[ComponentTaskBinding::new("consumer", "consumer_state")],
            shared_resources: &[
                ComponentTaskBinding::new("endpoint", "endpoint"),
                ComponentTaskBinding::new("rc_input", "rc_input"),
            ],
            init_spawn: true,
            activation: ComponentActivation::SerialEnabled,
        },
    ],
    resources: &[
        ComponentResource {
            id: "endpoint",
            kind: ComponentResourceKind::ExternalHardware {
                capability: TaskResourceCapability::UartRxDma,
            },
            activation: ComponentActivation::Always,
        },
        ComponentResource {
            id: "consumer_state",
            kind: ComponentResourceKind::InternalSoftware {
                resource: ComponentSoftwareResource::SerialConsumer,
                visibility: ComponentResourceVisibility::Private,
            },
            activation: ComponentActivation::SerialEnabled,
        },
        ComponentResource {
            id: "rc_input",
            kind: ComponentResourceKind::InternalSoftware {
                resource: ComponentSoftwareResource::RcInputSnapshot,
                visibility: ComponentResourceVisibility::Exposed,
            },
            activation: ComponentActivation::Always,
        },
        ComponentResource {
            id: "rx_buffers",
            kind: ComponentResourceKind::InitLocal(ComponentInitLocalResource::UartRxBuffers),
            activation: ComponentActivation::SerialEnabled,
        },
        ComponentResource {
            id: "free_queue",
            kind: ComponentResourceKind::InitLocal(ComponentInitLocalResource::UartRxFreeQueue),
            activation: ComponentActivation::SerialEnabled,
        },
        ComponentResource {
            id: "filled_queue",
            kind: ComponentResourceKind::InitLocal(ComponentInitLocalResource::UartRxFilledQueue),
            activation: ComponentActivation::SerialEnabled,
        },
    ],
};
