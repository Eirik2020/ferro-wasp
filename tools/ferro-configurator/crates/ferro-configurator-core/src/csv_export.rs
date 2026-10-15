//! One flight as CSV, for MATLAB's `readtable` or any spreadsheet.
//!
//! One row per logged record, one column per signal, each named with its
//! unit. `time_s` counts control ticks at the control rate measured from the
//! log, because the records' own timestamps have only millisecond resolution.
//! The log does not store the rate, so it is measured over the whole flight,
//! where that resolution costs well under a tick.

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use ferrowasp_core::blackbox::FlightRecord;
use serde::Serialize;

use crate::{
    blackbox::{FlightSelector, partial_path},
    error::{FerroError, Result},
    ulog::{file_error, flight_records, replace_with_completed_file},
};

/// The shortest flight whose control rate can be measured to within a few
/// hertz from millisecond timestamps.
const MIN_MEASURED_DURATION_US: u64 = 2_000_000;

/// Control rates are multiples of this; the measurement rounds to one.
const RATE_ROUNDING_HZ: f64 = 10.0;

pub const CSV_HEADER: &str = concat!(
    "time_s,seq,imu_seq,timestamp_ms,armed,imu_fresh,",
    "gyro_raw_roll_dps,gyro_raw_pitch_dps,gyro_raw_yaw_dps,",
    "gyro_roll_dps,gyro_pitch_dps,gyro_yaw_dps,",
    "setpoint_roll_dps,setpoint_pitch_dps,setpoint_yaw_dps,",
    "pid_roll_counts,pid_pitch_counts,pid_yaw_counts,",
    "throttle_counts,motor1_counts,motor2_counts,motor3_counts,motor4_counts",
);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CsvSummary {
    pub flight_id: u32,
    pub rows: u32,
    pub duration_s: f64,
    /// Control ticks per second, measured from the log and rounded to 10 Hz.
    pub control_rate_hz: u32,
    /// The measurement before rounding, so a doubtful rate can be checked.
    pub measured_rate_hz: f64,
    /// Control ticks between consecutive records: the log rate divisor.
    pub record_step: u32,
    /// Places where more than `record_step` ticks passed between records.
    pub gaps: u32,
    pub output: PathBuf,
}

/// How the flight's records sit in time.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Timing {
    control_rate_hz: u32,
    measured_rate_hz: f64,
    record_step: u32,
    gaps: u32,
}

pub fn convert_fwbb_to_csv(
    input: &Path,
    output: &Path,
    selector: FlightSelector,
    force: bool,
) -> Result<CsvSummary> {
    let (flight_id, records) = flight_records(input, selector)?;
    let timing = measure_timing(&records)?;
    write_csv(output, &records, timing, force)?;
    let ticks = records
        .windows(2)
        .map(|pair| {
            u64::from(
                pair[1]
                    .control_sequence
                    .wrapping_sub(pair[0].control_sequence),
            )
        })
        .sum::<u64>();
    Ok(CsvSummary {
        flight_id,
        rows: records.len() as u32,
        duration_s: ticks as f64 / f64::from(timing.control_rate_hz),
        control_rate_hz: timing.control_rate_hz,
        measured_rate_hz: timing.measured_rate_hz,
        record_step: timing.record_step,
        gaps: timing.gaps,
        output: output.to_path_buf(),
    })
}

fn measure_timing(records: &[FlightRecord]) -> Result<Timing> {
    let mut elapsed_us = 0u64;
    let mut ticks = 0u64;
    let mut steps: HashMap<u32, u32> = HashMap::new();
    for pair in records.windows(2) {
        elapsed_us += u64::from(pair[1].timestamp_us.wrapping_sub(pair[0].timestamp_us));
        let step = pair[1]
            .control_sequence
            .wrapping_sub(pair[0].control_sequence);
        if step == 0 {
            return Err(FerroError::Ulog(format!(
                "control sequence {} repeats",
                pair[1].control_sequence
            )));
        }
        ticks += u64::from(step);
        *steps.entry(step).or_default() += 1;
    }
    if elapsed_us < MIN_MEASURED_DURATION_US {
        return Err(FerroError::Ulog(format!(
            "the flight lasts {} ms; at least 2 s is needed to measure its control rate",
            elapsed_us / 1_000
        )));
    }
    let measured_rate_hz = ticks as f64 * 1e6 / elapsed_us as f64;
    let control_rate_hz = ((measured_rate_hz / RATE_ROUNDING_HZ).round() * RATE_ROUNDING_HZ) as u32;
    // The usual step is the divisor; the smallest wins a tie.
    let record_step = steps
        .iter()
        .max_by(|(step_a, count_a), (step_b, count_b)| {
            count_a.cmp(count_b).then(step_b.cmp(step_a))
        })
        .map(|(step, _)| *step)
        .unwrap_or(1);
    let gaps = steps
        .iter()
        .filter(|(step, _)| **step > record_step)
        .map(|(_, count)| *count)
        .sum();
    Ok(Timing {
        control_rate_hz,
        measured_rate_hz,
        record_step,
        gaps,
    })
}

fn write_csv(output: &Path, records: &[FlightRecord], timing: Timing, force: bool) -> Result<()> {
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
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&partial)
        .map_err(|error| file_error("open", &partial, error))?;
    let mut writer = BufWriter::new(file);
    let write = |writer: &mut BufWriter<fs::File>, text: &str| {
        writer
            .write_all(text.as_bytes())
            .map_err(|error| file_error("write", &partial, error))
    };
    write(&mut writer, CSV_HEADER)?;
    write(&mut writer, "\n")?;
    let first_sequence = records[0].control_sequence;
    let mut ticks = 0u64;
    let mut previous_sequence = first_sequence;
    for record in records {
        ticks += u64::from(record.control_sequence.wrapping_sub(previous_sequence));
        previous_sequence = record.control_sequence;
        let time_s = ticks as f64 / f64::from(timing.control_rate_hz);
        write(&mut writer, &csv_row(record, time_s))?;
    }
    let file = writer
        .into_inner()
        .map_err(|error| file_error("flush", &partial, error.into_error()))?;
    file.sync_all()
        .map_err(|error| file_error("flush", &partial, error))?;
    drop(file);
    replace_with_completed_file(&partial, output)
}

fn csv_row(record: &FlightRecord, time_s: f64) -> String {
    let dps = |value: i16| format!("{:.1}", f32::from(value) / 10.0);
    let mut fields = vec![
        format!("{time_s:.6}"),
        record.control_sequence.to_string(),
        record.imu_sequence.to_string(),
        format!("{:.3}", f64::from(record.timestamp_us) / 1_000.0),
        u8::from(record.flags & 1 != 0).to_string(),
        u8::from(record.flags & 2 != 0).to_string(),
    ];
    fields.extend(record.raw_gyro_dps10.into_iter().map(dps));
    fields.extend(record.filtered_gyro_dps10.into_iter().map(dps));
    fields.extend(record.command_dps10.into_iter().map(dps));
    fields.extend(record.pid.into_iter().map(|value| value.to_string()));
    fields.push(record.throttle.to_string());
    fields.extend(record.motors.into_iter().map(|value| value.to_string()));
    let mut row = fields.join(",");
    row.push('\n');
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_core::blackbox::{RECORDS_PER_PAGE, encode_page};
    use tempfile::tempdir;

    /// A record every `step` ticks of a `rate_hz` loop, stamped to the
    /// millisecond as the firmware's 1 kHz clock stamps it.
    fn records(rate_hz: u32, step: u32, count: u32) -> Vec<FlightRecord> {
        (0..count)
            .map(|index| {
                let sequence = 1_000 + index * step;
                let time_us = u64::from(index * step) * 1_000_000 / u64::from(rate_hz);
                FlightRecord {
                    timestamp_us: (5_000_000 + time_us / 1_000 * 1_000) as u32,
                    control_sequence: sequence,
                    imu_sequence: sequence * 2,
                    flags: 3,
                    raw_gyro_dps10: [10, -20, 30],
                    filtered_gyro_dps10: [40, 50, -60],
                    command_dps10: [70, 80, 90],
                    pid: [-10, 11, 12],
                    throttle: 500,
                    motors: [501, 502, 503, 504],
                }
            })
            .collect()
    }

    #[test]
    fn the_rate_and_divisor_come_from_the_log() {
        let timing = measure_timing(&records(2_000, 2, 2_500)).unwrap();
        assert_eq!(timing.control_rate_hz, 2_000);
        assert!((timing.measured_rate_hz - 2_000.0).abs() < 2.0);
        assert_eq!(timing.record_step, 2);
        assert_eq!(timing.gaps, 0);

        let mut gapped = records(1_000, 1, 3_000);
        for record in &mut gapped[1_500..] {
            record.control_sequence += 40;
            record.timestamp_us += 40_000;
        }
        let timing = measure_timing(&gapped).unwrap();
        assert_eq!(timing.control_rate_hz, 1_000);
        assert_eq!(timing.gaps, 1);

        assert!(measure_timing(&records(1_000, 1, 500)).is_err());
    }

    #[test]
    fn writes_a_header_and_one_row_per_record_in_stated_units() {
        let root = tempdir().unwrap();
        let input = root.path().join("flight.fwbb");
        let output = root.path().join("flight.csv");
        let flight = records(1_000, 4, 600);
        let mut bytes = Vec::new();
        for (page, chunk) in flight.chunks(RECORDS_PER_PAGE).enumerate() {
            bytes.extend_from_slice(&encode_page(9, page as u32, chunk).unwrap());
        }
        fs::write(&input, bytes).unwrap();

        let summary = convert_fwbb_to_csv(&input, &output, FlightSelector::Latest, false).unwrap();
        assert_eq!(summary.flight_id, 9);
        assert_eq!(summary.rows, 600);
        assert_eq!(summary.control_rate_hz, 1_000);
        assert_eq!(summary.record_step, 4);

        let text = fs::read_to_string(&output).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some(CSV_HEADER));
        assert_eq!(
            lines.next(),
            Some(
                "0.000000,1000,2000,5000.000,1,1,1.0,-2.0,3.0,4.0,5.0,-6.0,7.0,8.0,9.0,-10,11,12,500,501,502,503,504"
            )
        );
        assert!(lines.next().unwrap().starts_with("0.004000,1004,"));
        assert_eq!(text.lines().count(), 601);
        assert_eq!(
            CSV_HEADER.split(',').count(),
            text.lines().nth(1).unwrap().split(',').count()
        );

        assert!(convert_fwbb_to_csv(&input, &output, FlightSelector::Latest, false).is_err());
    }
}
