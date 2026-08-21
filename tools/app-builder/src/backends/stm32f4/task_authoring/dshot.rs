//! Host-only shapes used to type-check reusable physical-actuator task bodies.
//!
//! Generated firmware imports the concrete types with the same names from the
//! reviewed STM32F4 DShot and UART backends.

#![allow(missing_docs)]

use ferrowasp_stm32f4::serial::UartRxIrqOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotMotor {
    Motor1,
    Motor2,
    Motor3,
    Motor4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotInterruptEvent {
    Completed,
    Faulted,
    Spurious,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotServiceEvent {
    FrameStarted,
    Busy,
    LeaseExpired,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotTelemetryRequestError {
    Busy,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DshotCommandError {
    ThrottleOutOfRange,
    Faulted,
}

pub struct DshotMotorBank;

impl DshotMotorBank {
    pub fn command_stop(&mut self) {}

    pub fn command_throttles(
        &mut self,
        _commands: [u16; 4],
        _now_ms: u32,
        _lease_duration_ms: u32,
    ) -> Result<(), DshotCommandError> {
        Ok(())
    }

    pub fn request_telemetry(
        &mut self,
        _motor: DshotMotor,
    ) -> Result<(), DshotTelemetryRequestError> {
        Ok(())
    }

    pub fn service(&mut self, _now_ms: u32) -> DshotServiceEvent {
        DshotServiceEvent::FrameStarted
    }

    pub fn take_telemetry_request_sent(&mut self) -> Option<DshotMotor> {
        None
    }

    pub fn on_dma_interrupt(&mut self, _motor: DshotMotor) -> DshotInterruptEvent {
        DshotInterruptEvent::Completed
    }
}

pub struct Uart1RxIrq;

impl Uart1RxIrq {
    pub fn service_dma_irq(&mut self) -> UartRxIrqOutcome {
        UartRxIrqOutcome::Ignored
    }

    pub fn service_idle_irq(&mut self) -> UartRxIrqOutcome {
        UartRxIrqOutcome::Ignored
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartRxReadStatus {
    NoChunk,
    Chunk { len: usize, uart_error_seen: bool },
    RecycleError,
}

pub struct UartRxParserSide;

impl UartRxParserSide {
    pub fn read_chunk_with_status(
        &mut self,
        _output: &mut [u8; ferrowasp_stm32f4::memory::UART_RX_BUFFER_BYTES],
    ) -> UartRxReadStatus {
        UartRxReadStatus::NoChunk
    }
}
