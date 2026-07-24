#![no_std]
#![forbid(unsafe_code)]

use ferrowasp_mspv1::{MspOsdTelemetry, MspParser, MspResponder, OSD_TX_BUFFER_LEN};
use stm32f4xx_hal::{
    dma::{
        MemoryToPeripheral, PeripheralToMemory, Stream5, Stream7, Transfer,
        config::DmaConfig,
        traits::{DmaFlagExt, StreamISR},
    },
    pac::{DMA2, USART1},
    serial::{self, RxISR},
    ClearFlags, ReadFlags,
};

pub type Usart1RxTransfer<const N: usize> = Transfer<
    Stream5<DMA2>,
    4,
    serial::Rx<USART1>,
    PeripheralToMemory,
    &'static mut [u8; N],
>;

pub type Usart1TxTransfer<const N: usize> = Transfer<
    Stream7<DMA2>,
    4,
    serial::Tx<USART1>,
    MemoryToPeripheral,
    &'static mut [u8; N],
>;

#[derive(Clone, Copy, Debug, Default)]
pub struct OsdTelemetryState {
    telemetry: MspOsdTelemetry,
}

impl OsdTelemetryState {
    pub fn toggle_armed(&mut self) -> bool {
        self.telemetry.armed = !self.telemetry.armed;
        self.telemetry.armed
    }

    pub fn snapshot(&self) -> MspOsdTelemetry {
        self.telemetry
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerialChunk<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> SerialChunk<N> {
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > N {
            return None;
        }
        let mut chunk = Self {
            bytes: [0; N],
            len: bytes.len(),
        };
        chunk.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(chunk)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// One item delivered to the long-lived OSD consumer task.
///
/// The generated NUCLEO application transports this over a bounded
/// `rtic_sync::channel`. Keeping refresh requests in the same MPSC channel as
/// RX chunks gives the consumer one loss-aware wake-up mechanism instead of
/// spawning an already-active RTIC software task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OsdWork<const N: usize> {
    Rx(SerialChunk<N>),
    Refresh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultSeverity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArmingEffect {
    None,
    Inhibit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultDefinition {
    pub id: OsdFaultId,
    pub name: &'static str,
    pub severity: FaultSeverity,
    pub latching: bool,
    pub arming_effect: ArmingEffect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OsdFaultId {
    RxDma,
    RxWorkQueueFull,
    RxWorkConsumerClosed,
    WorkProducersClosed,
    RefreshQueueFull,
    TxQueueFull,
    TxConsumerClosed,
    TxProducerClosed,
    TxDma,
    TxCompletionQueueFull,
    TxCompletionConsumerClosed,
    TxCompletionProducerClosed,
    DebounceSpawnFailed,
}

impl OsdFaultId {
    pub const fn definition(self) -> FaultDefinition {
        let (name, severity) = match self {
            Self::RxDma => ("serial-rx-dma", FaultSeverity::Error),
            Self::RxWorkQueueFull => ("serial-rx-overflow", FaultSeverity::Warning),
            Self::RxWorkConsumerClosed => ("serial-rx-consumer-closed", FaultSeverity::Error),
            Self::WorkProducersClosed => ("osd-work-producers-closed", FaultSeverity::Error),
            Self::RefreshQueueFull => ("osd-refresh-overflow", FaultSeverity::Warning),
            Self::TxQueueFull => ("serial-tx-overflow", FaultSeverity::Warning),
            Self::TxConsumerClosed => ("serial-tx-consumer-closed", FaultSeverity::Error),
            Self::TxProducerClosed => ("serial-tx-producer-closed", FaultSeverity::Error),
            Self::TxDma => ("serial-tx-dma", FaultSeverity::Error),
            Self::TxCompletionQueueFull => {
                ("serial-tx-completion-overflow", FaultSeverity::Error)
            }
            Self::TxCompletionConsumerClosed => {
                ("serial-tx-completion-consumer-closed", FaultSeverity::Error)
            }
            Self::TxCompletionProducerClosed => {
                ("serial-tx-completion-producer-closed", FaultSeverity::Error)
            }
            Self::DebounceSpawnFailed => ("button-debounce-spawn-failed", FaultSeverity::Error),
        };
        FaultDefinition {
            id: self,
            name,
            severity,
            latching: false,
            // This is a validation-only NUCLEO display path. It has no actuator
            // authority, so these faults cannot themselves make an arming claim.
            arming_effect: ArmingEffect::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OsdFaultState {
    rx_dma: u32,
    rx_work_queue_full: u32,
    rx_work_consumer_closed: u32,
    work_producers_closed: u32,
    refresh_queue_full: u32,
    tx_queue_full: u32,
    tx_consumer_closed: u32,
    tx_producer_closed: u32,
    tx_dma: u32,
    tx_completion_queue_full: u32,
    tx_completion_consumer_closed: u32,
    tx_completion_producer_closed: u32,
    debounce_spawn_failed: u32,
}

impl OsdFaultState {
    pub fn record(&mut self, fault: OsdFaultId) {
        let counter = match fault {
            OsdFaultId::RxDma => &mut self.rx_dma,
            OsdFaultId::RxWorkQueueFull => &mut self.rx_work_queue_full,
            OsdFaultId::RxWorkConsumerClosed => &mut self.rx_work_consumer_closed,
            OsdFaultId::WorkProducersClosed => &mut self.work_producers_closed,
            OsdFaultId::RefreshQueueFull => &mut self.refresh_queue_full,
            OsdFaultId::TxQueueFull => &mut self.tx_queue_full,
            OsdFaultId::TxConsumerClosed => &mut self.tx_consumer_closed,
            OsdFaultId::TxProducerClosed => &mut self.tx_producer_closed,
            OsdFaultId::TxDma => &mut self.tx_dma,
            OsdFaultId::TxCompletionQueueFull => &mut self.tx_completion_queue_full,
            OsdFaultId::TxCompletionConsumerClosed => {
                &mut self.tx_completion_consumer_closed
            }
            OsdFaultId::TxCompletionProducerClosed => {
                &mut self.tx_completion_producer_closed
            }
            OsdFaultId::DebounceSpawnFailed => &mut self.debounce_spawn_failed,
        };
        *counter = counter.saturating_add(1);
    }

    pub const fn count(&self, fault: OsdFaultId) -> u32 {
        match fault {
            OsdFaultId::RxDma => self.rx_dma,
            OsdFaultId::RxWorkQueueFull => self.rx_work_queue_full,
            OsdFaultId::RxWorkConsumerClosed => self.rx_work_consumer_closed,
            OsdFaultId::WorkProducersClosed => self.work_producers_closed,
            OsdFaultId::RefreshQueueFull => self.refresh_queue_full,
            OsdFaultId::TxQueueFull => self.tx_queue_full,
            OsdFaultId::TxConsumerClosed => self.tx_consumer_closed,
            OsdFaultId::TxProducerClosed => self.tx_producer_closed,
            OsdFaultId::TxDma => self.tx_dma,
            OsdFaultId::TxCompletionQueueFull => self.tx_completion_queue_full,
            OsdFaultId::TxCompletionConsumerClosed => self.tx_completion_consumer_closed,
            OsdFaultId::TxCompletionProducerClosed => self.tx_completion_producer_closed,
            OsdFaultId::DebounceSpawnFailed => self.debounce_spawn_failed,
        }
    }

    pub fn record_n(&mut self, fault: OsdFaultId, count: u32) {
        for _ in 0..count {
            self.record(fault);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RxServiceResult<const N: usize> {
    NoData,
    Chunk(SerialChunk<N>),
    Fault(OsdFaultId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxCompletion {
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TxIrqOutcome {
    pub completion: Option<TxCompletion>,
    pub fault: Option<OsdFaultId>,
}

pub struct Usart1RxDma<const N: usize> {
    transfer: Usart1RxTransfer<N>,
    spare: Option<&'static mut [u8; N]>,
}

impl<const N: usize> Usart1RxDma<N> {
    pub fn new(
        stream: Stream5<DMA2>,
        rx: serial::Rx<USART1>,
        active: &'static mut [u8; N],
        spare: &'static mut [u8; N],
    ) -> Self {
        let mut transfer = Transfer::init_peripheral_to_memory(
            stream,
            rx,
            active,
            None,
            DmaConfig::default()
                .memory_increment(true)
                .fifo_enable(true)
                .transfer_error_interrupt(true)
                .direct_mode_error_interrupt(true)
                .fifo_error_interrupt(true)
                .transfer_complete_interrupt(true),
        );
        transfer.start(|_| {});
        Self {
            transfer,
            spare: Some(spare),
        }
    }

    pub fn service_idle(&mut self) -> RxServiceResult<N> {
        if !self.transfer.is_idle() {
            return RxServiceResult::NoData;
        }
        let len = N.saturating_sub(self.transfer.number_of_transfers() as usize);
        let delivered = self.rotate(len);
        self.transfer.clear_idle_interrupt();
        delivered
    }

    pub fn service_dma(&mut self) -> RxServiceResult<N> {
        let flags = self.transfer.flags();
        if flags.is_transfer_error() || flags.is_direct_mode_error() || flags.is_fifo_error() {
            self.transfer.clear_all_flags();
            return RxServiceResult::Fault(OsdFaultId::RxDma);
        }
        if !flags.is_transfer_complete() {
            return RxServiceResult::NoData;
        }
        let delivered = self.rotate(N);
        self.transfer.clear_all_flags();
        delivered
    }

    fn rotate(&mut self, len: usize) -> RxServiceResult<N> {
        if len == 0 {
            return RxServiceResult::NoData;
        }
        let Some(spare) = self.spare.take() else {
            return RxServiceResult::Fault(OsdFaultId::RxDma);
        };
        let Ok((filled, _)) = self.transfer.next_transfer(spare) else {
            return RxServiceResult::Fault(OsdFaultId::RxDma);
        };
        let chunk = SerialChunk::from_slice(&filled[..len.min(N)]);
        self.spare = Some(filled);
        match chunk {
            Some(chunk) => RxServiceResult::Chunk(chunk),
            None => RxServiceResult::Fault(OsdFaultId::RxDma),
        }
    }
}

pub struct Usart1TxDma<const N: usize> {
    transfer: Option<Usart1TxTransfer<N>>,
    config: DmaConfig,
    busy: bool,
}

impl<const N: usize> Usart1TxDma<N> {
    pub fn new(
        stream: Stream7<DMA2>,
        tx: serial::Tx<USART1>,
        buffer: &'static mut [u8; N],
    ) -> Self {
        let config = DmaConfig::default()
            .memory_increment(true)
            .fifo_enable(true)
            .transfer_error_interrupt(true)
            .direct_mode_error_interrupt(true)
            .fifo_error_interrupt(true)
            .transfer_complete_interrupt(true);
        Self {
            transfer: Some(Transfer::init_memory_to_peripheral(
                stream, tx, buffer, None, config,
            )),
            config,
            busy: false,
        }
    }

    pub fn start(&mut self, chunk: SerialChunk<N>) -> Result<(), OsdFaultId> {
        if self.busy {
            return Err(OsdFaultId::TxDma);
        }
        let Some(old) = self.transfer.take() else {
            return Err(OsdFaultId::TxDma);
        };
        let (stream, tx, buffer, _) = old.release();
        buffer.fill(0);
        let bytes = chunk.as_slice();
        buffer[..bytes.len()].copy_from_slice(bytes);
        let mut transfer =
            Transfer::init_memory_to_peripheral(stream, tx, buffer, None, self.config);
        transfer.start(|_| {});
        self.transfer = Some(transfer);
        self.busy = true;
        Ok(())
    }

    pub fn service_irq(&mut self) -> TxIrqOutcome {
        let Some(transfer) = self.transfer.as_mut() else {
            return TxIrqOutcome {
                completion: Some(TxCompletion::Failed),
                fault: Some(OsdFaultId::TxDma),
            };
        };
        let flags = transfer.flags();
        let terminal_fault = flags.is_transfer_error() || flags.is_direct_mode_error();
        if flags.is_fifo_error() && !flags.is_transfer_complete() && !terminal_fault {
            transfer.clear_fifo_error();
            return TxIrqOutcome {
                completion: None,
                fault: Some(OsdFaultId::TxDma),
            };
        }
        if !flags.is_transfer_complete() && !terminal_fault {
            return TxIrqOutcome::default();
        }
        transfer.clear_all_flags();
        self.busy = false;
        if terminal_fault {
            TxIrqOutcome {
                completion: Some(TxCompletion::Failed),
                fault: Some(OsdFaultId::TxDma),
            }
        } else {
            TxIrqOutcome {
                completion: Some(TxCompletion::Complete),
                fault: None,
            }
        }
    }
}

/// Protocol-only consumer. It has no STM32 or USART ownership.
pub struct OsdComponent {
    parser: MspParser,
    responder: MspResponder,
    telemetry: MspOsdTelemetry,
    overlay_step: u8,
    displayed_armed: bool,
}

impl OsdComponent {
    pub fn new() -> Self {
        Self {
            parser: MspParser::new(),
            responder: MspResponder::new(),
            telemetry: MspOsdTelemetry::default(),
            overlay_step: 0,
            displayed_armed: false,
        }
    }

    /// Processes one channel-delivered work item.
    ///
    /// Component state and parsing stay local to the divergent OSD task.
    /// `enqueue` performs a non-blocking send to the separate TX channel, so
    /// no RTIC shared-resource lock spans parsing or frame rendering.
    pub fn process_work<const N: usize, F>(
        &mut self,
        work: OsdWork<N>,
        telemetry: MspOsdTelemetry,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
        mut enqueue: F,
    ) -> u32
    where
        F: FnMut(SerialChunk<N>) -> bool,
    {
        self.telemetry = telemetry;
        let armed_changed = self.telemetry.armed != self.displayed_armed;
        let mut dropped = 0_u32;
        match work {
            OsdWork::Rx(chunk) => {
                for byte in chunk.as_slice() {
                    if let Ok(Some(packet)) = self.parser.parse(*byte)
                        && let Some(len) =
                            self.responder.reply(&packet, &self.telemetry, output)
                    {
                        dropped = dropped.saturating_add(enqueue_output(
                            output,
                            len,
                            &mut enqueue,
                        ));
                    }
                }
            }
            OsdWork::Refresh => {
                if let Some(len) = self.responder.heartbeat(output) {
                    dropped = dropped.saturating_add(enqueue_output(
                        output,
                        len,
                        &mut enqueue,
                    ));
                }
                if armed_changed {
                    let text: &[u8] = if self.telemetry.armed {
                        b"ARMED"
                    } else {
                        b"DISARMED"
                    };
                    if let Some(len) = self.responder.write_string(2, 2, 0, text, output) {
                        dropped = dropped.saturating_add(enqueue_output(
                            output,
                            len,
                            &mut enqueue,
                        ));
                    }
                    if let Some(len) = self.responder.draw_screen(output) {
                        dropped = dropped.saturating_add(enqueue_output(
                            output,
                            len,
                            &mut enqueue,
                        ));
                    }
                    self.displayed_armed = self.telemetry.armed;
                }
                if let Some(len) = self.next_overlay_frame(output) {
                    dropped = dropped.saturating_add(enqueue_output(
                        output,
                        len,
                        &mut enqueue,
                    ));
                }
            }
        }
        dropped
    }

    // Ported from FerroWasp's OsdTask::next_overlay_frame. One bounded frame
    // is emitted per refresh so DMA ownership remains in the hardware layer.
    fn next_overlay_frame(&mut self, output: &mut [u8; OSD_TX_BUFFER_LEN]) -> Option<usize> {
        let len = match self.overlay_step {
            0 => self.responder.clear_screen(output),
            1 => self.responder.write_string(1, 2, 0, b"FERROWASP", output),
            2 => {
                let text: &[u8] = if self.telemetry.armed { b"ARMED" } else { b"DISARMED" };
                self.responder.write_string(2, 2, 0, text, output)
            }
            3 => {
                let text: &[u8] = if self.telemetry.imu_stale { b"IMU STALE" } else { b"IMU OK" };
                self.responder.write_string(3, 2, 0, text, output)
            }
            4 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"VBAT ");
                write_vbat(&mut text[5..], self.telemetry.battery_voltage_v10);
                self.responder.write_string(4, 2, 0, trim_end(&text), output)
            }
            5 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"CELL ");
                write_cell_voltage(
                    &mut text[5..],
                    self.telemetry.battery_cell_count,
                    self.telemetry.battery_cell_voltage_v100,
                );
                self.responder.write_string(5, 2, 0, trim_end(&text), output)
            }
            6 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"CUR ");
                write_current(&mut text[4..], self.telemetry.amperage_ca);
                self.responder.write_string(6, 2, 0, trim_end(&text), output)
            }
            7 => {
                let mut text = [b' '; 18];
                copy_text(&mut text, b"THR ");
                write_u16(&mut text[4..], self.telemetry.osd_throttle);
                self.responder.write_string(7, 2, 0, trim_end(&text), output)
            }
            8..=13 => self.responder.write_string(
                self.overlay_step,
                2,
                0,
                &[b' '; 18],
                output,
            ),
            14 => self.responder.draw_screen(output),
            _ => self.responder.heartbeat(output),
        };
        self.overlay_step = (self.overlay_step + 1) % 15;
        len
    }
}

fn enqueue_output<const N: usize, F>(
    output: &[u8; OSD_TX_BUFFER_LEN],
    len: usize,
    enqueue: &mut F,
) -> u32
where
    F: FnMut(SerialChunk<N>) -> bool,
{
    let Some(chunk) = SerialChunk::from_slice(&output[..len.min(output.len())]) else {
        return 1;
    };
    u32::from(!enqueue(chunk))
}

fn copy_text(output: &mut [u8], text: &[u8]) {
    let len = output.len().min(text.len());
    output[..len].copy_from_slice(&text[..len]);
}

fn trim_end(text: &[u8]) -> &[u8] {
    let mut len = text.len();
    while len > 0 && text[len - 1] == b' ' {
        len -= 1;
    }
    &text[..len]
}

fn write_vbat(output: &mut [u8], voltage_v10: u8) {
    let whole = voltage_v10 / 10;
    let frac = voltage_v10 % 10;
    let mut pos = write_u16(output, whole as u16);
    if pos + 2 <= output.len() {
        output[pos] = b'.';
        pos += 1;
        output[pos] = b'0' + frac;
    }
}

fn write_cell_voltage(output: &mut [u8], cell_count: u8, voltage_v100: u16) {
    if cell_count == 0 || voltage_v100 == 0 {
        copy_text(output, b"---");
        return;
    }
    let whole = voltage_v100 / 100;
    let frac = voltage_v100 % 100;
    let mut pos = write_u16(output, whole);
    if pos + 3 <= output.len() {
        output[pos] = b'.';
        pos += 1;
        output[pos] = b'0' + (frac / 10) as u8;
        pos += 1;
        output[pos] = b'0' + (frac % 10) as u8;
    }
}

fn write_current(output: &mut [u8], amperage_ca: i16) {
    if amperage_ca < 0 {
        copy_text(output, b"---");
        return;
    }
    let amperage_ca = amperage_ca as u16;
    let whole = amperage_ca / 100;
    let frac = (amperage_ca % 100) / 10;
    let mut pos = write_u16(output, whole);
    if pos + 3 <= output.len() {
        output[pos] = b'.';
        pos += 1;
        output[pos] = b'0' + frac as u8;
        pos += 1;
        output[pos] = b'A';
    }
}

fn write_u16(output: &mut [u8], value: u16) -> usize {
    let mut digits = [0u8; 5];
    let mut value = value;
    let mut count = 0;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 || count == digits.len() {
            break;
        }
    }
    let len = output.len().min(count);
    for i in 0..len {
        output[i] = digits[count - 1 - i];
    }
    len
}

impl Default for OsdComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::*;

    #[test]
    fn fault_sink_counts_typed_faults_without_aliasing() {
        let mut faults = OsdFaultState::default();
        faults.record(OsdFaultId::RxWorkQueueFull);
        faults.record(OsdFaultId::RxWorkQueueFull);
        faults.record(OsdFaultId::TxDma);

        assert_eq!(faults.count(OsdFaultId::RxWorkQueueFull), 2);
        assert_eq!(faults.count(OsdFaultId::TxDma), 1);
        assert_eq!(faults.count(OsdFaultId::TxQueueFull), 0);
        assert_eq!(
            OsdFaultId::RxWorkQueueFull.definition().arming_effect,
            ArmingEffect::None
        );
    }

    #[test]
    fn refresh_emits_bounded_frames_without_shared_endpoint_state() {
        let mut component = OsdComponent::new();
        let mut output = [0_u8; OSD_TX_BUFFER_LEN];
        let mut frames = Vec::new();

        let dropped = component.process_work(
            OsdWork::<OSD_TX_BUFFER_LEN>::Refresh,
            MspOsdTelemetry::default(),
            &mut output,
            |frame| {
                frames.push(frame);
                true
            },
        );

        assert_eq!(dropped, 0);
        assert!(!frames.is_empty());
        assert!(frames
            .iter()
            .all(|frame| !frame.as_slice().is_empty() && frame.as_slice().len() <= 70));
    }

    #[test]
    fn rejected_tx_frames_are_returned_as_a_bounded_drop_count() {
        let mut component = OsdComponent::new();
        let mut output = [0_u8; OSD_TX_BUFFER_LEN];

        let dropped = component.process_work(
            OsdWork::<OSD_TX_BUFFER_LEN>::Refresh,
            MspOsdTelemetry::default(),
            &mut output,
            |_| false,
        );

        assert!(dropped > 0);
    }
}
