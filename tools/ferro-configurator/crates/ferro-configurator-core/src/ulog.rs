use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use ferrowasp_core::blackbox::{FLASH_PAGE_LEN, FlightRecord, decode_page, record_from_page};
use serde::Serialize;

use crate::{
    blackbox::{FlightSelector, partial_path, summarize_page},
    csv_export::{Timing, measure_timing},
    error::{FerroError, Result},
};

const ULOG_MAGIC: &[u8; 7] = b"ULog\x01\x12\x35";
const ULOG_VERSION: u8 = 1;
const SCHEMA_VERSION: u32 = 1;
const TOPIC_NAME: &str = "ferrowasp_rate_control";
const TOPIC_MESSAGE_ID: u16 = 0;
const MAX_REASONABLE_INTERVAL_US: u32 = 60_000_000;

const TOPIC_FORMAT: &str = concat!(
    "ferrowasp_rate_control:",
    "uint64_t timestamp;",
    "uint32_t flight_id;",
    "uint32_t seq;",
    "uint32_t imu_seq;",
    "float raw_roll_dps;",
    "float raw_pitch_dps;",
    "float raw_yaw_dps;",
    "float gyro_roll_dps;",
    "float gyro_pitch_dps;",
    "float gyro_yaw_dps;",
    "float setpoint_roll_dps;",
    "float setpoint_pitch_dps;",
    "float setpoint_yaw_dps;",
    "float pid_roll;",
    "float pid_pitch;",
    "float pid_yaw;",
    "float throttle_command;",
    "float motor1_command;",
    "float motor2_command;",
    "float motor3_command;",
    "float motor4_command;",
    "uint8_t armed;",
    "uint8_t imu_fresh;",
    "uint8_t flags;",
    "uint8_t _padding0;",
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConversionSummary {
    pub flight_id: u32,
    pub sample_count: u32,
    pub duration_us: u64,
    pub dropout_count: u32,
    /// Control ticks per second measured from the log; `None` for a flight
    /// under 2 s, whose timestamps are then the records' own milliseconds.
    pub control_rate_hz: Option<u32>,
    pub output_bytes: u64,
    pub output: PathBuf,
}

pub fn convert_fwbb_to_ulog(
    input: &Path,
    output: &Path,
    selector: FlightSelector,
    force: bool,
) -> Result<ConversionSummary> {
    let (flight_id, records) = flight_records(input, selector)?;
    let timing = measure_timing(&records)?;
    write_ulog(output, flight_id, &records, timing, force)
}

/// One flight's records from a CRC-validated FWBB archive, its pages
/// contiguous: the source every export reads.
pub(crate) fn flight_records(
    input: &Path,
    selector: FlightSelector,
) -> Result<(u32, Vec<FlightRecord>)> {
    let pages = read_and_validate_pages(input)?;
    let selected_id = match selector {
        FlightSelector::Latest => pages
            .last()
            .map(|(metadata, _)| metadata.flight_id)
            .ok_or_else(|| FerroError::Ulog("input contains no flash pages".to_owned()))?,
        FlightSelector::Id(id) => id,
    };

    let mut records = Vec::new();
    let mut expected_page_sequence = 0;
    for (metadata, page) in &pages {
        if metadata.flight_id != selected_id {
            continue;
        }
        if metadata.page_sequence != expected_page_sequence {
            return Err(FerroError::Ulog(format!(
                "flight {selected_id} page sequence {} is not contiguous at expected page {expected_page_sequence}",
                metadata.page_sequence
            )));
        }
        expected_page_sequence += 1;
        for index in 0..metadata.record_count as usize {
            records.push(record_from_page(page, index).map_err(|error| {
                FerroError::Ulog(format!(
                    "flight {selected_id} record {index} is invalid: {error:?}"
                ))
            })?);
        }
    }
    if records.is_empty() {
        return Err(FerroError::Ulog(format!(
            "flight ID {selected_id} is not present or contains no records"
        )));
    }
    Ok((selected_id, records))
}

fn read_and_validate_pages(
    path: &Path,
) -> Result<Vec<(ferrowasp_core::blackbox::PageMetadata, [u8; FLASH_PAGE_LEN])>> {
    let size = fs::metadata(path)
        .map_err(|error| file_error("inspect", path, error))?
        .len();
    if size == 0 || size % FLASH_PAGE_LEN as u64 != 0 {
        return Err(FerroError::Ulog(format!(
            "{} is not a non-empty, page-aligned FWBB file",
            path.display()
        )));
    }
    let mut file = File::open(path).map_err(|error| file_error("open", path, error))?;
    let mut pages = Vec::with_capacity((size / FLASH_PAGE_LEN as u64) as usize);
    let mut previous_flight = None;
    let mut expected_sequence = 0;
    for page_index in 0..(size / FLASH_PAGE_LEN as u64) as u32 {
        let mut page = [0u8; FLASH_PAGE_LEN];
        file.read_exact(&mut page)
            .map_err(|error| file_error("read", path, error))?;
        let summary = summarize_page(&page, page_index)?;
        if let Some(previous) = previous_flight
            && summary.flight_id < previous
        {
            return Err(FerroError::Ulog(format!(
                "flight IDs move backwards at archive page {page_index}"
            )));
        }
        if previous_flight != Some(summary.flight_id) {
            expected_sequence = 0;
            previous_flight = Some(summary.flight_id);
        }
        if summary.page_sequence != expected_sequence {
            return Err(FerroError::Ulog(format!(
                "archive page {page_index} has flight page sequence {}, expected {expected_sequence}",
                summary.page_sequence
            )));
        }
        expected_sequence += 1;
        let metadata = decode_page(&page).map_err(|error| {
            FerroError::Ulog(format!("archive page {page_index} is invalid: {error:?}"))
        })?;
        pages.push((metadata, page));
    }
    Ok(pages)
}

/// Timestamps count control ticks at the rate measured from the log, as the
/// CSV export's do; the records' own timestamps have only millisecond
/// resolution, and two records can share one.
fn write_ulog(
    output: &Path,
    flight_id: u32,
    records: &[FlightRecord],
    timing: Timing,
    force: bool,
) -> Result<ConversionSummary> {
    if output.exists() && !force {
        return Err(FerroError::FileIo {
            operation: "create",
            path: output.to_path_buf(),
            reason: "output already exists; pass --force to replace it".to_owned(),
        });
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("create directory for", output, error))?;
    }
    let partial = partial_path(output);
    if partial.exists() && !force {
        return Err(FerroError::FileIo {
            operation: "create",
            path: partial,
            reason: "partial conversion already exists; pass --force only after preserving it"
                .to_owned(),
        });
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&partial)
        .map_err(|error| file_error("open", &partial, error))?;
    let mut bytes_written = 0u64;

    bytes_written += write_bytes(&mut file, &partial, ULOG_MAGIC)?;
    bytes_written += write_bytes(&mut file, &partial, &[ULOG_VERSION])?;
    bytes_written += write_bytes(&mut file, &partial, &0u64.to_le_bytes())?;
    bytes_written += write_message(&mut file, &partial, b'B', &[0u8; 40])?;
    bytes_written += write_message(&mut file, &partial, b'F', TOPIC_FORMAT.as_bytes())?;
    bytes_written += write_info_string(&mut file, &partial, "sys_name", "FerroWasp")?;
    bytes_written += write_info_string(&mut file, &partial, "log_type", "converted_fwbb")?;
    bytes_written += write_info_string(&mut file, &partial, "source_format", "FerroWasp BB2")?;
    bytes_written += write_info_u32(&mut file, &partial, "schema_version", SCHEMA_VERSION)?;
    bytes_written += write_info_u32(&mut file, &partial, "flight_id", flight_id)?;
    if let Some((control_rate_hz, _)) = timing.rate {
        bytes_written += write_info_u32(&mut file, &partial, "control_rate_hz", control_rate_hz)?;
    }
    let mut subscription = Vec::with_capacity(3 + TOPIC_NAME.len());
    subscription.push(0);
    subscription.extend_from_slice(&TOPIC_MESSAGE_ID.to_le_bytes());
    subscription.extend_from_slice(TOPIC_NAME.as_bytes());
    bytes_written += write_message(&mut file, &partial, b'A', &subscription)?;

    let mut previous_timestamp = records[0].timestamp_us;
    let mut previous_sequence = records[0].control_sequence;
    let mut ticks = 0u64;
    let mut elapsed_ms_clock = 0u64;
    let mut timestamp = 0u64;
    let mut dropout_count = 0u32;
    for (index, record) in records.iter().enumerate() {
        if index != 0 {
            let interval = record.timestamp_us.wrapping_sub(previous_timestamp);
            if interval > MAX_REASONABLE_INTERVAL_US {
                return Err(FerroError::Ulog(format!(
                    "invalid control timestamp interval {interval} us at sequence {}",
                    record.control_sequence
                )));
            }
            elapsed_ms_clock += u64::from(interval);
            let step = record.control_sequence.wrapping_sub(previous_sequence);
            ticks += u64::from(step);
            if step > timing.record_step {
                let duration_us = timing
                    .ticks_to_us(u64::from(step - timing.record_step))
                    .unwrap_or(u64::from(interval));
                let duration_ms = ((duration_us + 500) / 1_000).clamp(1, u16::MAX as u64) as u16;
                bytes_written +=
                    write_message(&mut file, &partial, b'O', &duration_ms.to_le_bytes())?;
                dropout_count += 1;
            }
        }
        timestamp = timing.ticks_to_us(ticks).unwrap_or(elapsed_ms_clock);
        let data = data_record(record, flight_id, timestamp);
        let mut payload = Vec::with_capacity(2 + data.len());
        payload.extend_from_slice(&TOPIC_MESSAGE_ID.to_le_bytes());
        payload.extend_from_slice(&data);
        bytes_written += write_message(&mut file, &partial, b'D', &payload)?;
        previous_timestamp = record.timestamp_us;
        previous_sequence = record.control_sequence;
    }

    file.sync_all()
        .map_err(|error| file_error("flush", &partial, error))?;
    drop(file);
    replace_with_completed_file(&partial, output)?;
    Ok(ConversionSummary {
        flight_id,
        sample_count: records.len() as u32,
        duration_us: timestamp,
        dropout_count,
        control_rate_hz: timing.rate.map(|(rate_hz, _)| rate_hz),
        output_bytes: bytes_written,
        output: output.to_path_buf(),
    })
}

pub(crate) fn replace_with_completed_file(partial: &Path, output: &Path) -> Result<()> {
    if !output.exists() {
        return fs::rename(partial, output).map_err(|error| file_error("finalize", output, error));
    }
    let backup = PathBuf::from(format!("{}.replace-backup", output.display()));
    if backup.exists() {
        return Err(FerroError::FileIo {
            operation: "replace",
            path: output.to_path_buf(),
            reason: format!(
                "refusing to overwrite existing recovery file {}",
                backup.display()
            ),
        });
    }
    fs::rename(output, &backup).map_err(|error| file_error("preserve", output, error))?;
    if let Err(error) = fs::rename(partial, output) {
        let _ = fs::rename(&backup, output);
        return Err(file_error("finalize", output, error));
    }
    fs::remove_file(&backup).map_err(|error| file_error("remove recovery copy", &backup, error))
}

fn data_record(record: &FlightRecord, flight_id: u32, timestamp_us: u64) -> Vec<u8> {
    let mut output = Vec::with_capacity(91);
    output.extend_from_slice(&timestamp_us.to_le_bytes());
    output.extend_from_slice(&flight_id.to_le_bytes());
    output.extend_from_slice(&record.control_sequence.to_le_bytes());
    output.extend_from_slice(&record.imu_sequence.to_le_bytes());
    for value in record.raw_gyro_dps10 {
        output.extend_from_slice(&(value as f32 / 10.0).to_le_bytes());
    }
    for value in record.filtered_gyro_dps10 {
        output.extend_from_slice(&(value as f32 / 10.0).to_le_bytes());
    }
    for value in record.command_dps10 {
        output.extend_from_slice(&(value as f32 / 10.0).to_le_bytes());
    }
    for value in record.pid {
        output.extend_from_slice(&(value as f32).to_le_bytes());
    }
    output.extend_from_slice(&(record.throttle as f32).to_le_bytes());
    for value in record.motors {
        output.extend_from_slice(&(value as f32).to_le_bytes());
    }
    output.push(u8::from(record.flags & 1 != 0));
    output.push(u8::from(record.flags & 2 != 0));
    output.push(record.flags as u8);
    output
}

fn write_info_string(writer: &mut File, path: &Path, name: &str, value: &str) -> Result<u64> {
    let key = format!("char[{}] {name}", value.len());
    write_info(writer, path, &key, value.as_bytes())
}

fn write_info_u32(writer: &mut File, path: &Path, name: &str, value: u32) -> Result<u64> {
    write_info(
        writer,
        path,
        &format!("uint32_t {name}"),
        &value.to_le_bytes(),
    )
}

fn write_info(writer: &mut File, path: &Path, key: &str, value: &[u8]) -> Result<u64> {
    let key_length = u8::try_from(key.len())
        .map_err(|_| FerroError::Ulog("ULog info key is too long".to_owned()))?;
    let mut payload = Vec::with_capacity(1 + key.len() + value.len());
    payload.push(key_length);
    payload.extend_from_slice(key.as_bytes());
    payload.extend_from_slice(value);
    write_message(writer, path, b'I', &payload)
}

fn write_message(writer: &mut File, path: &Path, kind: u8, payload: &[u8]) -> Result<u64> {
    let length = u16::try_from(payload.len())
        .map_err(|_| FerroError::Ulog("ULog message exceeds uint16 length".to_owned()))?;
    write_bytes(writer, path, &length.to_le_bytes())?;
    write_bytes(writer, path, &[kind])?;
    write_bytes(writer, path, payload)?;
    Ok(payload.len() as u64 + 3)
}

fn write_bytes(writer: &mut File, path: &Path, bytes: &[u8]) -> Result<u64> {
    writer
        .write_all(bytes)
        .map_err(|error| file_error("write", path, error))?;
    Ok(bytes.len() as u64)
}

pub(crate) fn file_error(
    operation: &'static str,
    path: &Path,
    error: std::io::Error,
) -> FerroError {
    FerroError::FileIo {
        operation,
        path: path.to_path_buf(),
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_core::blackbox::encode_page;
    use tempfile::tempdir;

    fn record(sequence: u32, timestamp_us: u32) -> FlightRecord {
        FlightRecord {
            timestamp_us,
            control_sequence: sequence,
            imu_sequence: sequence * 2,
            flags: 3,
            raw_gyro_dps10: [10, 20, 30],
            filtered_gyro_dps10: [40, 50, 60],
            command_dps10: [70, 80, 90],
            pid: [10, 11, 12],
            throttle: 500,
            motors: [501, 502, 503, 504],
        }
    }

    fn messages(data: &[u8]) -> Vec<(u8, &[u8])> {
        assert_eq!(&data[..7], ULOG_MAGIC);
        assert_eq!(data[7], ULOG_VERSION);
        let mut offset = 16;
        let mut output = Vec::new();
        while offset < data.len() {
            let length = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap()) as usize;
            let kind = data[offset + 2];
            offset += 3;
            output.push((kind, &data[offset..offset + length]));
            offset += length;
        }
        output
    }

    #[test]
    fn writes_required_header_format_subscription_and_values() {
        let root = tempdir().unwrap();
        let input = root.path().join("flight.fwbb");
        let output = root.path().join("flight.ulg");
        fs::write(&input, encode_page(7, 0, &[record(10, 100_000)]).unwrap()).unwrap();

        let summary = convert_fwbb_to_ulog(&input, &output, FlightSelector::Latest, false).unwrap();
        let bytes = fs::read(&output).unwrap();
        let parsed = messages(&bytes);
        assert!(
            parsed
                .iter()
                .any(|(kind, payload)| { *kind == b'F' && *payload == TOPIC_FORMAT.as_bytes() })
        );
        let subscription = parsed.iter().find(|(kind, _)| *kind == b'A').unwrap().1;
        assert_eq!(&subscription[..3], &[0, 0, 0]);
        assert_eq!(&subscription[3..], TOPIC_NAME.as_bytes());
        let data = parsed.iter().find(|(kind, _)| *kind == b'D').unwrap().1;
        assert_eq!(u16::from_le_bytes(data[..2].try_into().unwrap()), 0);
        assert_eq!(u64::from_le_bytes(data[2..10].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(data[10..14].try_into().unwrap()), 7);
        assert_eq!(summary.sample_count, 1);
        assert_eq!(summary.duration_us, 0);
    }

    /// A record every `step` ticks of a `rate_hz` loop, stamped to the
    /// millisecond as the firmware's clock stamps it.
    fn flight(rate_hz: u32, step: u32, count: u32) -> Vec<FlightRecord> {
        (0..count)
            .map(|index| {
                let ticks = index * step;
                let time_us = u64::from(ticks) * 1_000_000 / u64::from(rate_hz);
                record(100 + ticks, (1_000_000 + time_us / 1_000 * 1_000) as u32)
            })
            .collect()
    }

    fn write_flight(path: &Path, flight_id: u32, records: &[FlightRecord]) {
        let mut bytes = encode_page(flight_id - 1, 0, &[record(1, 10_000)])
            .unwrap()
            .to_vec();
        for (page, chunk) in records
            .chunks(ferrowasp_core::blackbox::RECORDS_PER_PAGE)
            .enumerate()
        {
            bytes.extend_from_slice(&encode_page(flight_id, page as u32, chunk).unwrap());
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn selects_latest_flight_and_times_it_at_the_measured_rate() {
        let root = tempdir().unwrap();
        let input = root.path().join("flights.fwbb");
        let output = root.path().join("latest.ulg");
        // 3 s at 2 kHz, one record every 2 ticks, 20 ticks lost after record 1000.
        let mut records = flight(2_000, 2, 3_000);
        for record in &mut records[1_000..] {
            record.control_sequence += 20;
            record.timestamp_us += 10_000;
        }
        write_flight(&input, 7, &records);

        let summary = convert_fwbb_to_ulog(&input, &output, FlightSelector::Latest, false).unwrap();
        assert_eq!(summary.flight_id, 7);
        assert_eq!(summary.sample_count, 3_000);
        assert_eq!(summary.control_rate_hz, Some(2_000));
        assert_eq!(summary.dropout_count, 1);
        // 5998 + 20 ticks at 2 kHz.
        assert_eq!(summary.duration_us, 3_009_000);

        let bytes = fs::read(output).unwrap();
        let parsed = messages(&bytes);
        let dropout = parsed.iter().find(|(kind, _)| *kind == b'O').unwrap().1;
        // The 20 ticks beyond the usual 2, at 2 kHz.
        assert_eq!(u16::from_le_bytes(dropout.try_into().unwrap()), 10);
        let rate_info = parsed
            .iter()
            .find(|(kind, payload)| {
                *kind == b'I' && payload.windows(15).any(|w| w == b"control_rate_hz")
            })
            .unwrap()
            .1;
        assert_eq!(
            u32::from_le_bytes(rate_info[rate_info.len() - 4..].try_into().unwrap()),
            2_000
        );
        let data: Vec<_> = parsed.iter().filter(|(kind, _)| *kind == b'D').collect();
        let second = u64::from_le_bytes(data[1].1[2..10].try_into().unwrap());
        assert_eq!(second, 1_000);
    }

    #[test]
    fn records_sharing_a_millisecond_convert() {
        let root = tempdir().unwrap();
        let input = root.path().join("flight.fwbb");
        let output = root.path().join("flight.ulg");
        // Every tick logged at 2 kHz: two records per millisecond stamp.
        write_flight(&input, 3, &flight(2_000, 1, 5_000));

        let summary = convert_fwbb_to_ulog(&input, &output, FlightSelector::Latest, false).unwrap();
        assert_eq!(summary.control_rate_hz, Some(2_000));
        assert_eq!(summary.dropout_count, 0);
        assert_eq!(summary.duration_us, 2_499_500);
    }

    #[test]
    fn corrupt_input_is_rejected_without_final_output() {
        let root = tempdir().unwrap();
        let input = root.path().join("flight.fwbb");
        let output = root.path().join("flight.ulg");
        let mut page = encode_page(7, 0, &[record(1, 10_000)]).unwrap();
        page[0] ^= 1;
        fs::write(&input, page).unwrap();

        assert!(convert_fwbb_to_ulog(&input, &output, FlightSelector::Latest, false).is_err());
        assert!(!output.exists());
    }
}
