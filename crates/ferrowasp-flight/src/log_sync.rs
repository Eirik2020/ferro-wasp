//! Log sync: a host downloads every flight it has not stored, acknowledges
//! each one, and only then may the log be erased.
//!
//! The log holds flights back to back, each a run of pages carrying its
//! flight identifier, in increasing order. This module finds flights by
//! binary search over those identifiers and asks the sync ledger
//! ([`crate::sync_ledger`]) which ones a host has acknowledged. The command
//! line answers with the lines formatted here, each within one response
//! frame.

use crate::flash_storage::{StorageLayout, USB_RESPONSE_CAPACITY, page_flight_id};
use crate::sync_ledger::{self, LedgerError};
use core::fmt::Write as _;
use heapless::{String, Vec};

/// Flights one `logs unsynced` answer lists; the host acknowledges them and
/// asks again for the rest.
pub const MAX_LISTED_FLIGHTS: usize = 8;

/// One flight's run of log pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredFlight {
    pub flight_id: u32,
    pub start_page: u32,
    pub pages: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogSyncError<E> {
    Device(E),
    /// A page below the append point did not decode.
    Corrupt,
}

/// Why an acknowledgement was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AckRefusal {
    NoSuchFlight,
    /// The flight is still being written.
    Recording,
    /// The host's length differs from the flight on the device: it read an
    /// older copy, or the last page landed after it read.
    LengthChanged,
    /// The identifier is past what the ledger can record; erase the log.
    LedgerFull,
}

impl AckRefusal {
    pub const fn response(self) -> &'static str {
        match self {
            Self::NoSuchFlight => "ERR no such flight\r\n",
            Self::Recording => "ERR flight still recording\r\n",
            Self::LengthChanged => "ERR flight length changed; download it again\r\n",
            Self::LedgerFull => "ERR sync ledger full; erase the log over USB\r\n",
        }
    }
}

/// The flight log page `page_index` belongs to; a torn last page belongs
/// to the flight before it ([`page_flight_id`]).
fn flight_id_at<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    page_index: u32,
) -> Result<u32, LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let flight_id = page_flight_id(page_index, |index, page| {
        let address = layout
            .log_page_address(index)
            .ok_or(LogSyncError::Corrupt)?;
        read(address, page).map_err(LogSyncError::Device)
    })?;
    flight_id.ok_or(LogSyncError::Corrupt)
}

/// The first page in `0..end` whose flight identifier is at least `flight_id`.
fn lower_bound<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    end: u32,
    flight_id: u32,
) -> Result<u32, LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let (mut low, mut high) = (0, end);
    while low < high {
        let middle = low + (high - low) / 2;
        if flight_id_at(layout, read, middle)? < flight_id {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    Ok(low)
}

/// Where `flight_id` lies in the first `used_pages` log pages, if anywhere.
pub fn find_flight<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    used_pages: u32,
    flight_id: u32,
) -> Result<Option<StoredFlight>, LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let start_page = lower_bound(layout, read, used_pages, flight_id)?;
    if start_page == used_pages || flight_id_at(layout, read, start_page)? != flight_id {
        return Ok(None);
    }
    let end = match flight_id.checked_add(1) {
        Some(next) => lower_bound(layout, read, used_pages, next)?,
        None => used_pages,
    };
    Ok(Some(StoredFlight {
        flight_id,
        start_page,
        pages: end - start_page,
    }))
}

fn is_synced<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    flight_id: u32,
) -> Result<bool, LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    match sync_ledger::is_synced(layout, read, flight_id) {
        Ok(synced) => Ok(synced),
        // Past the ledger's range: never acknowledged, so always offered.
        Err(LedgerError::OutOfRange) => Ok(false),
        Err(LedgerError::Device(error)) => Err(LogSyncError::Device(error)),
    }
}

/// Walks the stored flights from the newest down, calling `visit` with each
/// and whether a host acknowledged it, until `visit` returns `false`.
fn walk_flights<E, Read, Visit>(
    layout: StorageLayout,
    read: &mut Read,
    used_pages: u32,
    mut visit: Visit,
) -> Result<(), LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
    Visit: FnMut(StoredFlight, bool) -> bool,
{
    let mut end = used_pages;
    while end != 0 {
        let flight_id = flight_id_at(layout, read, end - 1)?;
        let start_page = lower_bound(layout, read, end, flight_id)?;
        let flight = StoredFlight {
            flight_id,
            start_page,
            pages: end - start_page,
        };
        if !visit(flight, is_synced(layout, read, flight_id)?) {
            break;
        }
        end = start_page;
    }
    Ok(())
}

/// Up to [`MAX_LISTED_FLIGHTS`] complete flights no host has acknowledged,
/// newest first, and whether more remain. `recording` is left out: it is
/// still growing.
pub fn unsynced_flights<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    used_pages: u32,
    recording: Option<u32>,
) -> Result<(Vec<StoredFlight, MAX_LISTED_FLIGHTS>, bool), LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    let mut listed = Vec::new();
    let mut more = false;
    walk_flights(layout, read, used_pages, |flight, synced| {
        if synced || recording == Some(flight.flight_id) {
            return true;
        }
        if listed.push(flight).is_err() {
            more = true;
            return false;
        }
        true
    })?;
    Ok((listed, more))
}

/// Whether erasing the log would lose nothing a host has not stored: no
/// flight is recording and every flight is acknowledged.
pub fn all_synced<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    used_pages: u32,
    recording: Option<u32>,
) -> Result<bool, LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    if recording.is_some() {
        return Ok(false);
    }
    let mut all = true;
    walk_flights(layout, read, used_pages, |_, synced| {
        all &= synced;
        synced
    })?;
    Ok(all)
}

/// Checks a host's acknowledgement of `flight_id`, `pages` long, before the
/// ledger records it.
pub fn check_ack<E, Read>(
    layout: StorageLayout,
    read: &mut Read,
    used_pages: u32,
    recording: Option<u32>,
    flight_id: u32,
    pages: u32,
) -> Result<Result<(), AckRefusal>, LogSyncError<E>>
where
    Read: FnMut(u32, &mut [u8]) -> Result<(), E>,
{
    if recording == Some(flight_id) {
        return Ok(Err(AckRefusal::Recording));
    }
    if sync_ledger::bit_location(layout, flight_id).is_none() {
        return Ok(Err(if flight_id == 0 {
            AckRefusal::NoSuchFlight
        } else {
            AckRefusal::LedgerFull
        }));
    }
    Ok(match find_flight(layout, read, used_pages, flight_id)? {
        None => Err(AckRefusal::NoSuchFlight),
        Some(flight) if flight.pages != pages => Err(AckRefusal::LengthChanged),
        Some(_) => Ok(()),
    })
}

/// The `logs unsynced` answer: a count line, then one line per flight.
///
/// ```text
/// OK unsynced n=2 more=0
/// OK flight=7 start=120 pages=412
/// OK flight=5 start=40 pages=80
/// ```
pub fn emit_unsynced_lines<Emit>(flights: &[StoredFlight], more: bool, mut emit: Emit) -> bool
where
    Emit: FnMut(&str) -> bool,
{
    let mut line = String::<USB_RESPONSE_CAPACITY>::new();
    if write!(
        line,
        "OK unsynced n={} more={}\r\n",
        flights.len(),
        u8::from(more)
    )
    .is_err()
        || !emit(line.as_str())
    {
        return false;
    }
    for flight in flights {
        line.clear();
        if write!(
            line,
            "OK flight={} start={} pages={}\r\n",
            flight.flight_id, flight.start_page, flight.pages
        )
        .is_err()
            || !emit(line.as_str())
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::sync_ledger::{MarkStep, SYNC_LEDGER_MAGIC, next_mark_step};
    use ferrowasp_core::blackbox::{FLASH_PAGE_LEN, FlightRecord, RECORDS_PER_PAGE, encode_page};
    use std::vec::Vec as StdVec;

    const CAPACITY: u32 = 64 * 1024;

    struct Nor(StdVec<u8>);

    impl Nor {
        /// A log holding `pages_per_flight[i]` pages of flight `i + 1`.
        fn with_flights(layout: StorageLayout, pages_per_flight: &[u32]) -> (Self, u32) {
            let mut nor = Self(std::vec![0xff; CAPACITY as usize]);
            let mut page_index = 0;
            for (flight, pages) in pages_per_flight.iter().enumerate() {
                for sequence in 0..*pages {
                    let records = [FlightRecord::default(); RECORDS_PER_PAGE];
                    let page = encode_page(flight as u32 + 1, sequence, &records).unwrap();
                    let address = layout.log_page_address(page_index).unwrap() as usize;
                    nor.0[address..address + FLASH_PAGE_LEN].copy_from_slice(&page);
                    page_index += 1;
                }
            }
            (nor, page_index)
        }

        fn reader(&self) -> impl FnMut(u32, &mut [u8]) -> Result<(), ()> + '_ {
            |address, output| {
                let start = address as usize;
                output.copy_from_slice(&self.0[start..start + output.len()]);
                Ok(())
            }
        }

        fn mark(&mut self, layout: StorageLayout, flight_id: u32) {
            loop {
                let step = next_mark_step(layout, &mut self.reader(), flight_id).unwrap();
                match step {
                    MarkStep::EraseLedger { .. } => unreachable!("fresh ledger"),
                    MarkStep::ProgramHeader { address } => {
                        let start = address as usize;
                        self.0[start..start + 8].copy_from_slice(&SYNC_LEDGER_MAGIC);
                    }
                    MarkStep::ProgramBit { address, value } => self.0[address as usize] &= value,
                    MarkStep::Done => return,
                }
            }
        }
    }

    fn ids(flights: &[StoredFlight]) -> StdVec<u32> {
        flights.iter().map(|flight| flight.flight_id).collect()
    }

    #[test]
    fn flights_are_found_by_identifier_with_their_page_runs() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let (nor, used) = Nor::with_flights(layout, &[3, 1, 5]);
        let mut read = nor.reader();
        assert_eq!(
            find_flight(layout, &mut read, used, 2),
            Ok(Some(StoredFlight {
                flight_id: 2,
                start_page: 3,
                pages: 1
            }))
        );
        assert_eq!(
            find_flight(layout, &mut read, used, 3)
                .unwrap()
                .unwrap()
                .pages,
            5
        );
        assert_eq!(find_flight(layout, &mut read, used, 4), Ok(None));
        assert_eq!(find_flight(layout, &mut read, 0, 1), Ok(None));
    }

    #[test]
    fn unsynced_flights_leave_out_acknowledged_and_recording_ones() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let (mut nor, used) = Nor::with_flights(layout, &[2, 3, 1, 4]);
        nor.mark(layout, 2);

        let (listed, more) = unsynced_flights(layout, &mut nor.reader(), used, Some(4)).unwrap();
        assert_eq!(ids(&listed), [3, 1]);
        assert!(!more);
        assert_eq!(
            listed[0],
            StoredFlight {
                flight_id: 3,
                start_page: 5,
                pages: 1
            }
        );
        assert!(!all_synced(layout, &mut nor.reader(), used, None).unwrap());
    }

    #[test]
    fn a_long_backlog_is_listed_in_rounds() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let (mut nor, used) = Nor::with_flights(layout, &[1; MAX_LISTED_FLIGHTS + 2]);
        let (first, more) = unsynced_flights(layout, &mut nor.reader(), used, None).unwrap();
        assert_eq!(first.len(), MAX_LISTED_FLIGHTS);
        assert_eq!(first[0].flight_id, MAX_LISTED_FLIGHTS as u32 + 2);
        assert!(more);
        for flight in &first {
            nor.mark(layout, flight.flight_id);
        }
        let (rest, more) = unsynced_flights(layout, &mut nor.reader(), used, None).unwrap();
        assert_eq!(ids(&rest), [2, 1]);
        assert!(!more);
    }

    #[test]
    fn erase_waits_until_every_flight_is_acknowledged() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let (mut nor, used) = Nor::with_flights(layout, &[2, 2]);
        assert!(all_synced(layout, &mut nor.reader(), 0, None).unwrap());
        nor.mark(layout, 2);
        assert!(!all_synced(layout, &mut nor.reader(), used, None).unwrap());
        nor.mark(layout, 1);
        assert!(all_synced(layout, &mut nor.reader(), used, None).unwrap());
        assert!(!all_synced(layout, &mut nor.reader(), used, Some(3)).unwrap());
    }

    #[test]
    fn a_torn_page_counts_as_the_last_page_of_its_flight() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let (mut nor, used) = Nor::with_flights(layout, &[3, 2]);
        // Tear flight 1's last page: power was cut while it was written.
        let torn = layout.log_page_address(2).unwrap() as usize;
        nor.0[torn + 100..torn + FLASH_PAGE_LEN].fill(0xff);

        let (listed, _) = unsynced_flights(layout, &mut nor.reader(), used, None).unwrap();
        assert_eq!(
            listed.as_slice(),
            [
                StoredFlight {
                    flight_id: 2,
                    start_page: 3,
                    pages: 2
                },
                StoredFlight {
                    flight_id: 1,
                    start_page: 0,
                    pages: 3
                }
            ]
        );
        assert_eq!(
            check_ack(layout, &mut nor.reader(), used, None, 1, 3).unwrap(),
            Ok(())
        );
    }

    #[test]
    fn an_acknowledgement_must_name_a_complete_flight_at_its_length() {
        let layout = StorageLayout::new(CAPACITY).unwrap();
        let (nor, used) = Nor::with_flights(layout, &[2, 3]);
        let mut read = nor.reader();
        let mut check = |recording, flight, pages| {
            check_ack(layout, &mut read, used, recording, flight, pages).unwrap()
        };
        assert_eq!(check(None, 2, 3), Ok(()));
        assert_eq!(check(None, 2, 2), Err(AckRefusal::LengthChanged));
        assert_eq!(check(None, 3, 1), Err(AckRefusal::NoSuchFlight));
        assert_eq!(check(None, 0, 1), Err(AckRefusal::NoSuchFlight));
        assert_eq!(check(Some(2), 2, 3), Err(AckRefusal::Recording));
        assert_eq!(
            check(None, sync_ledger::SYNC_LEDGER_CAPACITY + 1, 1),
            Err(AckRefusal::LedgerFull)
        );
    }

    #[test]
    fn every_answer_line_fits_one_response_frame() {
        let widest = StoredFlight {
            flight_id: u32::MAX,
            start_page: u32::MAX,
            pages: u32::MAX,
        };
        let mut lines = StdVec::new();
        assert!(emit_unsynced_lines(
            &[widest; MAX_LISTED_FLIGHTS],
            true,
            |line| {
                lines.push(std::string::String::from(line));
                true
            }
        ));
        assert_eq!(lines.len(), MAX_LISTED_FLIGHTS + 1);
        assert_eq!(lines[0], "OK unsynced n=8 more=1\r\n");
        assert!(lines.iter().all(|line| line.len() <= USB_RESPONSE_CAPACITY));
        for refusal in [
            AckRefusal::NoSuchFlight,
            AckRefusal::Recording,
            AckRefusal::LengthChanged,
            AckRefusal::LedgerFull,
        ] {
            assert!(refusal.response().len() <= USB_RESPONSE_CAPACITY);
        }
    }
}
