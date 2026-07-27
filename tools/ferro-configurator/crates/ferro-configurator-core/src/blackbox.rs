use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use ferrowasp_core::blackbox::{
    FLASH_PAGE_LEN, FLIGHT_RECORD_FLAG_BOOT_SESSION_START, decode_page, record_from_page,
};
use serde::Serialize;

use crate::{
    client::{FerroClient, LogInfo},
    error::{FerroError, Result},
    transport::LineTransport,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PageSummary {
    pub flight_id: u32,
    pub page_sequence: u32,
    pub record_count: u8,
    pub boot_session_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FlightSpan {
    pub flight_id: u32,
    pub start_page: u32,
    pub end_page: u32,
    pub boot_session_start: bool,
}

impl FlightSpan {
    pub const fn page_count(self) -> u32 {
        self.end_page - self.start_page
    }

    pub const fn byte_count(self) -> u64 {
        self.page_count() as u64 * FLASH_PAGE_LEN as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogEntry {
    pub boot_session: Option<u32>,
    pub flight: FlightSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlightCatalog {
    pub storage: LogInfo,
    pub flights: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightSelector {
    Latest,
    Id(u32),
}

impl FlightSelector {
    pub fn parse(value: &str) -> Result<Self> {
        if value.eq_ignore_ascii_case("latest") {
            return Ok(Self::Latest);
        }
        let id = value.parse::<u32>().map_err(|_| {
            FerroError::Blackbox(
                "flight must be an integer from 1 through 4294967295 or `latest`".to_owned(),
            )
        })?;
        if id == 0 {
            return Err(FerroError::Blackbox(
                "flight ID zero is not valid".to_owned(),
            ));
        }
        Ok(Self::Id(id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadSummary {
    pub flight_id: u32,
    pub pages: u32,
    pub bytes: u64,
    pub output: PathBuf,
    pub resumed_pages: u32,
}

pub fn summarize_page(page: &[u8; FLASH_PAGE_LEN], page_index: u32) -> Result<PageSummary> {
    let metadata = decode_page(page).map_err(|error| {
        FerroError::Blackbox(format!("invalid flash page {page_index}: {error:?}"))
    })?;
    let boot_session_start = if metadata.record_count == 0 {
        false
    } else {
        record_from_page(page, 0)
            .map_err(|error| {
                FerroError::Blackbox(format!(
                    "invalid first record at flash page {page_index}: {error:?}"
                ))
            })?
            .flags
            & FLIGHT_RECORD_FLAG_BOOT_SESSION_START
            != 0
    };
    Ok(PageSummary {
        flight_id: metadata.flight_id,
        page_sequence: metadata.page_sequence,
        record_count: metadata.record_count,
        boot_session_start,
    })
}

pub fn lower_bound_flight_id(
    page_count: u32,
    flight_id: u32,
    mut read: impl FnMut(u32) -> Result<[u8; FLASH_PAGE_LEN]>,
) -> Result<u32> {
    let mut low = 0;
    let mut high = page_count;
    while low < high {
        let middle = low + (high - low) / 2;
        let metadata = summarize_page(&read(middle)?, middle)?;
        if metadata.flight_id < flight_id {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    Ok(low)
}

pub fn resolve_flight_span(
    used_pages: u32,
    selector: FlightSelector,
    mut read: impl FnMut(u32) -> Result<[u8; FLASH_PAGE_LEN]>,
) -> Result<FlightSpan> {
    if used_pages == 0 {
        return Err(FerroError::Blackbox("no stored flights".to_owned()));
    }
    let selected_id = match selector {
        FlightSelector::Latest => summarize_page(&read(used_pages - 1)?, used_pages - 1)?.flight_id,
        FlightSelector::Id(id) => id,
    };
    let start = lower_bound_flight_id(used_pages, selected_id, &mut read)?;
    if start == used_pages {
        return Err(missing_flight(selected_id));
    }
    let first = summarize_page(&read(start)?, start)?;
    if first.flight_id != selected_id {
        return Err(missing_flight(selected_id));
    }
    let end = if selected_id == u32::MAX {
        used_pages
    } else {
        lower_bound_flight_id(used_pages, selected_id + 1, &mut read)?
    };
    Ok(FlightSpan {
        flight_id: selected_id,
        start_page: start,
        end_page: end,
        boot_session_start: first.boot_session_start,
    })
}

pub fn catalog_flights(
    used_pages: u32,
    mut read: impl FnMut(u32) -> Result<[u8; FLASH_PAGE_LEN]>,
) -> Result<Vec<FlightSpan>> {
    let mut spans = Vec::new();
    let mut cursor = used_pages;
    while cursor != 0 {
        let last = summarize_page(&read(cursor - 1)?, cursor - 1)?;
        let start = lower_bound_flight_id(cursor, last.flight_id, &mut read)?;
        let first = summarize_page(&read(start)?, start)?;
        spans.push(FlightSpan {
            flight_id: last.flight_id,
            start_page: start,
            end_page: cursor,
            boot_session_start: first.boot_session_start,
        });
        cursor = start;
    }
    spans.reverse();
    Ok(spans)
}

pub fn catalog_device<T: LineTransport>(client: &mut FerroClient<T>) -> Result<FlightCatalog> {
    let storage = client.log_info()?;
    let mut cache = BTreeMap::new();
    let spans = catalog_flights(storage.used_pages, |index| {
        cached_page(client, &mut cache, index)
    })?;
    let mut current_session = None;
    let mut next_session = 1;
    let flights = spans
        .into_iter()
        .map(|flight| {
            if flight.boot_session_start {
                current_session = Some(next_session);
                next_session += 1;
            }
            CatalogEntry {
                boot_session: current_session,
                flight,
            }
        })
        .collect();
    Ok(FlightCatalog { storage, flights })
}

pub fn resolve_device_flight<T: LineTransport>(
    client: &mut FerroClient<T>,
    selector: FlightSelector,
) -> Result<(LogInfo, FlightSpan)> {
    let storage = client.log_info()?;
    let mut cache = BTreeMap::new();
    let span = resolve_flight_span(storage.used_pages, selector, |index| {
        cached_page(client, &mut cache, index)
    })?;
    Ok((storage, span))
}

pub fn download_flight<T: LineTransport>(
    client: &mut FerroClient<T>,
    span: FlightSpan,
    output: &Path,
    resume: bool,
    mut progress: impl FnMut(u32, u32),
) -> Result<DownloadSummary> {
    if output.exists() {
        return Err(FerroError::FileIo {
            operation: "create",
            path: output.to_path_buf(),
            reason: "output already exists; choose another path".to_owned(),
        });
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("create directory for", output, error))?;
    }
    let partial = partial_path(output);
    let resumed_pages = if resume {
        validate_partial_download(&partial, span)?
    } else {
        if partial.exists() {
            return Err(FerroError::FileIo {
                operation: "create",
                path: partial,
                reason: "partial download already exists; pass --resume or remove it".to_owned(),
            });
        }
        0
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(resume)
        .truncate(!resume)
        .write(true)
        .open(&partial)
        .map_err(|error| file_error("open", &partial, error))?;

    for local_index in resumed_pages..span.page_count() {
        let device_index = span.start_page + local_index;
        let page = client.read_log_page(device_index)?;
        let summary = summarize_page(&page, device_index)?;
        if summary.flight_id != span.flight_id || summary.page_sequence != local_index {
            return Err(FerroError::Blackbox(format!(
                "flight changed or page order became inconsistent at device page {device_index}"
            )));
        }
        file.write_all(&page)
            .map_err(|error| file_error("write", &partial, error))?;
        progress(local_index + 1, span.page_count());
    }
    file.sync_all()
        .map_err(|error| file_error("flush", &partial, error))?;
    drop(file);
    fs::rename(&partial, output).map_err(|error| file_error("finalize", output, error))?;
    Ok(DownloadSummary {
        flight_id: span.flight_id,
        pages: span.page_count(),
        bytes: span.byte_count(),
        output: output.to_path_buf(),
        resumed_pages,
    })
}

pub fn validate_partial_download(path: &Path, span: FlightSpan) -> Result<u32> {
    if !path.exists() {
        return Ok(0);
    }
    let size = fs::metadata(path)
        .map_err(|error| file_error("inspect", path, error))?
        .len();
    if size % FLASH_PAGE_LEN as u64 != 0 {
        return Err(FerroError::Blackbox(format!(
            "cannot resume {}: {} trailing bytes",
            path.display(),
            size % FLASH_PAGE_LEN as u64
        )));
    }
    let page_count = u32::try_from(size / FLASH_PAGE_LEN as u64).map_err(|_| {
        FerroError::Blackbox(format!("partial file {} is too large", path.display()))
    })?;
    if page_count > span.page_count() {
        return Err(FerroError::Blackbox(format!(
            "cannot resume: partial file has {page_count} pages but flight {} has {}",
            span.flight_id,
            span.page_count()
        )));
    }
    let mut file = File::open(path).map_err(|error| file_error("open", path, error))?;
    for local_index in 0..page_count {
        let mut page = [0u8; FLASH_PAGE_LEN];
        file.read_exact(&mut page)
            .map_err(|error| file_error("read", path, error))?;
        let summary = summarize_page(&page, local_index)?;
        if summary.flight_id != span.flight_id {
            return Err(FerroError::Blackbox(format!(
                "cannot resume: page {local_index} belongs to flight {}, expected {}",
                summary.flight_id, span.flight_id
            )));
        }
        if summary.page_sequence != local_index {
            return Err(FerroError::Blackbox(format!(
                "cannot resume: flight page sequence {} at local page {local_index}",
                summary.page_sequence
            )));
        }
    }
    Ok(page_count)
}

pub fn partial_path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

fn cached_page<T: LineTransport>(
    client: &mut FerroClient<T>,
    cache: &mut BTreeMap<u32, [u8; FLASH_PAGE_LEN]>,
    index: u32,
) -> Result<[u8; FLASH_PAGE_LEN]> {
    if let Some(page) = cache.get(&index) {
        return Ok(*page);
    }
    let page = client.read_log_page(index)?;
    summarize_page(&page, index)?;
    cache.insert(index, page);
    Ok(page)
}

fn missing_flight(flight_id: u32) -> FerroError {
    FerroError::Blackbox(format!("flight ID {flight_id} is not present"))
}

fn file_error(operation: &'static str, path: &Path, error: std::io::Error) -> FerroError {
    FerroError::FileIo {
        operation,
        path: path.to_path_buf(),
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_core::blackbox::{FlightRecord, encode_page};
    use tempfile::tempdir;

    fn page(flight: u32, sequence: u32, boot: bool) -> [u8; FLASH_PAGE_LEN] {
        let record = FlightRecord {
            flags: if boot {
                FLIGHT_RECORD_FLAG_BOOT_SESSION_START
            } else {
                0
            },
            ..FlightRecord::default()
        };
        encode_page(flight, sequence, &[record]).unwrap()
    }

    fn pages() -> Vec<[u8; FLASH_PAGE_LEN]> {
        vec![
            page(1, 0, false),
            page(1, 1, false),
            page(2, 0, false),
            page(2, 1, false),
            page(3, 0, true),
            page(3, 1, false),
            page(4, 0, false),
            page(5, 0, true),
        ]
    }

    #[test]
    fn resolves_latest_and_numeric_flight_bounds() {
        let pages = pages();
        let latest = resolve_flight_span(pages.len() as u32, FlightSelector::Latest, |index| {
            Ok(pages[index as usize])
        })
        .unwrap();
        assert_eq!(
            latest,
            FlightSpan {
                flight_id: 5,
                start_page: 7,
                end_page: 8,
                boot_session_start: true,
            }
        );
        let second = resolve_flight_span(pages.len() as u32, FlightSelector::Id(2), |index| {
            Ok(pages[index as usize])
        })
        .unwrap();
        assert_eq!((second.start_page, second.end_page), (2, 4));
    }

    #[test]
    fn catalogs_explicit_boot_sessions() {
        let pages = pages();
        let spans = catalog_flights(pages.len() as u32, |index| Ok(pages[index as usize])).unwrap();
        assert_eq!(
            spans.iter().map(|span| span.flight_id).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            spans
                .iter()
                .map(|span| span.boot_session_start)
                .collect::<Vec<_>>(),
            [false, false, true, false, true]
        );
    }

    #[test]
    fn resume_requires_crc_identity_and_sequence() {
        let root = tempdir().unwrap();
        let path = root.path().join("flight.fwbb.part");
        let span = FlightSpan {
            flight_id: 2,
            start_page: 2,
            end_page: 4,
            boot_session_start: false,
        };
        fs::write(&path, page(2, 0, false)).unwrap();
        assert_eq!(validate_partial_download(&path, span).unwrap(), 1);

        fs::write(&path, page(1, 0, false)).unwrap();
        assert!(
            validate_partial_download(&path, span)
                .unwrap_err()
                .to_string()
                .contains("belongs to flight 1")
        );
    }

    #[test]
    fn corrupt_resume_page_is_rejected() {
        let root = tempdir().unwrap();
        let path = root.path().join("flight.fwbb.part");
        let span = FlightSpan {
            flight_id: 2,
            start_page: 0,
            end_page: 1,
            boot_session_start: false,
        };
        let mut corrupt = page(2, 0, false);
        corrupt[0] ^= 1;
        fs::write(&path, corrupt).unwrap();
        assert!(
            validate_partial_download(&path, span)
                .unwrap_err()
                .to_string()
                .contains("CrcMismatch")
        );
    }
}
