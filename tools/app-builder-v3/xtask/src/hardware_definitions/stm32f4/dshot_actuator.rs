//! Application declaration for the reviewed Foxeer DShot/telemetry component.

use super::{
    board_declaration::BoardDeclaration, dshot::DshotBankHardwareDeclaration,
    serial::SerialPeripheral,
};

/// Physical resources exposed by the single reviewed actuator component.
pub const DSHOT_BANK_RESOURCE: &str = "dshot_motors";
/// Shared USART1 RX interrupt-side owner.
pub const ESC_UART_IRQ_RESOURCE: &str = "uart1_rx";
/// Task-local USART1 parser side.
pub const ESC_UART_PARSER_RESOURCE: &str = "esc_telemetry_uart";
/// Shared sticky telemetry discontinuity flag.
pub const ESC_DISCONTINUITY_RESOURCE: &str = "esc_telemetry_discontinuity";
/// Task-local ESC request producer.
pub const ESC_REQUEST_PRODUCER_RESOURCE: &str = "esc_request_producer";
/// Task-local ESC request consumer.
pub const ESC_REQUEST_CONSUMER_RESOURCE: &str = "esc_request_consumer";
/// Task-local DShot acknowledgement producer.
pub const ESC_ACK_PRODUCER_RESOURCE: &str = "esc_ack_producer";
/// Task-local DShot acknowledgement consumer.
pub const ESC_ACK_CONSUMER_RESOURCE: &str = "esc_ack_consumer";
/// Task-local telemetry update producer.
pub const ESC_UPDATE_PRODUCER_RESOURCE: &str = "esc_telemetry_update_producer";
/// Task-local telemetry update consumer.
pub const ESC_UPDATE_CONSUMER_RESOURCE: &str = "esc_telemetry_update_consumer";
/// Task-local legacy telemetry manager state.
pub const ESC_MANAGER_STATE_RESOURCE: &str = "esc_manager_state";
/// Task-local bounded diagnostic counter.
pub const ESC_MANAGER_REPORT_TICKS_RESOURCE: &str = "esc_manager_report_ticks";
/// Task-local pending DShot telemetry request.
pub const DSHOT_PENDING_REQUEST_RESOURCE: &str = "esc_actuator_request";
/// Task-local request-submission state.
pub const DSHOT_REQUEST_SUBMITTED_RESOURCE: &str = "esc_actuator_request_submitted";
/// Task-local latched-fault report state.
pub const DSHOT_FAULT_REPORTED_RESOURCE: &str = "dshot_fault_reported";

/// Selected priorities and default gate for a four-output physical actuator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DshotActuatorDeclaration {
    /// Stable application component identifier.
    pub id: &'static str,
    /// Board-local four-lane DShot hardware identifier.
    pub hardware_id: &'static str,
    /// Board-local receive-only ESC telemetry endpoint identifier.
    pub telemetry_hardware_id: &'static str,
    /// Physical output gate. The golden generated candidate requires `false`.
    pub output_enabled: bool,
    /// Four DMA completion interrupt priority.
    pub dma_priority: u8,
    /// Sole safety-owned actuator adapter priority.
    pub actuator_priority: u8,
    /// Bounded DShot service priority.
    pub service_priority: u8,
    /// USART1 and RX DMA interrupt priority.
    pub telemetry_interrupt_priority: u8,
    /// ESC manager priority.
    pub telemetry_manager_priority: u8,
}

impl DshotActuatorDeclaration {
    /// Creates the reviewed task topology with physical output disabled.
    pub const fn foxeer_disabled(
        id: &'static str,
        hardware_id: &'static str,
        telemetry_hardware_id: &'static str,
    ) -> Self {
        Self {
            id,
            hardware_id,
            telemetry_hardware_id,
            output_enabled: false,
            dma_priority: 16,
            actuator_priority: 15,
            service_priority: 13,
            telemetry_interrupt_priority: 5,
            telemetry_manager_priority: 4,
        }
    }

    /// Explicitly sets the physical output gate.
    pub const fn output_enabled(mut self, enabled: bool) -> Self {
        self.output_enabled = enabled;
        self
    }
}

pub(crate) fn validate(
    declaration: DshotActuatorDeclaration,
    board: &BoardDeclaration,
) -> Result<(), String> {
    let hardware = board.dshot_bank(declaration.hardware_id).ok_or_else(|| {
        format!(
            "DShot actuator `{}` consumes undeclared bank `{}`",
            declaration.id, declaration.hardware_id
        )
    })?;
    validate_bank(hardware)?;
    let telemetry = board
        .serial(declaration.telemetry_hardware_id)
        .ok_or_else(|| {
            format!(
                "DShot actuator `{}` consumes undeclared telemetry endpoint `{}`",
                declaration.id, declaration.telemetry_hardware_id
            )
        })?;
    if telemetry.port.peripheral != SerialPeripheral::Usart1
        || telemetry.port.rx.is_none()
        || telemetry.port.tx.is_some()
    {
        return Err(format!(
            "DShot actuator `{}` requires a receive-only USART1 telemetry endpoint",
            declaration.id
        ));
    }
    if declaration.dma_priority != 16
        || declaration.actuator_priority != 15
        || declaration.service_priority != 13
        || declaration.telemetry_interrupt_priority != 5
        || declaration.telemetry_manager_priority != 4
    {
        return Err(format!(
            "DShot actuator `{}` priorities drift from the reviewed 16/15/13/5/4 topology",
            declaration.id
        ));
    }
    Ok(())
}

fn validate_bank(hardware: &DshotBankHardwareDeclaration) -> Result<(), String> {
    if !hardware.tim1_frame_master_with_tim8_itr0 {
        return Err(format!(
            "DShot bank `{}` must start TIM8 from TIM1 through ITR0",
            hardware.id
        ));
    }
    for (index, lane) in hardware.lanes.iter().enumerate() {
        let expected = index as u8 + 1;
        if lane.physical_output != expected || lane.logical_motor != expected {
            return Err(format!(
                "DShot bank `{}` must preserve identity physical/logical order at lane {expected}",
                hardware.id
            ));
        }
    }
    Ok(())
}

/// Concrete type supplied by one component-owned local resource.
pub(crate) fn local_rust_type(target: &str) -> Option<&'static str> {
    match target {
        ESC_UART_PARSER_RESOURCE => Some("UartRxParserSide"),
        ESC_REQUEST_PRODUCER_RESOURCE => Some("EscRequestProducer"),
        ESC_REQUEST_CONSUMER_RESOURCE => Some("EscRequestConsumer"),
        ESC_ACK_PRODUCER_RESOURCE => Some("EscAckProducer"),
        ESC_ACK_CONSUMER_RESOURCE => Some("EscAckConsumer"),
        ESC_UPDATE_PRODUCER_RESOURCE => Some("EscTelemetryUpdateProducer"),
        ESC_UPDATE_CONSUMER_RESOURCE => Some("EscTelemetryUpdateConsumer"),
        ESC_MANAGER_STATE_RESOURCE => Some("EscManager"),
        ESC_MANAGER_REPORT_TICKS_RESOURCE => Some("u16"),
        DSHOT_PENDING_REQUEST_RESOURCE => Some("Option<EscActuatorRequest>"),
        DSHOT_REQUEST_SUBMITTED_RESOURCE | DSHOT_FAULT_REPORTED_RESOURCE => Some("bool"),
        _ => None,
    }
}

/// Concrete type supplied by one component-owned shared resource.
pub(crate) fn shared_rust_type(target: &str) -> Option<&'static str> {
    match target {
        DSHOT_BANK_RESOURCE => Some("DshotMotorBank"),
        ESC_UART_IRQ_RESOURCE => Some("Uart1RxIrq"),
        ESC_DISCONTINUITY_RESOURCE => Some("bool"),
        _ => None,
    }
}
