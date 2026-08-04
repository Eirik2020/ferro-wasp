//! Reusable protocol-neutral receive-only serial endpoint component.

use crate::{
    component::{
        ComponentActivation, ComponentConfigurationKind, ComponentDefinition,
        ComponentInitLocalResource, ComponentLayer, ComponentResource, ComponentResourceKind,
        ComponentResourceVisibility, ComponentSoftwareResource,
        ComponentSoftwareResourceInitializer, ComponentSoftwareResourceOwnership, ComponentTask,
        ComponentTaskBinding, ComponentTaskTrigger,
    },
    task::{HardwareInterrupt, SOFTWARE_SERIAL_RX, TaskResourceCapability, TaskSafetyClass},
    tasks,
};

/// UART profile configuration, DMA/IDLE handling, and bounded raw-byte transport.
pub const SERIAL_PORT_COMPONENT: ComponentDefinition = ComponentDefinition {
    id: "serial_port",
    layer: ComponentLayer::HardwareEndpoint,
    configuration_kind: ComponentConfigurationKind::SerialPort,
    tasks: &[
        ComponentTask {
            definition: tasks::UART_RX_DMA_IRQ,
            safety_class: TaskSafetyClass::NonSafetyCritical,
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
            safety_class: TaskSafetyClass::NonSafetyCritical,
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
            id: "rx",
            kind: ComponentResourceKind::InternalSoftware {
                resource: ComponentSoftwareResource {
                    type_id: SOFTWARE_SERIAL_RX,
                    rust_type: "UartRxParserSide",
                    ownership: ComponentSoftwareResourceOwnership::Shared,
                    initializer: ComponentSoftwareResourceInitializer::SerialRxOutput,
                },
                visibility: ComponentResourceVisibility::Exposed,
            },
            activation: ComponentActivation::SerialEnabled,
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
