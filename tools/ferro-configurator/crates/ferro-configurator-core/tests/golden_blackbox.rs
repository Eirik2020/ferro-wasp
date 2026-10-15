//! FerroConfigurator's exports of a real Foxeer log, byte for byte: the CSV
//! MATLAB reads and the ULog PlotJuggler reads. A change here is a change to
//! what every analysis sees; regenerate the expected files only on purpose
//! (`fixtures/blackbox/SOURCE.txt`).

use std::path::{Path, PathBuf};

use ferro_configurator_core::{
    FlightSelector, convert_fwbb_to_csv, convert_fwbb_to_ulog, sha256_file,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../fixtures/blackbox")
        .join(name)
}

#[test]
fn the_csv_export_is_unchanged() {
    let output = tempfile::tempdir().unwrap();
    let csv = output.path().join("flight.csv");
    let summary = convert_fwbb_to_csv(
        &fixture("foxeer-f405-2khz.fwbb"),
        &csv,
        FlightSelector::Latest,
        false,
    )
    .unwrap();
    assert_eq!(summary.flight_id, 13);
    assert_eq!(summary.control_rate_hz, 2_000);
    assert_eq!(summary.record_step, 2);
    assert_eq!(summary.gaps, 398);
    assert_eq!(
        std::fs::read_to_string(&csv).unwrap(),
        std::fs::read_to_string(fixture("foxeer-f405-2khz.csv")).unwrap()
    );
}

#[test]
fn the_ulog_export_is_unchanged() {
    let output = tempfile::tempdir().unwrap();
    let ulog = output.path().join("flight.ulg");
    let summary = convert_fwbb_to_ulog(
        &fixture("foxeer-f405-2khz.fwbb"),
        &ulog,
        FlightSelector::Latest,
        false,
    )
    .unwrap();
    assert_eq!(summary.control_rate_hz, Some(2_000));
    assert_eq!(summary.dropout_count, 398);
    let expected = std::fs::read_to_string(fixture("foxeer-f405-2khz.ulg.sha256")).unwrap();
    // sha256_file writes uppercase hex; sha256sum, which made the file, lowercase.
    assert_eq!(
        sha256_file(&ulog).unwrap().to_ascii_lowercase(),
        expected.split_whitespace().next().unwrap()
    );
}
