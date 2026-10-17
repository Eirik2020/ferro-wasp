//! The props-off ESC telemetry bench check over USB (ADR-0045): sample the
//! `esc` command over time, then report per physical output how well the
//! bidirectional DShot reply eRPM agrees with the legacy UART eRPM, and how
//! many replies were lost.
//!
//! `summarise` and the CSV only read. `run_motor_spin` is the operator's
//! motor test with sampling between renewals: the same lease request the
//! configurator's motor test sends, which the firmware refuses unless
//! disarmed with the arm switch off, and which lapses within 250 ms of the
//! last renewal.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::client::{EscLane, FerroClient};
use crate::error::{FerroError, Result};
use crate::transport::LineTransport;

/// A UART sample older than this is not compared: it may describe a speed
/// the motor has left.
pub const FRESH_UART_MS: u32 = 100;

/// One poll of the `esc` command, timed on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EscBenchSample {
    pub host_ms: u64,
    pub lanes: [EscLane; 4],
}

/// One physical output's result over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct EscBenchOutput {
    pub output: u8,
    /// Samples where both eRPMs were present and the UART one was fresh.
    pub compared: u32,
    /// Mean of reply minus UART eRPM over the compared samples.
    pub mean_difference_erpm: f64,
    /// Largest difference either way.
    pub max_abs_difference_erpm: u32,
    /// Replies counted during the run, valid and invalid.
    pub replies_valid: u32,
    pub replies_invalid: u32,
}

impl EscBenchOutput {
    /// Invalid replies as a fraction of all replies counted, if any were.
    pub fn loss(&self) -> Option<f64> {
        let total = self.replies_valid + self.replies_invalid;
        (total > 0).then(|| f64::from(self.replies_invalid) / f64::from(total))
    }
}

/// Summarises a run's samples, in the order taken.
pub fn summarise(samples: &[EscBenchSample]) -> [EscBenchOutput; 4] {
    core::array::from_fn(|index| {
        let mut result = EscBenchOutput {
            output: index as u8 + 1,
            ..EscBenchOutput::default()
        };
        let mut sum = 0i64;
        for sample in samples {
            let lane = sample.lanes[index];
            if let (Some(uart), Some(age), Some(reply)) =
                (lane.uart_erpm, lane.uart_age_ms, lane.reply_erpm)
                && age <= FRESH_UART_MS
            {
                let difference = i64::from(reply) - i64::from(uart);
                sum += difference;
                result.compared += 1;
                result.max_abs_difference_erpm = result
                    .max_abs_difference_erpm
                    .max(difference.unsigned_abs().min(u64::from(u32::MAX)) as u32);
            }
        }
        if result.compared > 0 {
            result.mean_difference_erpm = sum as f64 / f64::from(result.compared);
        }
        // The counters only grow, so the run's share is last minus first.
        if let (Some(first), Some(last)) = (samples.first(), samples.last()) {
            result.replies_valid = last.lanes[index]
                .replies_valid
                .wrapping_sub(first.lanes[index].replies_valid);
            result.replies_invalid = last.lanes[index]
                .replies_invalid
                .wrapping_sub(first.lanes[index].replies_invalid);
        }
        result
    })
}

/// The CSV header and one row per output per sample, for MATLAB's
/// `readtable`: a value never received is left empty, which reads as NaN.
pub const CSV_HEADER: &str =
    "host_ms,output,uart_erpm,uart_age_ms,reply_erpm,replies_valid,replies_invalid";

pub fn csv_rows(sample: &EscBenchSample) -> [String; 4] {
    let cell = |value: Option<u32>| value.map(|value| value.to_string()).unwrap_or_default();
    sample.lanes.map(|lane| {
        format!(
            "{},{},{},{},{},{},{}",
            sample.host_ms,
            lane.output,
            cell(lane.uart_erpm),
            cell(lane.uart_age_ms),
            cell(lane.reply_erpm),
            lane.replies_valid,
            lane.replies_invalid
        )
    })
}

/// How a motor spin run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpinOutcome {
    pub samples: usize,
    /// `esc` polls that failed; the motor kept spinning.
    pub failed_polls: usize,
}

/// Holds logical motor `motor` (1-4) at idle for `duration`, renewing its
/// lease every `period` and handing each `esc` sample to `record`, then stops
/// every motor. A refused or failed renewal ends the run; the stop is sent
/// whatever ended it, and the firmware's lease stops the motor regardless.
pub fn run_motor_spin<T: LineTransport>(
    client: &mut FerroClient<T>,
    motor: u8,
    duration: Duration,
    period: Duration,
    mut record: impl FnMut(&EscBenchSample) -> Result<()>,
) -> Result<SpinOutcome> {
    let started = Instant::now();
    let mut outcome = SpinOutcome {
        samples: 0,
        failed_polls: 0,
    };
    let mut run = || -> Result<()> {
        while started.elapsed() < duration {
            let tick = Instant::now();
            client.motor_spin(motor)?;
            match client.esc_telemetry() {
                Ok(lanes) => {
                    let sample = EscBenchSample {
                        host_ms: started.elapsed().as_millis() as u64,
                        lanes,
                    };
                    record(&sample)?;
                    outcome.samples += 1;
                }
                Err(error @ FerroError::DeviceRejected { .. }) => return Err(error),
                Err(_) => outcome.failed_polls += 1,
            }
            if let Some(rest) = period.checked_sub(tick.elapsed()) {
                std::thread::sleep(rest);
            }
        }
        Ok(())
    };
    let result = run();
    let stopped = client.motor_stop();
    result?;
    stopped?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    const ESC_LINES: [&str; 4] = [
        "OK esc1 u=66 a=12 b=67 v=10 x=0",
        "OK esc2 u=- a=- b=0 v=10 x=0",
        "OK esc3 u=- a=- b=0 v=10 x=0",
        "OK esc4 u=- a=- b=0 v=10 x=0",
    ];

    #[test]
    fn a_spin_renews_samples_and_stops() {
        let mut lines = std::vec::Vec::new();
        for _ in 0..3 {
            lines.push("OK motor");
            lines.extend(ESC_LINES);
        }
        lines.push("OK motor stop");
        let mut client =
            FerroClient::new(MockTransport::with_lines(lines), Duration::from_millis(20));
        let mut recorded = 0;
        let outcome = run_motor_spin(
            &mut client,
            2,
            Duration::from_millis(30),
            Duration::from_millis(10),
            |sample| {
                assert_eq!(sample.lanes[0].reply_erpm, Some(6_700));
                recorded += 1;
                Ok(())
            },
        )
        .unwrap();
        assert!(outcome.samples >= 1 && outcome.samples <= 3, "{outcome:?}");
        assert_eq!(outcome.samples, recorded);
        let writes = client.into_transport().writes;
        assert_eq!(
            writes.first().map(String::as_str),
            Some("motor spin 2 CONFIRM")
        );
        assert_eq!(writes.last().map(String::as_str), Some("motor stop"));
    }

    #[test]
    fn a_refused_spin_ends_the_run_and_still_stops() {
        let lines = ["ERR motor arm switch is on", "OK motor stop"];
        let mut client =
            FerroClient::new(MockTransport::with_lines(lines), Duration::from_millis(20));
        let result = run_motor_spin(
            &mut client,
            1,
            Duration::from_secs(5),
            Duration::from_millis(10),
            |_| Ok(()),
        );
        assert!(matches!(result, Err(FerroError::DeviceRejected { .. })));
        let writes = client.into_transport().writes;
        assert_eq!(writes, ["motor spin 1 CONFIRM", "motor stop"]);
    }

    fn lane(output: u8) -> EscLane {
        EscLane {
            output,
            uart_erpm: None,
            uart_age_ms: None,
            reply_erpm: None,
            replies_valid: 0,
            replies_invalid: 0,
        }
    }

    fn sample(host_ms: u64, first: EscLane) -> EscBenchSample {
        EscBenchSample {
            host_ms,
            lanes: [first, lane(2), lane(3), lane(4)],
        }
    }

    #[test]
    fn fresh_pairs_are_compared_and_stale_or_missing_ones_skipped() {
        let both = |uart, age, reply| EscLane {
            uart_erpm: Some(uart),
            uart_age_ms: Some(age),
            reply_erpm: Some(reply),
            ..lane(1)
        };
        let samples = [
            sample(0, both(6_600, 10, 6_700)),
            sample(100, both(6_800, 50, 6_700)),
            // Stale UART: not compared.
            sample(200, both(1_000, 500, 6_700)),
            // No reply yet: not compared.
            sample(
                300,
                EscLane {
                    reply_erpm: None,
                    ..both(6_600, 10, 0)
                },
            ),
        ];
        let [first, second, ..] = summarise(&samples);
        assert_eq!(first.compared, 2);
        assert_eq!(first.mean_difference_erpm, 0.0);
        assert_eq!(first.max_abs_difference_erpm, 100);
        assert_eq!(second.compared, 0);
    }

    #[test]
    fn reply_loss_counts_only_the_run() {
        let counts = |valid, invalid| EscLane {
            replies_valid: valid,
            replies_invalid: invalid,
            ..lane(1)
        };
        let samples = [
            sample(0, counts(1_000, 7)),
            sample(1_000, counts(1_990, 17)),
        ];
        let [first, ..] = summarise(&samples);
        assert_eq!(first.replies_valid, 990);
        assert_eq!(first.replies_invalid, 10);
        assert_eq!(first.loss(), Some(0.01));
        assert_eq!(summarise(&[])[0].loss(), None);
    }

    #[test]
    fn csv_rows_leave_missing_values_empty() {
        let rows = csv_rows(&sample(
            42,
            EscLane {
                uart_erpm: Some(6_600),
                uart_age_ms: Some(12),
                replies_valid: 5,
                ..lane(1)
            },
        ));
        assert_eq!(rows[0], "42,1,6600,12,,5,0");
        assert_eq!(rows[3], "42,4,,,,0,0");
        assert_eq!(CSV_HEADER.split(',').count(), rows[0].split(',').count());
    }
}
