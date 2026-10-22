//! Flight records on their way to storage: the bounded queue from the control
//! loop, page assembly, and finding the append point and each page's flight
//! in an append-only log.
//!
//! Requirements: LOG-001

use ferrowasp_boundary_contracts::messages::ControlTick;
use ferrowasp_io_contracts::link::Stamped;
use ferrowasp_protocol::blackbox::{
    FLASH_PAGE_LEN, FLIGHT_RECORD_FLAG_BOOT_SESSION_START, FlightRecord, RECORDS_PER_PAGE,
    decode_page, encode_page,
};
use ferrowasp_protocol::rate_record::{CompactRateBlackboxFields, CompactRateBlackboxSample};
use ferrowasp_storage::layout::{StorageLayout, StorageReadError};
use heapless::spsc::{Consumer, Producer, Queue};

pub const RECORD_QUEUE_CAPACITY: usize = 32;

/// Records on their way to the log writer: the flash manager's staging
/// between the observer's blackbox stream, which a board sizes to ride out
/// its storage's longest write stall, and the page assembler.
pub type RecordQueue<const N: usize = RECORD_QUEUE_CAPACITY> = Queue<FlightRecord, N>;

pub type RecordProducer = Producer<'static, FlightRecord>;

pub type RecordConsumer = Consumer<'static, FlightRecord>;

/// The flight record for a control tick from the observer's blackbox
/// stream, or `None` for a tick the log rate divisor skips: a tick is kept
/// when its control sequence is a multiple of the divisor, clamped to 1-16.
pub fn record_for_tick(tick: &Stamped<ControlTick>, divisor: u32) -> Option<FlightRecord> {
    let divisor = divisor.clamp(1, 16);
    let control = &tick.value;
    if !control.control_seq.is_multiple_of(divisor) {
        return None;
    }
    let sample = CompactRateBlackboxSample::from_fields(CompactRateBlackboxFields {
        seq: control.control_seq,
        imu_seq: control.imu_seq,
        armed: control.armed,
        imu_fresh: control.imu_fresh,
        raw_gyro_dps: control.raw_gyro_dps,
        filtered_gyro_dps: control.filtered_gyro_dps,
        command_dps: control.command_dps,
        pid: control.pid,
        throttle: control.throttle,
        motors: control.motors,
    });
    Some(FlightRecord {
        // The log format keeps a 32-bit microsecond stamp, which wraps every
        // 71.6 minutes; the configurator's ULog conversion unwraps it from
        // consecutive records. The stamp is the tick's own.
        timestamp_us: tick.at.0 as u32,
        control_sequence: sample.seq,
        imu_sequence: sample.imu_seq,
        flags: u16::from(sample.flags),
        raw_gyro_dps10: sample.raw_gyro_dps10,
        filtered_gyro_dps10: sample.gyro_dps10,
        command_dps10: sample.command_dps10,
        pid: sample.pid,
        throttle: sample.throttle,
        motors: sample.motors,
    })
}

/// Finds the append point and next flight identifier in an append-only log.
///
/// Pages are only ever written in order, so the append point is the first
/// blank page. A page that does not decode, right after one that does, is a
/// page program cut short by power loss: it stays as the last page of its
/// flight, and the next flight follows it (see [`page_flight_id`]). Anything
/// else that does not decode means the region holds something other than
/// FerroWasp's log; it reads as empty and stays read-only until erased.
pub fn scan_log<E, Read>(
    layout: StorageLayout,
    mut read: Read,
) -> Result<(u32, u32, bool), StorageReadError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let mut page = [0xff; FLASH_PAGE_LEN];
    let mut read_page = |index: u32, page: &mut [u8; FLASH_PAGE_LEN]| {
        let address = layout
            .log_page_address(index)
            .ok_or(StorageReadError::InvalidLayout)?;
        read(address, page).map_err(StorageReadError::Device)
    };

    let mut low = 0u32;
    let mut high = layout.log_page_count;
    while low < high {
        let middle = low + (high - low) / 2;
        read_page(middle, &mut page)?;
        if page.iter().all(|byte| *byte == 0xff) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let next_page = low;
    if next_page == 0 {
        return Ok((0, 1, true));
    }

    const FOREIGN: (u32, u32, bool) = (0, 1, false);
    read_page(0, &mut page)?;
    if decode_page(&page).is_err() {
        return Ok(FOREIGN);
    }
    let newest = match page_flight_id(next_page - 1, |index, page| read_page(index, page))? {
        Some(flight_id) => flight_id,
        None => return Ok(FOREIGN),
    };
    // A full log has no blank page left to write.
    let writable = next_page < layout.log_page_count;
    Ok((next_page, newest.wrapping_add(1).max(1), writable))
}

/// The flight that log page `index` belongs to. A page that does not decode
/// right after one that does is a torn last page of that flight; `None` for
/// anything else that does not decode.
pub fn page_flight_id<E, Read>(index: u32, mut read: Read) -> Result<Option<u32>, E>
where
    Read: FnMut(u32, &mut [u8; FLASH_PAGE_LEN]) -> Result<(), E>,
{
    let mut page = [0xff; FLASH_PAGE_LEN];
    read(index, &mut page)?;
    if let Ok(metadata) = decode_page(&page) {
        return Ok(Some(metadata.flight_id));
    }
    let Some(previous) = index.checked_sub(1) else {
        return Ok(None);
    };
    read(previous, &mut page)?;
    Ok(decode_page(&page).ok().map(|metadata| metadata.flight_id))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssembleError {
    NotRecording,
    PageNotConsumed,
    Format,
}

pub struct PageAssembler {
    records: [FlightRecord; RECORDS_PER_PAGE],
    record_count: usize,
    flight_id: u32,
    page_sequence: u32,
    recording: bool,
    boot_session_start_pending: bool,
    ready_page: [u8; FLASH_PAGE_LEN],
    page_ready: bool,
}

impl PageAssembler {
    pub const fn new() -> Self {
        Self {
            records: [FlightRecord {
                timestamp_us: 0,
                control_sequence: 0,
                imu_sequence: 0,
                flags: 0,
                raw_gyro_dps10: [0; 3],
                filtered_gyro_dps10: [0; 3],
                command_dps10: [0; 3],
                pid: [0; 3],
                throttle: 0,
                motors: [0; 4],
            }; RECORDS_PER_PAGE],
            record_count: 0,
            flight_id: 0,
            page_sequence: 0,
            recording: false,
            boot_session_start_pending: false,
            ready_page: [0xff; FLASH_PAGE_LEN],
            page_ready: false,
        }
    }

    pub fn start(&mut self, flight_id: u32, boot_session_start: bool) {
        self.record_count = 0;
        self.flight_id = flight_id;
        self.page_sequence = 0;
        self.recording = true;
        self.boot_session_start_pending = boot_session_start;
        self.page_ready = false;
    }

    pub const fn recording(&self) -> bool {
        self.recording
    }

    /// Returns `true` when a complete page is available through
    /// [`Self::take_ready_page`]. The caller must consume that page before
    /// submitting another record.
    pub fn push(&mut self, mut record: FlightRecord) -> Result<bool, AssembleError> {
        if !self.recording {
            return Err(AssembleError::NotRecording);
        }
        if self.page_ready {
            return Err(AssembleError::PageNotConsumed);
        }
        if self.boot_session_start_pending {
            record.flags |= FLIGHT_RECORD_FLAG_BOOT_SESSION_START;
            self.boot_session_start_pending = false;
        }
        self.records[self.record_count] = record;
        self.record_count += 1;
        if self.record_count < RECORDS_PER_PAGE {
            return Ok(false);
        }
        self.finish_page()?;
        Ok(true)
    }

    pub fn stop(&mut self) -> Result<bool, AssembleError> {
        self.recording = false;
        if self.record_count == 0 {
            Ok(self.page_ready)
        } else {
            self.finish_page()?;
            Ok(true)
        }
    }

    /// The flight not yet wholly on flash: still recording, its last page
    /// not taken, or, with `page_pending`, a taken page not yet written.
    /// Log sync leaves it alone until it is complete.
    pub const fn unfinished_flight(&self, page_pending: bool) -> Option<u32> {
        if self.flight_id != 0 && (self.recording || self.page_ready || page_pending) {
            Some(self.flight_id)
        } else {
            None
        }
    }

    pub fn take_ready_page(&mut self) -> Option<[u8; FLASH_PAGE_LEN]> {
        if !self.page_ready {
            return None;
        }
        self.page_ready = false;
        Some(self.ready_page)
    }

    fn finish_page(&mut self) -> Result<(), AssembleError> {
        if self.page_ready {
            return Err(AssembleError::PageNotConsumed);
        }
        self.ready_page = encode_page(
            self.flight_id,
            self.page_sequence,
            &self.records[..self.record_count],
        )
        .map_err(|_| AssembleError::Format)?;
        self.page_ready = true;
        self.page_sequence = self.page_sequence.wrapping_add(1);
        self.record_count = 0;
        Ok(())
    }
}

impl Default for PageAssembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Produces the deterministic pattern used by destructive scratch-sector tests.
pub fn scratch_test_page() -> [u8; FLASH_PAGE_LEN] {
    let mut page = [0u8; FLASH_PAGE_LEN];
    for (index, byte) in page.iter_mut().enumerate() {
        *byte = (index as u8).rotate_left(1) ^ 0xa5;
    }
    page[..8].copy_from_slice(b"FWTEST01");
    page
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_io_contracts::time::TimestampMicros;
    use ferrowasp_protocol::blackbox::{decode_page, record_from_page};
    use ferrowasp_storage::layout::CONFIG_SECTOR_SIZE;

    fn record(sequence: u32) -> FlightRecord {
        FlightRecord {
            control_sequence: sequence,
            ..FlightRecord::default()
        }
    }

    /// A 64 KiB flash holding `pages` from the log start: `Some((flight,
    /// sequence))` for a whole page, `None` for one torn by a power cut.
    fn log_with(layout: StorageLayout, pages: &[Option<(u32, u32)>]) -> [u8; 64 * 1024] {
        let mut flash = [0xff_u8; 64 * 1024];
        for (index, entry) in pages.iter().enumerate() {
            let address = layout.log_page_address(index as u32).unwrap() as usize;
            let (flight, sequence) = entry.unwrap_or((9, 9));
            let mut page = encode_page(flight, sequence, &[record(0); RECORDS_PER_PAGE]).unwrap();
            if entry.is_none() {
                // The program stopped part-way: the rest is still erased.
                page[100..].fill(0xff);
            }
            flash[address..address + FLASH_PAGE_LEN].copy_from_slice(&page);
        }
        flash
    }

    fn scan_flash(layout: StorageLayout, flash: &[u8]) -> (u32, u32, bool) {
        scan_log(layout, |address, output: &mut [u8]| {
            let start = address as usize;
            output.copy_from_slice(&flash[start..start + output.len()]);
            Ok::<(), ()>(())
        })
        .unwrap()
    }

    fn tick(control_seq: u32) -> Stamped<ControlTick> {
        Stamped {
            seq: 7,
            at: TimestampMicros((1 << 32) + 1_234),
            value: ControlTick {
                logged: true,
                control_seq,
                imu_seq: 40,
                imu_fresh: true,
                armed: true,
                raw_gyro_dps: [1.0, 2.0, 3.0],
                filtered_gyro_dps: [0.5, -0.5, 1.5],
                command_dps: [10.0, -20.0, 30.0],
                pid: [0.1, 0.2, 0.3],
                throttle: 1_100.0,
                motors: [1_000.0, 1_100.0, 1_200.0, 1_300.0],
                ..ControlTick::default()
            },
        }
    }

    #[test]
    fn a_streamed_tick_becomes_the_record_the_control_loop_built() {
        let tick = tick(12);
        let compact = CompactRateBlackboxSample::from_fields(CompactRateBlackboxFields {
            seq: 12,
            imu_seq: 40,
            armed: true,
            imu_fresh: true,
            raw_gyro_dps: [1.0, 2.0, 3.0],
            filtered_gyro_dps: [0.5, -0.5, 1.5],
            command_dps: [10.0, -20.0, 30.0],
            pid: [0.1, 0.2, 0.3],
            throttle: 1_100.0,
            motors: [1_000.0, 1_100.0, 1_200.0, 1_300.0],
        });

        let record = record_for_tick(&tick, 1).unwrap();

        assert_eq!(record.timestamp_us, 1_234);
        assert_eq!(record.control_sequence, 12);
        assert_eq!(record.imu_sequence, 40);
        assert_eq!(record.flags, u16::from(compact.flags));
        assert_eq!(record.raw_gyro_dps10, compact.raw_gyro_dps10);
        assert_eq!(record.filtered_gyro_dps10, compact.gyro_dps10);
        assert_eq!(record.command_dps10, compact.command_dps10);
        assert_eq!(record.pid, compact.pid);
        assert_eq!(record.throttle, compact.throttle);
        assert_eq!(record.motors, compact.motors);
    }

    #[test]
    fn the_divisor_keeps_ticks_whose_sequence_it_divides() {
        assert!(record_for_tick(&tick(10), 2).is_some());
        assert!(record_for_tick(&tick(11), 2).is_none());
        assert!(record_for_tick(&tick(11), 0).is_some());
        assert!(record_for_tick(&tick(32), 99).is_some());
        assert!(record_for_tick(&tick(40), 99).is_none());
    }

    #[test]
    fn a_log_erase_stopped_part_way_scans_back_to_the_pages_it_left() {
        use ferrowasp_protocol::blackbox::{RECORDS_PER_PAGE, encode_page};

        let layout = StorageLayout::new(64 * 1024).unwrap();
        let mut flash = [0xff_u8; 64 * 1024];
        // Flights 1 and 2, twenty pages each, across the first three
        // sixteen-page sectors.
        for page_index in 0..40 {
            let flight = page_index / 20 + 1;
            let page =
                encode_page(flight, page_index % 20, &[record(0); RECORDS_PER_PAGE]).unwrap();
            let address = layout.log_page_address(page_index).unwrap() as usize;
            flash[address..address + FLASH_PAGE_LEN].copy_from_slice(&page);
        }
        let scan = |flash: &[u8]| {
            scan_log(layout, |address, output: &mut [u8]| {
                let start = address as usize;
                output.copy_from_slice(&flash[start..start + output.len()]);
                Ok::<(), ()>(())
            })
        };
        assert_eq!(scan(&flash), Ok((40, 3, true)));

        // Arming stopped the erase after it cleared the second sector, which
        // held the end of flight 1 and the start of flight 2.
        let mut step = 0;
        while let Some(address) = layout.log_erase_sector_address(step) {
            let start = address as usize;
            flash[start..start + CONFIG_SECTOR_SIZE as usize].fill(0xff);
            step += 1;
            if address == layout.log_start_address + CONFIG_SECTOR_SIZE {
                break;
            }
        }
        // Only flight 1's first sixteen pages are left; the append point and
        // the next flight move down to them.
        assert_eq!(scan(&flash), Ok((16, 2, true)));
    }

    #[test]
    fn log_erase_clears_the_ledger_first_then_walks_down_to_the_log_start() {
        let layout = StorageLayout::new(64 * 1024).unwrap();
        let steps = layout.log_sector_count();
        assert_eq!(
            layout.log_erase_sector_address(0),
            Some(layout.sync_ledger_address())
        );
        assert_eq!(
            layout.log_erase_sector_address(steps - 1),
            Some(layout.log_start_address)
        );
        assert_eq!(layout.log_erase_sector_address(steps), None);
    }

    #[test]
    fn a_torn_page_ends_its_flight_and_logging_carries_on_after_it() {
        let layout = StorageLayout::new(64 * 1024).unwrap();
        let torn = [Some((1, 0)), Some((1, 1)), Some((1, 2)), None];
        assert_eq!(scan_flash(layout, &log_with(layout, &torn)), (4, 2, true));

        // The next flight, written after the torn page. The old scan stopped
        // at the torn page here, found data after it, and stopped logging.
        let mut later = torn.to_vec();
        later.extend([Some((2, 0)), Some((2, 1))]);
        assert_eq!(scan_flash(layout, &log_with(layout, &later)), (6, 3, true));
    }

    #[test]
    fn the_scan_tells_an_empty_full_or_foreign_region_apart() {
        let layout = StorageLayout::new(64 * 1024).unwrap();
        assert_eq!(scan_flash(layout, &log_with(layout, &[])), (0, 1, true));

        let mut foreign = [0xff_u8; 64 * 1024];
        let start = layout.log_start_address as usize;
        foreign[start..start + 3000].fill(0x5a);
        assert_eq!(scan_flash(layout, &foreign), (0, 1, false));

        // Two pages that do not decode in a row are not one torn page.
        let garbled = [Some((1, 0)), None, None];
        assert_eq!(
            scan_flash(layout, &log_with(layout, &garbled)),
            (0, 1, false)
        );

        let full: heapless::Vec<_, 256> = (0..layout.log_page_count)
            .map(|sequence| Some((4, sequence)))
            .collect();
        assert_eq!(
            scan_flash(layout, &log_with(layout, &full)),
            (layout.log_page_count, 5, false)
        );
    }

    #[test]
    fn assembler_emits_every_five_records_and_flushes_partial_page() {
        let mut assembler = PageAssembler::new();
        assembler.start(12, false);
        for sequence in 0..4 {
            assert_eq!(assembler.push(record(sequence)), Ok(false));
        }
        assert_eq!(assembler.push(record(4)), Ok(true));
        let full = assembler.take_ready_page().unwrap();
        assert_eq!(decode_page(&full).unwrap().record_count, 5);
        assert_eq!(record_from_page(&full, 4).unwrap().control_sequence, 4);

        assembler.push(record(5)).unwrap();
        assert!(assembler.stop().unwrap());
        let partial = assembler.take_ready_page().unwrap();
        assert_eq!(decode_page(&partial).unwrap().record_count, 1);
    }

    #[test]
    fn assembler_marks_only_first_record_of_first_flight_after_boot() {
        let mut assembler = PageAssembler::new();
        assembler.start(12, true);
        for sequence in 0..5 {
            assembler.push(record(sequence)).unwrap();
        }
        let first_page = assembler.take_ready_page().unwrap();
        assert_ne!(
            record_from_page(&first_page, 0).unwrap().flags & FLIGHT_RECORD_FLAG_BOOT_SESSION_START,
            0
        );
        assert_eq!(
            record_from_page(&first_page, 1).unwrap().flags & FLIGHT_RECORD_FLAG_BOOT_SESSION_START,
            0
        );
        assembler.stop().unwrap();

        assembler.start(13, false);
        assembler.push(record(5)).unwrap();
        assembler.stop().unwrap();
        let second_flight = assembler.take_ready_page().unwrap();
        assert_eq!(
            record_from_page(&second_flight, 0).unwrap().flags
                & FLIGHT_RECORD_FLAG_BOOT_SESSION_START,
            0
        );
    }

    #[test]
    fn a_flight_is_unfinished_until_its_last_page_is_written() {
        let mut assembler = PageAssembler::new();
        assert_eq!(assembler.unfinished_flight(false), None);
        assembler.start(4, false);
        assert_eq!(assembler.unfinished_flight(false), Some(4));
        assembler.push(record(1)).unwrap();
        assembler.stop().unwrap();
        assert_eq!(assembler.unfinished_flight(false), Some(4));
        assembler.take_ready_page().unwrap();
        assert_eq!(assembler.unfinished_flight(true), Some(4));
        assert_eq!(assembler.unfinished_flight(false), None);
    }
}
