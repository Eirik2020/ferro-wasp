//! The onboard log format against a real Foxeer log: every page decodes,
//! every record matches the CSV FerroConfigurator exported from it, and
//! re-encoding each page reproduces the bytes the firmware wrote.
//!
//! The fixture is described in `fixtures/blackbox/SOURCE.txt`.

use ferrowasp_protocol::blackbox::{
    FLASH_PAGE_LEN, FlightRecord, decode_page, encode_page, record_from_page,
};

const FWBB: &[u8] = include_bytes!("../../../fixtures/blackbox/foxeer-f405-2khz.fwbb");
const CSV: &str = include_str!("../../../fixtures/blackbox/foxeer-f405-2khz.csv");

fn decoded() -> Vec<FlightRecord> {
    let mut records = Vec::new();
    for (index, page) in FWBB.chunks(FLASH_PAGE_LEN).enumerate() {
        let page: &[u8; FLASH_PAGE_LEN] = page.try_into().expect("whole pages");
        let metadata = decode_page(page).expect("a CRC-valid page");
        assert_eq!(metadata.flight_id, 13);
        assert_eq!(metadata.page_sequence, index as u32);
        let page_records: Vec<_> = (0..usize::from(metadata.record_count))
            .map(|record| record_from_page(page, record).expect("a valid record"))
            .collect();
        let reencoded = encode_page(metadata.flight_id, metadata.page_sequence, &page_records)
            .expect("records that fit a page");
        assert_eq!(&reencoded, page, "page {index} re-encodes differently");
        records.extend(page_records);
    }
    records
}

/// Tenths of a degree per second from the CSV's one-decimal value.
fn dps10(field: &str) -> i16 {
    (field.parse::<f64>().expect("a number") * 10.0).round() as i16
}

#[test]
fn every_page_decodes_and_reencodes_to_the_firmwares_bytes() {
    assert_eq!(FWBB.len(), 400 * FLASH_PAGE_LEN);
    assert_eq!(decoded().len(), 2_000);
}

#[test]
fn every_record_matches_the_configurators_export() {
    let records = decoded();
    let rows: Vec<Vec<&str>> = CSV
        .lines()
        .skip(1)
        .map(|row| row.split(',').collect())
        .collect();
    assert_eq!(rows.len(), records.len());
    for (row, record) in rows.iter().zip(&records) {
        let integer = |column: usize| row[column].parse::<i64>().expect("an integer");
        assert_eq!(integer(1), i64::from(record.control_sequence));
        assert_eq!(integer(2), i64::from(record.imu_sequence));
        assert_eq!(
            (row[3].parse::<f64>().unwrap() * 1_000.0).round() as u32,
            record.timestamp_us
        );
        assert_eq!(integer(4), i64::from(record.flags & 1));
        assert_eq!(integer(5), i64::from((record.flags >> 1) & 1));
        let axes = |first: usize| {
            [
                dps10(row[first]),
                dps10(row[first + 1]),
                dps10(row[first + 2]),
            ]
        };
        assert_eq!(axes(6), record.raw_gyro_dps10);
        assert_eq!(axes(9), record.filtered_gyro_dps10);
        assert_eq!(axes(12), record.command_dps10);
        let pid = [integer(15), integer(16), integer(17)].map(|value| value as i16);
        assert_eq!(pid, record.pid);
        assert_eq!(integer(18), i64::from(record.throttle));
        let motors = [integer(19), integer(20), integer(21), integer(22)].map(|value| value as u16);
        assert_eq!(motors, record.motors);
    }
}
