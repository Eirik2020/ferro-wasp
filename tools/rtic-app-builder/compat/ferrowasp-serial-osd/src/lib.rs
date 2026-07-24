#![no_std]
#![forbid(unsafe_code)]

use ferrowasp_mspv1::{MspOsdTelemetry, MspParser, MspResponder, OSD_TX_BUFFER_LEN};
use heapless::Deque;
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

#[derive(Clone)]
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

/// HAL-free boundary consumed by the OSD component and produced/consumed by
/// the fixed USART1 hardware tasks.
pub struct SerialRxTx<const N: usize, const RX_DEPTH: usize, const TX_DEPTH: usize> {
    rx: Deque<SerialChunk<N>, RX_DEPTH>,
    tx: Deque<SerialChunk<N>, TX_DEPTH>,
    refresh_pending: bool,
    pub rx_overflows: u32,
    pub tx_overflows: u32,
}

impl<const N: usize, const RX_DEPTH: usize, const TX_DEPTH: usize>
    SerialRxTx<N, RX_DEPTH, TX_DEPTH>
{
    pub const fn new() -> Self {
        Self {
            rx: Deque::new(),
            tx: Deque::new(),
            refresh_pending: false,
            rx_overflows: 0,
            tx_overflows: 0,
        }
    }

    pub fn publish_rx(&mut self, bytes: &[u8]) {
        let Some(chunk) = SerialChunk::from_slice(bytes) else {
            return;
        };
        if self.rx.push_back(chunk).is_err() {
            self.rx_overflows = self.rx_overflows.saturating_add(1);
        }
    }

    pub fn take_rx(&mut self) -> Option<SerialChunk<N>> {
        self.rx.pop_front()
    }

    pub fn enqueue_tx(&mut self, bytes: &[u8]) {
        let Some(chunk) = SerialChunk::from_slice(bytes) else {
            return;
        };
        if self.tx.push_back(chunk).is_err() {
            self.tx_overflows = self.tx_overflows.saturating_add(1);
        }
    }

    pub fn take_tx(&mut self) -> Option<SerialChunk<N>> {
        self.tx.pop_front()
    }

    pub fn request_refresh(&mut self) {
        self.refresh_pending = true;
    }

    fn take_refresh(&mut self) -> bool {
        core::mem::take(&mut self.refresh_pending)
    }
}

impl<const N: usize, const RX_DEPTH: usize, const TX_DEPTH: usize> Default
    for SerialRxTx<N, RX_DEPTH, TX_DEPTH>
{
    fn default() -> Self {
        Self::new()
    }
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

    pub fn service_idle<const RX_DEPTH: usize, const TX_DEPTH: usize>(
        &mut self,
        endpoint: &mut SerialRxTx<N, RX_DEPTH, TX_DEPTH>,
    ) -> bool {
        if !self.transfer.is_idle() {
            return false;
        }
        let len = N.saturating_sub(self.transfer.number_of_transfers() as usize);
        let delivered = self.rotate(len, endpoint);
        self.transfer.clear_idle_interrupt();
        delivered
    }

    pub fn service_dma<const RX_DEPTH: usize, const TX_DEPTH: usize>(
        &mut self,
        endpoint: &mut SerialRxTx<N, RX_DEPTH, TX_DEPTH>,
    ) -> bool {
        let flags = self.transfer.flags();
        if flags.is_transfer_error() || flags.is_direct_mode_error() || flags.is_fifo_error() {
            self.transfer.clear_all_flags();
            return false;
        }
        if !flags.is_transfer_complete() {
            return false;
        }
        let delivered = self.rotate(N, endpoint);
        self.transfer.clear_all_flags();
        delivered
    }

    fn rotate<const RX_DEPTH: usize, const TX_DEPTH: usize>(
        &mut self,
        len: usize,
        endpoint: &mut SerialRxTx<N, RX_DEPTH, TX_DEPTH>,
    ) -> bool {
        if len == 0 {
            return false;
        }
        let Some(spare) = self.spare.take() else {
            return false;
        };
        let Ok((filled, _)) = self.transfer.next_transfer(spare) else {
            return false;
        };
        endpoint.publish_rx(&filled[..len.min(N)]);
        self.spare = Some(filled);
        true
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

    pub fn start_next<const RX_DEPTH: usize, const TX_DEPTH: usize>(
        &mut self,
        endpoint: &mut SerialRxTx<N, RX_DEPTH, TX_DEPTH>,
    ) {
        if self.busy {
            return;
        }
        let Some(chunk) = endpoint.take_tx() else {
            return;
        };
        let Some(old) = self.transfer.take() else {
            return;
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
    }

    pub fn service_irq<const RX_DEPTH: usize, const TX_DEPTH: usize>(
        &mut self,
        endpoint: &mut SerialRxTx<N, RX_DEPTH, TX_DEPTH>,
    ) {
        let Some(transfer) = self.transfer.as_mut() else {
            return;
        };
        let flags = transfer.flags();
        if !(flags.is_transfer_complete()
            || flags.is_transfer_error()
            || flags.is_direct_mode_error())
        {
            if flags.is_fifo_error() {
                transfer.clear_fifo_error();
            }
            return;
        }
        transfer.clear_all_flags();
        self.busy = false;
        self.start_next(endpoint);
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

    pub fn process<const N: usize, const RX_DEPTH: usize, const TX_DEPTH: usize>(
        &mut self,
        endpoint: &mut SerialRxTx<N, RX_DEPTH, TX_DEPTH>,
        telemetry: &OsdTelemetryState,
        output: &mut [u8; OSD_TX_BUFFER_LEN],
    ) -> bool {
        self.telemetry = telemetry.snapshot();
        let armed_changed = self.telemetry.armed != self.displayed_armed;
        let mut queued = false;
        while let Some(chunk) = endpoint.take_rx() {
            for byte in chunk.as_slice() {
                if let Ok(Some(packet)) = self.parser.parse(*byte)
                    && let Some(len) = self.responder.reply(&packet, &self.telemetry, output)
                {
                    endpoint.enqueue_tx(&output[..len]);
                    queued = true;
                }
            }
        }
        if endpoint.take_refresh() {
            if let Some(len) = self.responder.heartbeat(output) {
                endpoint.enqueue_tx(&output[..len]);
                queued = true;
            }
            if armed_changed {
                let text: &[u8] = if self.telemetry.armed {
                    b"ARMED"
                } else {
                    b"DISARMED"
                };
                if let Some(len) = self.responder.write_string(2, 2, 0, text, output) {
                    endpoint.enqueue_tx(&output[..len]);
                    queued = true;
                }
                if let Some(len) = self.responder.draw_screen(output) {
                    endpoint.enqueue_tx(&output[..len]);
                    queued = true;
                }
                self.displayed_armed = self.telemetry.armed;
            }
            if let Some(len) = self.next_overlay_frame(output) {
                endpoint.enqueue_tx(&output[..len]);
                queued = true;
            }
        }
        queued
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
