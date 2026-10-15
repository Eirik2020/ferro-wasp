//! Writing the flight log: records into pages, pages onto flash in order, one
//! flight per arming.
//!
//! The storage owner calls it each pass: [`LogWriter::flush_write_back`] at
//! the top, then [`LogWriter::write_pending`], then [`LogWriter::drain`]. It
//! owns where the log ends, which flight is next, and the one page waiting to
//! be written; the owner reports what each call says happened.

use ferrowasp_core::blackbox::FLASH_PAGE_LEN;
use ferrowasp_drivers::nor_storage::NorStorage;

use crate::flash_storage::{PageAssembler, RecordConsumer, StorageLayout};

/// Records taken from the queue in one pass.
///
/// The owner's loop yields for a millisecond each pass, so one record per
/// pass capped the store near a thousand records a second however fast the
/// flash is. Bounded so a full queue cannot starve the rest of the system.
pub const RECORD_DRAIN_PER_PASS: u32 = 16;

/// How long log pages may wait in a store's write-back buffer before they are
/// written out; at most this much log is lost to a power cut.
pub const WRITE_BACK_MAX_AGE_MS: u64 = 100;

/// What [`LogWriter::write_pending`] did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageWrite {
    /// No page was waiting.
    Nothing,
    Written,
    /// The log region is full; recording has to stop.
    RegionFull,
    /// The page did not program; it stays waiting.
    ProgramFailed,
}

/// What one [`LogWriter::drain`] did, for the owner's counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrainReport {
    pub pages_written: u32,
    pub records_dropped: u32,
    pub write_faults: u32,
}

pub struct LogWriter {
    assembler: PageAssembler,
    pending: Option<[u8; FLASH_PAGE_LEN]>,
    next_page: u32,
    next_flight: u32,
    writable: bool,
    boot_session_start: bool,
    write_back_since_ms: Option<u64>,
}

impl Default for LogWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl LogWriter {
    /// Not writable until storage is recovered (`recover`).
    pub const fn new() -> Self {
        Self {
            assembler: PageAssembler::new(),
            pending: None,
            next_page: 0,
            next_flight: 1,
            writable: false,
            boot_session_start: true,
            write_back_since_ms: None,
        }
    }

    pub const fn next_page(&self) -> u32 {
        self.next_page
    }

    pub const fn next_flight(&self) -> u32 {
        self.next_flight
    }

    pub const fn writable(&self) -> bool {
        self.writable
    }

    /// Whether a flight is being recorded.
    pub const fn recording(&self) -> bool {
        self.assembler.recording()
    }

    /// The flight still being recorded or written out, if any.
    pub const fn unfinished_flight(&self) -> Option<u32> {
        self.assembler.unfinished_flight(self.pending.is_some())
    }

    /// Where a scan of the flash found the log's end.
    pub fn recover(&mut self, next_page: u32, next_flight: u32, writable: bool) {
        self.next_page = next_page;
        self.next_flight = next_flight;
        self.writable = writable;
    }

    /// After a log erase: an empty, writable log starting at flight 1.
    pub fn reset_after_erase(&mut self) {
        self.recover(0, 1, true);
    }

    /// Writes out pages a store holds in RAM once they have waited
    /// [`WRITE_BACK_MAX_AGE_MS`], or at once when no flight is recording.
    pub fn flush_write_back<F: NorStorage>(
        &mut self,
        flash: &mut F,
        now_ms: u64,
    ) -> Result<(), F::Error> {
        if !flash.write_back_pending() {
            self.write_back_since_ms = None;
            return Ok(());
        }
        let since_ms = *self.write_back_since_ms.get_or_insert(now_ms);
        if !self.assembler.recording() || now_ms.saturating_sub(since_ms) >= WRITE_BACK_MAX_AGE_MS {
            self.write_back_since_ms = None;
            flash.flush()?;
        }
        Ok(())
    }

    /// Writes the page waiting, taking a finished one from the assembler
    /// when none is.
    pub fn write_pending<F: NorStorage>(
        &mut self,
        flash: &mut F,
        layout: StorageLayout,
    ) -> PageWrite {
        if self.writable && self.pending.is_none() {
            self.pending = self.assembler.take_ready_page();
        }
        let Some(page) = self.pending.as_ref() else {
            return PageWrite::Nothing;
        };
        let Some(address) = layout.log_page_address(self.next_page) else {
            return PageWrite::RegionFull;
        };
        if flash.page_program(address, page).is_err() {
            return PageWrite::ProgramFailed;
        }
        self.pending = None;
        self.next_page = self.next_page.saturating_add(1);
        PageWrite::Written
    }

    /// Takes up to [`RECORD_DRAIN_PER_PASS`] records. An armed record starts
    /// a flight if none is recording; a disarmed one ends it. With the queue
    /// empty and the craft disarmed, a flight still recording ends too.
    ///
    /// `record_while_disarmed` is the bench image's: every record goes to the
    /// log, and an empty queue never ends the flight, which would split one
    /// bench run into hundreds of tiny flights.
    pub fn drain<F: NorStorage>(
        &mut self,
        flash: &mut F,
        layout: StorageLayout,
        records: &mut RecordConsumer,
        armed: bool,
        record_while_disarmed: bool,
    ) -> DrainReport {
        let mut report = DrainReport::default();
        if !self.writable {
            if records.dequeue().is_some() {
                report.records_dropped += 1;
            }
            return report;
        }
        if records.peek().is_none() {
            if !record_while_disarmed
                && !armed
                && self.assembler.recording()
                && self.assembler.stop().is_err()
            {
                report.write_faults += 1;
            }
            return report;
        }
        let mut drained = 0;
        while drained < RECORD_DRAIN_PER_PASS {
            // Write a finished page before taking another record: the
            // assembler holds one page, so a record taken while it is full
            // is dropped. A store that programs in the background takes the
            // next page once idle, the records waiting queued meanwhile; a
            // failed write stays pending for `write_pending` to report.
            if self.pending.is_none() {
                self.pending = self.assembler.take_ready_page();
            }
            if let Some(page) = self.pending.as_ref() {
                if flash.busy().unwrap_or(true) {
                    break;
                }
                let Some(address) = layout.log_page_address(self.next_page) else {
                    break;
                };
                if flash.page_program(address, page).is_err() {
                    break;
                }
                self.pending = None;
                self.next_page = self.next_page.saturating_add(1);
                report.pages_written += 1;
            }
            let Some(record) = records.dequeue() else {
                break;
            };
            drained += 1;
            // A bench capture's records keep their honest armed flag.
            let logged = record_while_disarmed || record.flags & 1 != 0;
            if logged && !self.assembler.recording() {
                self.assembler
                    .start(self.next_flight, self.boot_session_start);
                self.boot_session_start = false;
                self.next_flight = self.next_flight.wrapping_add(1).max(1);
            }
            if logged {
                if self.assembler.push(record).is_err() {
                    report.records_dropped += 1;
                }
            } else if self.assembler.recording() && self.assembler.stop().is_err() {
                report.write_faults += 1;
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::flash_storage::RecordQueue;
    use crate::test_nor::MemoryNor;
    use ferrowasp_core::blackbox::{
        FLIGHT_RECORD_FLAG_BOOT_SESSION_START, FlightRecord, decode_page, record_from_page,
    };
    use std::boxed::Box;
    use std::vec::Vec;

    const CAPACITY: u32 = 64 * 1024;

    fn layout() -> StorageLayout {
        StorageLayout::new(CAPACITY).unwrap()
    }

    fn record(sequence: u32, armed: bool) -> FlightRecord {
        FlightRecord {
            control_sequence: sequence,
            flags: u16::from(armed),
            ..FlightRecord::default()
        }
    }

    struct Rig {
        log: LogWriter,
        nor: MemoryNor,
        input: crate::flash_storage::RecordProducer,
        output: RecordConsumer,
    }

    impl Rig {
        fn new() -> Self {
            let (input, output) = Box::leak(Box::new(RecordQueue::<64>::new())).split();
            let mut log = LogWriter::new();
            log.recover(0, 1, true);
            Self {
                log,
                nor: MemoryNor::erased(CAPACITY),
                input,
                output,
            }
        }

        /// Queues records, then runs passes as the owner does until the
        /// queue is empty and nothing is left to write.
        fn fly(&mut self, records: &[FlightRecord], armed_after: bool) -> DrainReport {
            let mut total = DrainReport::default();
            for chunk in records.chunks(32) {
                for value in chunk {
                    self.input.enqueue(*value).unwrap();
                }
                for _ in 0..8 {
                    self.pass(armed_after, &mut total);
                }
            }
            for _ in 0..8 {
                self.pass(armed_after, &mut total);
            }
            total
        }

        fn pass(&mut self, armed: bool, total: &mut DrainReport) {
            if self.log.write_pending(&mut self.nor, layout()) == PageWrite::Written {
                total.pages_written += 1;
            }
            let report = self
                .log
                .drain(&mut self.nor, layout(), &mut self.output, armed, false);
            total.pages_written += report.pages_written;
            total.records_dropped += report.records_dropped;
            total.write_faults += report.write_faults;
        }

        /// Every logged page: flight, sequence, and its records.
        fn pages(&mut self) -> Vec<(u32, u32, Vec<FlightRecord>)> {
            (0..self.log.next_page())
                .map(|index| {
                    let mut page = [0u8; FLASH_PAGE_LEN];
                    self.nor
                        .read(layout().log_page_address(index).unwrap(), &mut page)
                        .unwrap();
                    let metadata = decode_page(&page).unwrap();
                    let records = (0..usize::from(metadata.record_count))
                        .map(|i| record_from_page(&page, i).unwrap())
                        .collect();
                    (metadata.flight_id, metadata.page_sequence, records)
                })
                .collect()
        }
    }

    #[test]
    fn an_arming_is_one_flight_of_pages_in_order_and_disarming_closes_it() {
        let mut rig = Rig::new();
        let armed: Vec<_> = (0..12).map(|seq| record(seq, true)).collect();
        rig.fly(&armed, true);
        assert_eq!(rig.log.unfinished_flight(), Some(1));
        let report = rig.fly(&[record(12, false)], false);
        assert_eq!(report.records_dropped, 0);
        assert_eq!(rig.log.unfinished_flight(), None);

        let pages = rig.pages();
        assert!(pages.iter().all(|(flight, _, _)| *flight == 1));
        let sequences: Vec<_> = pages.iter().map(|(_, sequence, _)| *sequence).collect();
        assert_eq!(sequences, (0..pages.len() as u32).collect::<Vec<_>>());
        let logged: Vec<_> = pages.iter().flat_map(|(_, _, records)| records).collect();
        assert_eq!(logged.len(), 12, "the disarmed record is not logged");
        assert_ne!(logged[0].flags & FLIGHT_RECORD_FLAG_BOOT_SESSION_START, 0);
        assert!(
            logged[1..]
                .iter()
                .all(|r| r.flags & FLIGHT_RECORD_FLAG_BOOT_SESSION_START == 0)
        );
        assert_eq!(rig.log.next_flight(), 2);
    }

    #[test]
    fn the_next_arming_is_the_next_flight_without_a_boot_mark() {
        let mut rig = Rig::new();
        rig.fly(&[record(0, true), record(1, true)], true);
        rig.fly(&[record(2, false)], false);
        rig.fly(&[record(3, true), record(4, true)], true);
        rig.fly(&[record(5, false)], false);
        let pages = rig.pages();
        let flights: Vec<_> = pages.iter().map(|(flight, _, _)| *flight).collect();
        assert_eq!(flights, [1, 2]);
        assert_eq!(
            pages[1].2[0].flags & FLIGHT_RECORD_FLAG_BOOT_SESSION_START,
            0
        );
    }

    #[test]
    fn a_log_that_is_not_writable_drops_one_record_a_pass() {
        let mut rig = Rig::new();
        rig.log.recover(0, 1, false);
        rig.input.enqueue(record(0, true)).unwrap();
        rig.input.enqueue(record(1, true)).unwrap();
        let report = rig
            .log
            .drain(&mut rig.nor, layout(), &mut rig.output, true, false);
        assert_eq!(report.records_dropped, 1);
        assert_eq!(rig.log.next_page(), 0);
    }

    #[test]
    fn a_full_region_and_a_failed_program_are_reported_and_keep_the_page() {
        let mut rig = Rig::new();
        rig.fly(
            &(0..6).map(|seq| record(seq, true)).collect::<Vec<_>>(),
            true,
        );
        rig.log.recover(layout().log_page_count, 2, true);
        rig.fly(
            &(6..12).map(|seq| record(seq, true)).collect::<Vec<_>>(),
            true,
        );
        assert_eq!(
            rig.log.write_pending(&mut rig.nor, layout()),
            PageWrite::RegionFull
        );

        let mut rig = Rig::new();
        rig.nor.fail_program = true;
        rig.fly(
            &(0..6).map(|seq| record(seq, true)).collect::<Vec<_>>(),
            true,
        );
        assert_eq!(
            rig.log.write_pending(&mut rig.nor, layout()),
            PageWrite::ProgramFailed
        );
        assert_eq!(rig.log.next_page(), 0);
        rig.nor.fail_program = false;
        assert_eq!(
            rig.log.write_pending(&mut rig.nor, layout()),
            PageWrite::Written
        );
    }

    #[test]
    fn written_back_pages_wait_while_recording_and_flush_when_old_or_stopped() {
        let mut rig = Rig::new();
        rig.nor.write_back = true;
        rig.fly(
            &(0..6).map(|seq| record(seq, true)).collect::<Vec<_>>(),
            true,
        );
        assert!(rig.nor.held);
        rig.log.flush_write_back(&mut rig.nor, 1_000).unwrap();
        rig.log.flush_write_back(&mut rig.nor, 1_099).unwrap();
        assert_eq!(rig.nor.flushes, 0, "recording, and younger than 100 ms");
        rig.log.flush_write_back(&mut rig.nor, 1_100).unwrap();
        assert_eq!(rig.nor.flushes, 1);

        rig.fly(&[record(6, true), record(7, false)], false);
        assert!(rig.nor.held);
        rig.log.flush_write_back(&mut rig.nor, 2_000).unwrap();
        assert_eq!(rig.nor.flushes, 2, "no flight recording: at once");
    }

    #[test]
    fn a_bench_capture_logs_disarmed_records_and_never_closes_on_an_empty_queue() {
        let mut rig = Rig::new();
        for seq in 0..3 {
            rig.input.enqueue(record(seq, false)).unwrap();
        }
        rig.log
            .drain(&mut rig.nor, layout(), &mut rig.output, false, true);
        assert_eq!(rig.log.unfinished_flight(), Some(1));
        rig.log
            .drain(&mut rig.nor, layout(), &mut rig.output, false, true);
        assert_eq!(rig.log.unfinished_flight(), Some(1), "still recording");
    }
}
