import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.blackbox_analyzer import BlackboxSample, write_csv
from tools.flight_report import (
    find_events,
    load_samples,
    prepare_rows,
    sample_rate,
    write_summary,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
FLIGHT_REPORT = REPO_ROOT / "tools" / "flight_report.py"


def flight(
    control_rate_hz: int,
    seconds: float,
    *,
    record_step: int = 1,
    first_seq: int = 1_000,
    first_us: int = 5_000_000,
    timed: bool = True,
) -> list[BlackboxSample]:
    """Synthetic records, every `record_step`th control tick, with the onboard
    log's whole-millisecond timestamps."""
    samples = []
    for tick in range(0, round(seconds * control_rate_hz), record_step):
        exact_us = first_us + tick * 1_000_000 // control_rate_hz
        samples.append(
            BlackboxSample(
                version=2,
                seq=first_seq + tick,
                imu_seq=first_seq + tick,
                flags=3,
                raw_dps=(0.0, 0.0, 0.0),
                gyro_dps=(float(tick % 7), 0.0, 0.0),
                command_dps=(0.0, 0.0, 0.0),
                pid=(0, 0, 0),
                throttle=600,
                motors=(500, 500, 500, 500),
                timestamp_us=exact_us // 1_000 * 1_000 if timed else None,
            )
        )
    return samples


def analyzer_csv(directory: str, samples: list[BlackboxSample]) -> Path:
    path = Path(directory) / "flight.csv"
    write_csv(path, samples)
    return path


def bb2_line(seq: int) -> str:
    return (
        f"DRONE: BB2 seq {seq} imu {seq} flags 3 gyro10 [1, 2, 3] cmd10 [0, 0, 0] "
        "pid [0, 0, 0] thr 600 motors [500, 500, 500, 500]"
    )


class MeasuredRateTests(unittest.TestCase):
    def measured(self, samples: list[BlackboxSample]):
        with tempfile.TemporaryDirectory() as directory:
            return sample_rate(load_samples(analyzer_csv(directory, samples)), None)

    def test_a_400_hz_log_measures_400_hz(self) -> None:
        rate = self.measured(flight(400, 3.0))

        self.assertAlmostEqual(rate.hz, 400.0, delta=0.5)
        self.assertIn("measured from `timestamp_us`", rate.source)
        self.assertIn("control loop 400.", rate.source)

    def test_a_2_khz_log_measures_2_khz(self) -> None:
        rate = self.measured(flight(2_000, 3.0))

        self.assertAlmostEqual(rate.hz, 2_000.0, delta=1.0)
        self.assertIn("control loop 2000.", rate.source)

    def test_a_log_keeping_every_second_tick_reports_its_own_sample_rate(self) -> None:
        rate = self.measured(flight(2_000, 3.0, record_step=2))

        self.assertAlmostEqual(rate.hz, 1_000.0, delta=0.5)
        self.assertIn("control loop 2000.", rate.source)

    def test_a_reboot_does_not_bend_the_rate(self) -> None:
        before = flight(2_000, 1.5, first_seq=900_000, first_us=60_000_000)
        after = flight(2_000, 1.5, first_seq=10, first_us=4_000)

        rate = self.measured(before + after)

        self.assertAlmostEqual(rate.hz, 2_000.0, delta=1.0)
        self.assertIn(f"{len(before) + len(after) - 2} sample intervals", rate.source)

    def test_trims_and_times_are_in_seconds_at_the_measured_rate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            rows = load_samples(analyzer_csv(directory, flight(2_000, 3.0)))
        rate = sample_rate(rows, None)

        trimmed = prepare_rows(rows, rate.hz, 0.5, 0.5)

        self.assertAlmostEqual(len(trimmed), 4_000, delta=2)
        self.assertAlmostEqual(trimmed[-1]["t"], 2.0, delta=0.001)

    def test_the_summary_states_the_rate_and_where_it_came_from(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = analyzer_csv(directory, flight(2_000, 2.0))
            rows = load_samples(source)
            rate = sample_rate(rows, None)
            rows = prepare_rows(rows, rate.hz, 0.0, 0.0)
            summary = Path(directory) / "report.md"
            write_summary(summary, source, rows, rate, [], find_events(rows))
            text = summary.read_text(encoding="utf-8")

        stated = re.search(
            r"^- sample rate: `([0-9.]+) Hz`, measured from `timestamp_us`: .*control loop ([0-9.]+) Hz$",
            text,
            re.M,
        )
        self.assertIsNotNone(stated, text)
        assert stated is not None
        self.assertAlmostEqual(float(stated.group(1)), 2_000.0, delta=1.0)
        self.assertAlmostEqual(float(stated.group(2)), 2_000.0, delta=1.0)


class NoMeasurableTimeTests(unittest.TestCase):
    def test_a_csv_without_timestamps_is_refused_naming_the_override(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            rows = load_samples(analyzer_csv(directory, flight(2_000, 3.0, timed=False)))

        with self.assertRaisesRegex(ValueError, "--sample-rate"):
            sample_rate(rows, None)

    def test_under_a_second_of_timestamps_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            rows = load_samples(analyzer_csv(directory, flight(2_000, 0.5)))

        with self.assertRaisesRegex(ValueError, "--sample-rate"):
            sample_rate(rows, None)

    def test_an_rtt_log_carries_no_time(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.log"
            path.write_text("\n".join(bb2_line(seq) for seq in range(2_000)) + "\n", encoding="utf-8")
            rows = load_samples(path)

        self.assertEqual(len(rows), 2_000)
        with self.assertRaisesRegex(ValueError, "--sample-rate"):
            sample_rate(rows, None)

    def test_the_override_is_used_as_given(self) -> None:
        rate = sample_rate([{"seq": 1.0}, {"seq": 2.0}], 2_000.0)

        self.assertEqual(rate.hz, 2_000.0)
        self.assertEqual(rate.source, "given with `--sample-rate`")

    def test_a_meaningless_override_is_refused(self) -> None:
        for override in (0.0, -400.0, float("nan"), float("inf")):
            with self.subTest(override=override):
                with self.assertRaisesRegex(ValueError, "--sample-rate"):
                    sample_rate([{"seq": 1.0}, {"seq": 2.0}], override)

    def test_the_command_exits_nonzero_and_writes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = analyzer_csv(directory, flight(400, 3.0, timed=False))
            out_dir = Path(directory) / "report"
            result = subprocess.run(
                [sys.executable, str(FLIGHT_REPORT), str(source), "--out-dir", str(out_dir)],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
                timeout=60,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--sample-rate", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertFalse(out_dir.exists())


class CsvInputTests(unittest.TestCase):
    def test_a_configurator_csv_is_not_an_analyzer_csv(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "flight.csv"
            path.write_text(
                "time_s,seq,imu_seq,timestamp_ms,armed,imu_fresh\n0.0,1,1,1.000,1,1\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "not a `blackbox_analyzer.py --csv` export"):
                load_samples(path)

    def test_a_malformed_timestamp_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = analyzer_csv(directory, flight(400, 0.01))
            text = path.read_text(encoding="utf-8").splitlines()
            text[2] = "12.5x" + text[2][text[2].index(","):]
            path.write_text("\n".join(text) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, ":3:"):
                load_samples(path)


if __name__ == "__main__":
    unittest.main()
