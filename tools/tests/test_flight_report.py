import csv
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


def with_field(path: Path, record: int, column: str, value: str) -> Path:
    """Set one field of the `record`th data row (0-based) of an analyzer CSV."""
    with path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.reader(handle))
    rows[record + 1][rows[0].index(column)] = value
    with path.open("w", newline="", encoding="utf-8") as handle:
        csv.writer(handle).writerows(rows)
    return path


# Runs the tool as a script, with matplotlib unimportable when asked.
RUNNER = """
import runpy, sys
if sys.argv.pop(1) == "block":
    sys.modules["matplotlib"] = None
    sys.modules["matplotlib.pyplot"] = None
sys.path.insert(0, sys.argv[1].rsplit("/", 1)[0])
sys.argv = sys.argv[1:]
runpy.run_path(sys.argv[0], run_name="__main__")
"""


def run_report(*args: str, block_matplotlib: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            "-c",
            RUNNER,
            "block" if block_matplotlib else "allow",
            FLIGHT_REPORT.as_posix(),
            *args,
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )


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

    def test_a_malformed_value_the_report_reads_is_refused_with_its_line(self) -> None:
        cases = (
            ("seq", ""),
            ("seq", "inf"),
            ("seq", "nan"),
            ("seq", "-1"),
            ("seq", "1.5"),
            ("armed", ""),
            ("throttle", "inf"),
            ("gyro_roll_dps", "nan"),
            ("cmd_yaw_dps", ""),
            ("pid_pitch", "-inf"),
            ("motor4", ""),
            ("imu_seq", ""),
            ("timestamp_us", "-1"),
            ("timestamp_us", str(2**32)),
        )
        for column, value in cases:
            with self.subTest(column=column, value=value):
                with tempfile.TemporaryDirectory() as directory:
                    path = with_field(analyzer_csv(directory, flight(400, 0.05)), 4, column, value)
                    with self.assertRaisesRegex(ValueError, f":6: {column} "):
                        load_samples(path)

    def test_the_analyzers_empty_optional_fields_and_a_wrapping_timestamp_are_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = analyzer_csv(directory, flight(400, 0.05))
            for column in ("flight_id", "raw_roll_dps", "raw_pitch_dps", "raw_yaw_dps", "timestamp_us"):
                with_field(path, 4, column, "")
            with_field(path, 5, "timestamp_us", str(2**32 - 1))
            rows = load_samples(path)

        self.assertEqual(rows[4]["raw_roll_dps"], 0.0)
        self.assertNotIn("timestamp_us", rows[4])
        self.assertEqual(rows[5]["timestamp_us"], float(2**32 - 1))

    def test_an_oversized_field_is_refused_with_its_line(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = with_field(analyzer_csv(directory, flight(400, 0.05)), 2, "flight_id", "9" * 200_000)
            with self.assertRaisesRegex(ValueError, ": after line 3: field larger than field limit"):
                load_samples(path)


class TrimTests(unittest.TestCase):
    def rows(self) -> list[dict[str, float]]:
        with tempfile.TemporaryDirectory() as directory:
            return load_samples(analyzer_csv(directory, flight(400, 0.5)))

    def test_a_trim_must_be_finite_and_non_negative(self) -> None:
        for option, start, end in (
            ("--trim-start", float("inf"), 0.0),
            ("--trim-start", float("nan"), 0.0),
            ("--trim-end", 0.0, -1.0),
            ("--trim-end", 0.0, float("inf")),
        ):
            with self.subTest(start=start, end=end):
                with self.assertRaisesRegex(ValueError, option):
                    prepare_rows(self.rows(), 400.0, start, end)

    def test_a_huge_rate_and_trim_remove_everything_rather_than_overflow(self) -> None:
        with self.assertRaisesRegex(ValueError, "trim would remove all samples"):
            prepare_rows(self.rows(), 1e308, 10.0, 0.0)


class CommandErrorTests(unittest.TestCase):
    """Each refusal is an `error:` line and exit 1, never a traceback, and
    writes nothing."""

    def assert_refused(self, result: subprocess.CompletedProcess[str], out_dir: Path, says: str) -> None:
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("error:", result.stderr)
        self.assertIn(says, result.stderr)
        self.assertNotIn("Traceback", result.stderr)
        self.assertFalse(out_dir.exists())

    def test_malformed_input_and_options(self) -> None:
        cases = (
            ("seq", "inf", (), "seq 'inf' is not a whole number"),
            ("flight_id", "9" * 200_000, (), "field larger than field limit"),
            (None, None, ("--trim-start", "inf"), "--trim-start"),
            (None, None, ("--sample-rate", "1e308", "--trim-start", "10"), "trim would remove all samples"),
        )
        for column, value, options, says in cases:
            with self.subTest(column=column, options=options):
                with tempfile.TemporaryDirectory() as directory:
                    source = analyzer_csv(directory, flight(400, 1.5))
                    if column is not None:
                        with_field(source, 4, column, value)
                    out_dir = Path(directory) / "report"
                    result = run_report(str(source), "--out-dir", str(out_dir), *options)
                    self.assert_refused(result, out_dir, says)

    def test_without_matplotlib_nothing_is_written(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = analyzer_csv(directory, flight(400, 1.5))
            out_dir = Path(directory) / "report"
            result = run_report(str(source), "--out-dir", str(out_dir), block_matplotlib=True)
            self.assert_refused(result, out_dir, "matplotlib")


if __name__ == "__main__":
    unittest.main()
