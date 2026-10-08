//! Role-free UART ports, started at boot for whatever function config binds.
//!
//! A port's interrupt tasks move received bytes from DMA into the port's
//! owned stream and record transport discontinuities there. They know
//! nothing about the function reading the stream, so any port can serve any
//! function its wiring supports. The function task sees a discontinuity on
//! its stream and decides what it means: RC input turns it into an RC link
//! invalidation, so a broken receiver still fails closed.
//!
//! Pins and DMA streams stay fixed per port at compile time; only the line
//! settings and the function are chosen at boot, from `ResolvedBindings`.
//! Nothing here depends on a chip family: each family's backend starts its
//! ports and hands back the pieces defined here.

use crate::app_storage::UartRxStorageResources;
use crate::memory::{
    OWNED_UART_RX_QUEUE_DEPTH, UART_RX_BUFFER_BYTES, UartOwnedDiscontinuities, UartOwnedReader,
    UartOwnedRxChannel, UartOwnedTxChannel, UartOwnedTxCompletion, UartOwnedTxOwner,
    UartOwnedWriter,
};
use crate::uart_common::{
    UartOwnedRxBridge, UartOwnedRxBridgeOutcome, UartRxDeliveryError, UartRxIrqOutcome,
    UartRxIrqService, UartRxParserSide, UartTxBuf, UartTxDmaService, UartTxIrqOutcome,
    UartTxStartError,
};
use core::fmt::Write as _;
use defmt::{info, warn};
use ferrowasp_io_core::serial::{
    Discontinuity, LogicalSerialPort, MSP_V1_MAX_FRAME_LEN, RcProtocol, ResolvedBindings,
    SerialBindings, SerialFunction, SerialFunctionSlots, SerialRoute, TxChunk, resolve_bindings,
};
use ferrowasp_io_core::time::TimestampMicros;

pub type UartPortBridge =
    UartOwnedRxBridge<'static, UART_RX_BUFFER_BYTES, OWNED_UART_RX_QUEUE_DEPTH>;

/// One port's receive side: the DMA state its interrupts service and the
/// bridge into its owned stream.
pub struct UartRxPort<Irq> {
    pub irq: Irq,
    pub bridge: UartPortBridge,
}

/// What a function task owns: the port's byte stream, its discontinuity
/// record, and a writer when the port was started with a transmit path.
pub struct SerialPortEndpoint {
    pub port: LogicalSerialPort,
    pub reader: UartOwnedReader<'static>,
    pub discontinuities: UartOwnedDiscontinuities<'static>,
    pub writer: Option<UartOwnedWriter<'static>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UartRxEvent {
    DmaComplete,
    IdleLine,
}

/// Service one receive interrupt. Returns the discontinuity it recorded, if
/// any, for the caller to log. Losing a DMA buffer is unrecoverable and
/// panics, as the per-role handlers did.
pub fn service_uart_rx_port<Irq>(
    port: &mut UartRxPort<Irq>,
    event: UartRxEvent,
    now: TimestampMicros,
) -> Option<Discontinuity>
where
    Irq: UartRxIrqService,
{
    let outcome = match event {
        UartRxEvent::DmaComplete => port.irq.service_dma_irq(),
        UartRxEvent::IdleLine => port.irq.service_idle_irq(),
    };
    let generation = port.irq.rx_generation();
    let cause = match outcome {
        UartRxIrqOutcome::Ignored | UartRxIrqOutcome::NoChunk => None,
        UartRxIrqOutcome::Delivered => match port.bridge.publish_next(now) {
            UartOwnedRxBridgeOutcome::Published => None,
            // The stream already recorded the overflow and woke its reader.
            UartOwnedRxBridgeOutcome::QueueOverflow => return None,
            UartOwnedRxBridgeOutcome::InvalidChunk => Some(Discontinuity::FramingError),
            UartOwnedRxBridgeOutcome::NoChunk | UartOwnedRxBridgeOutcome::Disabled => {
                Some(Discontinuity::TransportReset)
            }
            UartOwnedRxBridgeOutcome::RecycleFailed => {
                panic!("UART RX DMA buffer could not be recycled")
            }
        },
        UartRxIrqOutcome::DmaError => Some(Discontinuity::DmaError),
        UartRxIrqOutcome::DeliveryError(UartRxDeliveryError::FilledQueueFull) => {
            panic!("UART filled queue full; RX buffer ownership would be lost")
        }
        UartRxIrqOutcome::DeliveryError(UartRxDeliveryError::NoFreshBuffer) => match event {
            UartRxEvent::DmaComplete => panic!("UART RX free-buffer pool exhausted"),
            UartRxEvent::IdleLine => Some(Discontinuity::TransportReset),
        },
        UartRxIrqOutcome::DeliveryError(UartRxDeliveryError::PlannerRejected) => match event {
            UartRxEvent::DmaComplete => Some(Discontinuity::TransportReset),
            UartRxEvent::IdleLine => panic!("UART RX IDLE planner did not deliver a buffer"),
        },
        UartRxIrqOutcome::DeliveryError(UartRxDeliveryError::TransferNotReady) => {
            Some(Discontinuity::TransportReset)
        }
    };
    if let Some(cause) = cause {
        port.bridge.record_discontinuity(cause, generation, now);
    }
    cause
}

/// What a shared port task needs of a receive port, whichever UART and DMA
/// stream a board wires it to. A port config left unbound is `None` and does
/// nothing; its peripheral was never enabled, so its interrupts never fire.
pub trait UartRxPortService {
    fn service(&mut self, event: UartRxEvent, now: TimestampMicros) -> Option<Discontinuity>;
}

impl<Irq> UartRxPortService for UartRxPort<Irq>
where
    Irq: UartRxIrqService,
{
    fn service(&mut self, event: UartRxEvent, now: TimestampMicros) -> Option<Discontinuity> {
        service_uart_rx_port(self, event, now)
    }
}

impl<P> UartRxPortService for Option<P>
where
    P: UartRxPortService,
{
    fn service(&mut self, event: UartRxEvent, now: TimestampMicros) -> Option<Discontinuity> {
        self.as_mut()?.service(event, now)
    }
}

/// A transmit path that was not started refuses every chunk and ignores its
/// interrupt.
impl<T> UartTxDmaService for Option<T>
where
    T: UartTxDmaService,
{
    fn start_chunk(
        &mut self,
        chunk: &TxChunk<MSP_V1_MAX_FRAME_LEN>,
    ) -> Result<(), UartTxStartError> {
        match self {
            Some(tx_dma) => tx_dma.start_chunk(chunk),
            None => Err(UartTxStartError::TransferMissing),
        }
    }

    fn service_irq(&mut self) -> UartTxIrqOutcome {
        match self {
            Some(tx_dma) => tx_dma.service_irq(),
            None => UartTxIrqOutcome::Ignored,
        }
    }
}

/// A port's transmit side, present only when its function talks back: the DMA
/// stream, the owner the transmit worker drains, and the completion its
/// interrupt reports to.
pub struct UartTxPort<Tx> {
    pub dma: Option<Tx>,
    pub owner: Option<UartOwnedTxOwner<'static>>,
    pub completion: Option<UartOwnedTxCompletion<'static>>,
}

impl<Tx> UartTxPort<Tx> {
    pub const fn unbound() -> Self {
        Self {
            dma: None,
            owner: None,
            completion: None,
        }
    }
}

/// Static buffers and the owned stream for one receive port.
pub struct UartRxPortStorage {
    pub rx: UartRxStorageResources,
    pub stream: &'static mut UartOwnedRxChannel,
}

/// Static buffers for a port that can transmit as well.
pub struct UartRxTxPortStorage {
    pub rx: UartRxStorageResources,
    pub stream: &'static mut UartOwnedRxChannel,
    pub tx_buffer: UartTxBuf,
    pub tx_stream: &'static mut UartOwnedTxChannel,
}

/// Pair a started receive side with its owned stream: the port keeps the
/// producer end, the function's endpoint gets the reader.
pub fn rx_port<Irq>(
    port: LogicalSerialPort,
    irq: Irq,
    parser: UartRxParserSide,
    stream: &'static mut UartOwnedRxChannel,
    writer: Option<UartOwnedWriter<'static>>,
) -> (UartRxPort<Irq>, SerialPortEndpoint) {
    let (producer, reader, discontinuities) = stream.split();
    (
        UartRxPort {
            irq,
            bridge: UartOwnedRxBridge::new(parser, producer),
        },
        SerialPortEndpoint {
            port,
            reader,
            discontinuities,
            writer,
        },
    )
}

/// Split a transmit stream for a port whose transmit DMA has started: the
/// writer goes to the function, the owner and completion to the port tasks.
pub fn tx_port<Tx>(
    dma: Tx,
    stream: &'static mut UartOwnedTxChannel,
) -> (UartTxPort<Tx>, UartOwnedWriter<'static>) {
    let (writer, owner, completion) = stream.split();
    (
        UartTxPort {
            dma: Some(dma),
            owner: Some(owner),
            completion: Some(completion),
        },
        writer,
    )
}

/// Give the function bound to `port` its endpoint.
pub fn place_endpoint(
    functions: &mut SerialFunctionSlots<SerialPortEndpoint>,
    bindings: &ResolvedBindings,
    port: LogicalSerialPort,
    endpoint: SerialPortEndpoint,
) {
    // Resolved bindings hold each function at most once.
    let _ = functions.place(bindings.function(port), endpoint);
}

/// Resolve the bindings to start the ports with: `saved` when flash holds a
/// table, else the board's `defaults`, with RC input in `rc_protocol`. Every
/// binding the board cannot carry is logged and left off, and each bound port
/// is logged.
pub fn resolve_boot_serial_bindings(
    saved: Option<SerialBindings>,
    defaults: SerialBindings,
    routes: &[SerialRoute],
    rc_protocol: RcProtocol,
) -> ResolvedBindings {
    if saved.is_none() {
        info!("Serial ports use the board's default bindings");
    }
    info!("RC input protocol: {=str}", rc_protocol.name());
    let resolved = resolve_bindings(saved.unwrap_or(defaults), routes, rc_protocol);
    for issue in resolved.issues() {
        warn!(
            "Serial binding {=str} -> {=str} dropped: the port cannot carry it",
            issue.port.name(),
            issue.function.name()
        );
    }
    for port in LogicalSerialPort::ALL {
        let function = resolved.function(port);
        if function != SerialFunction::None {
            info!("Serial {=str}: {=str}", port.name(), function.name());
        }
    }
    resolved
}

/// The longest `serial` reply line, inside a USB response frame.
const SERIAL_REPLY_LINE_CAPACITY: usize = 64;

/// The `serial` reply, one line at a time so each fits a USB response frame
/// however many ports a board routes: a header saying whether the table is
/// saved or the board's defaults and how many port lines follow, then
/// `uartN=function` for each port the board routes.
pub fn write_serial_bindings(
    stored: Option<SerialBindings>,
    defaults: SerialBindings,
    routes: &[SerialRoute],
    mut line: impl FnMut(&str),
) {
    let source = if stored.is_some() { "saved" } else { "default" };
    let bindings = stored.unwrap_or(defaults);
    let mut text = heapless::String::<SERIAL_REPLY_LINE_CAPACITY>::new();
    // Neither line can exceed the capacity: the header is fixed apart from a
    // port count below ten, and a port line is at most 24 bytes.
    let _ = write!(
        text,
        "OK serial {source} ports={}; applies after save and reboot\r\n",
        routes.len()
    );
    line(&text);
    for route in routes {
        text.clear();
        let _ = write!(
            text,
            "OK {}={}\r\n",
            route.logical.name(),
            bindings.function(route.logical).name()
        );
        line(&text);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use ferrowasp_io_core::serial::{SerialCapabilities, SerialProfile};
    use std::{string::String, vec::Vec};

    const fn route(logical: LogicalSerialPort) -> SerialRoute {
        SerialRoute {
            logical,
            peripheral: "test",
            tx_pin: "test",
            rx_pin: "test",
            profile: SerialProfile::disabled(),
            rx_dma: "test",
            tx_dma: None,
            capabilities: SerialCapabilities {
                sbus: true,
                crsf: false,
                mavlink: false,
                msp: false,
                esc_telemetry: true,
                cli: false,
                tx: false,
            },
        }
    }

    fn reply(stored: Option<SerialBindings>, routes: &[SerialRoute]) -> Vec<String> {
        let defaults =
            SerialBindings::none().with(LogicalSerialPort::Uart2, SerialFunction::RcInput);
        let mut lines = Vec::new();
        write_serial_bindings(stored, defaults, routes, |line| {
            lines.push(String::from(line))
        });
        lines
    }

    #[test]
    fn the_reply_is_a_counted_header_then_one_line_per_routed_port() {
        let routes = [
            route(LogicalSerialPort::Uart1),
            route(LogicalSerialPort::Uart2),
        ];
        assert_eq!(
            reply(None, &routes),
            [
                "OK serial default ports=2; applies after save and reboot\r\n",
                "OK uart1=none\r\n",
                "OK uart2=rc\r\n",
            ]
        );
        let saved =
            SerialBindings::none().with(LogicalSerialPort::Uart1, SerialFunction::EscTelemetry);
        assert_eq!(
            reply(Some(saved), &routes)[..2],
            [
                "OK serial saved ports=2; applies after save and reboot\r\n",
                "OK uart1=esc_telemetry\r\n",
            ]
        );
    }

    #[test]
    fn every_line_fits_a_usb_response_frame() {
        let routes = LogicalSerialPort::ALL.map(route);
        let mut worst = SerialBindings::none();
        for port in LogicalSerialPort::ALL {
            worst.set(port, SerialFunction::EscTelemetry);
        }
        let lines = reply(Some(worst), &routes);
        assert_eq!(lines.len(), 1 + LogicalSerialPort::ALL.len());
        for line in lines {
            assert!(line.ends_with("\r\n"), "{line:?} lost its terminator");
            assert!(line.len() <= SERIAL_REPLY_LINE_CAPACITY);
        }
    }
}
