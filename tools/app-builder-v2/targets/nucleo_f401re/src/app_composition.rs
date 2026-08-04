use crate::app::{
    AppDeclaration, InitDeclaration, SoftwareResourceDeclaration, SoftwareResourcesDeclaration,
};
use crate::{
    task::{TaskDeclaration, resource},
    tasks,
};

pub const BLINK_LED: TaskDeclaration = tasks::BLINK
    .spawned_as("blink_led")
    .priority(1)
    .with_local(&[resource("led").to_hw("led3")])
    .with_shared(&[resource("enabled").to_sw("blink_enabled")]);

pub const REPORT_BLINK_TASK: TaskDeclaration =
    tasks::REPORT_BLINK.spawned_as("report_blink").priority(1);

pub const BUTTON_EXTI_TASK: TaskDeclaration = tasks::BUTTON_EXTI
    .interrupt_as("button_exti", "button")
    .priority(2)
    .with_local(&[resource("button").to_hw("user_button")])
    .with_shared(&[resource("enabled").to_sw("blink_enabled")]);

pub const SBUS_DMA_IRQ_TASK: TaskDeclaration = tasks::UART_RX_DMA_IRQ
    .dma_rx_interrupt_as("sbus_dma_irq", "rx")
    .priority(3)
    .with_shared(&[resource("rx").to_hw("sbus_rx")]);

pub const SBUS_IDLE_IRQ_TASK: TaskDeclaration = tasks::UART_RX_IDLE_IRQ
    .peripheral_interrupt_as("sbus_idle_irq", "rx")
    .priority(3)
    .with_shared(&[resource("rx").to_hw("sbus_rx")]);

pub const SBUS_PARSE_TASK: TaskDeclaration = tasks::SBUS_PARSE
    .spawned_as("sbus_parse")
    .priority(2)
    .with_shared(&[resource("rx").to_hw("sbus_rx")]);

pub const APP: AppDeclaration = AppDeclaration {
    init: InitDeclaration {
        spawns: &[BLINK_LED, SBUS_PARSE_TASK],
    },
    tasks: &[
        BLINK_LED,
        REPORT_BLINK_TASK,
        BUTTON_EXTI_TASK,
        SBUS_DMA_IRQ_TASK,
        SBUS_IDLE_IRQ_TASK,
        SBUS_PARSE_TASK,
    ],
    software_resources: SoftwareResourcesDeclaration {
        shared: &[SoftwareResourceDeclaration::bool("blink_enabled", true)],
        local: &[],
    },
};
