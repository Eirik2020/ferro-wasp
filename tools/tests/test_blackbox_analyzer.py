import tempfile
import unittest
import math
from dataclasses import replace
from pathlib import Path

from tools.blackbox_analyzer import (
    BlackboxSample,
    centered_drift_stats,
    longest_flight_window,
    measured_control_rate_hz,
    samples_from_file,
    sequence_stats,
    strongest_frequency,
    timestamp_stats,
    u32_forward_delta,
)


def sample(seq: int, imu_seq: int, timestamp_us: int | None = None) -> BlackboxSample:
    return BlackboxSample(
        version=2,
        seq=seq,
        imu_seq=imu_seq,
        flags=2,
        raw_dps=(0.0, 0.0, 0.0),
        gyro_dps=(0.0, 0.0, 0.0),
        command_dps=(0.0, 0.0, 0.0),
        pid=(0, 0, 0),
        throttle=0,
        motors=(0, 0, 0, 0),
        timestamp_us=timestamp_us,
    )


class SequenceStatsTests(unittest.TestCase):
    def test_reports_one_khz_pattern_at_four_hundred_hz_control(self) -> None:
        samples = [
            sample(10, 100),
            sample(11, 102),
            sample(12, 105),
            sample(13, 107),
            sample(14, 110),
        ]

        stats = sequence_stats(samples)

        self.assertAlmostEqual(stats.estimated_imu_rate_hz, 1000.0)
        self.assertEqual(stats.contiguous_imu_delta_counts, ((2, 2), (3, 2)))
        self.assertEqual(stats.repeated_imu_samples, 0)

    def test_rtt_gap_does_not_look_like_an_imu_gap(self) -> None:
        samples = [sample(20, 200), sample(21, 202), sample(24, 210)]

        stats = sequence_stats(samples)

        self.assertEqual(stats.missing_blackbox_frames, 2)
        self.assertAlmostEqual(stats.estimated_imu_rate_hz, 1000.0)
        self.assertEqual(stats.contiguous_imu_delta_counts, ((2, 1),))

    def test_counts_repeated_imu_sample_on_contiguous_control_pair(self) -> None:
        stats = sequence_stats([sample(30, 300), sample(31, 300)])

        self.assertEqual(stats.repeated_imu_samples, 1)
        self.assertEqual(stats.contiguous_imu_delta_counts, ((0, 1),))

    def test_u32_delta_wraps(self) -> None:
        self.assertEqual(u32_forward_delta(0xFFFF_FFFF, 1), 2)

    def test_reports_flash_control_timestamp_jitter(self) -> None:
        stats = timestamp_stats(
            [
                sample(1, 10, 0xFFFF_F000),
                sample(2, 12, 0xFFFF_F9C4),
                sample(3, 15, 0x0000_0388),
                sample(4, 17, 0x0000_0D50),
            ]
        )

        self.assertIsNotNone(stats)
        assert stats is not None
        self.assertEqual(stats.contiguous_pairs, 3)
        self.assertEqual(stats.min_interval_us, 2500)
        self.assertEqual(stats.max_interval_us, 2504)
        self.assertEqual(stats.p99_abs_jitter_us, 4)
        self.assertEqual(stats.intervals_over_3000_us, 0)

    def test_reads_one_flight_from_a_configurator_csv(self) -> None:
        header = (
            "time_s,seq,imu_seq,timestamp_ms,armed,imu_fresh,"
            "gyro_raw_roll_dps,gyro_raw_pitch_dps,gyro_raw_yaw_dps,"
            "gyro_roll_dps,gyro_pitch_dps,gyro_yaw_dps,"
            "setpoint_roll_dps,setpoint_pitch_dps,setpoint_yaw_dps,"
            "pid_roll_counts,pid_pitch_counts,pid_yaw_counts,"
            "throttle_counts,motor1_counts,motor2_counts,motor3_counts,motor4_counts"
        )
        row = "0.001000,44,110,123.000,1,0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,10,-11,12,321,100,101,102,103"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "flight.csv"
            path.write_text(f"{header}\n{row}\n", encoding="utf-8")
            samples = samples_from_file(path)

        self.assertEqual(len(samples), 1)
        value = samples[0]
        self.assertEqual((value.seq, value.imu_seq, value.timestamp_us), (44, 110, 123_000))
        self.assertTrue(value.armed)
        self.assertFalse(value.imu_fresh)
        self.assertEqual(value.raw_dps, (0.1, 0.2, 0.3))
        self.assertEqual(value.command_dps, (0.7, 0.8, 0.9))
        self.assertEqual(value.pid, (10, -11, 12))
        self.assertEqual((value.throttle, value.motors), (321, (100, 101, 102, 103)))

    def test_reads_the_shared_fixture_as_the_configurator_exported_it(self) -> None:
        # fixtures/blackbox/SOURCE.txt: a real Foxeer flight at 2 kHz, divisor 2.
        path = Path(__file__).resolve().parents[2] / "fixtures/blackbox/foxeer-f405-2khz.csv"
        samples = samples_from_file(path)

        self.assertEqual(len(samples), 2_000)
        self.assertEqual(round(measured_control_rate_hz(samples)), 2_000)
        self.assertTrue(all(sample.armed for sample in samples))

    def test_a_raw_onboard_log_points_at_the_configurator(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "flight-3.fwbb"
            path.write_bytes(bytes(256))
            with self.assertRaisesRegex(OSError, "ferro-configurator convert flight-3.fwbb --csv"):
                samples_from_file(path)
            other = Path(directory) / "other.csv"
            other.write_text("a,b\n1,2\n", encoding="utf-8")
            with self.assertRaisesRegex(OSError, "not a FerroConfigurator flight CSV"):
                samples_from_file(other)

    def test_centered_drift_reports_signed_mean_rate(self) -> None:
        base = replace(
            sample(1, 10, 0),
            flags=3,
            throttle=600,
            gyro_dps=(-2.0, 4.0, 1.0),
            flight_id=7,
        )
        samples = [
            base,
            replace(
                base,
                seq=2,
                timestamp_us=2_500,
                gyro_dps=(-4.0, 6.0, 3.0),
            ),
            replace(
                base,
                seq=3,
                timestamp_us=5_000,
                gyro_dps=(-6.0, 8.0, 5.0),
            ),
        ]

        stats = centered_drift_stats(samples, settle_seconds=0.0)

        self.assertIsNotNone(stats)
        assert stats is not None
        self.assertEqual(stats.sample_count, 3)
        self.assertAlmostEqual(stats.axes[0].mean_dps, -4.0)
        self.assertAlmostEqual(stats.axes[1].mean_dps, 6.0)
        self.assertAlmostEqual(stats.axes[2].mean_dps, 3.0)
        self.assertAlmostEqual(stats.integrated_duration_s, 0.005)
        self.assertAlmostEqual(stats.axes[0].signed_rotation_degrees, -0.02)

    def test_centered_drift_rejects_commands_and_low_throttle(self) -> None:
        eligible = replace(
            sample(1, 10, 0),
            flags=3,
            throttle=600,
            gyro_dps=(1.0, 2.0, 3.0),
        )
        samples = [
            eligible,
            replace(eligible, seq=2, command_dps=(0.0, 0.0, 1.0)),
            replace(eligible, seq=3, throttle=499),
            replace(eligible, seq=4, flags=2),
        ]

        stats = centered_drift_stats(samples, settle_seconds=0.0)

        self.assertIsNotNone(stats)
        assert stats is not None
        self.assertEqual(stats.sample_count, 1)
        self.assertEqual(stats.axes[0].mean_dps, 1.0)

    def test_centered_drift_waits_after_command_returns_to_center(self) -> None:
        base = replace(
            sample(1, 10, 0),
            flags=3,
            throttle=600,
            gyro_dps=(10.0, 0.0, 0.0),
        )
        samples = [
            replace(base, command_dps=(1.0, 0.0, 0.0)),
            replace(base, seq=2, timestamp_us=2_500),
            replace(base, seq=3, timestamp_us=5_000),
            replace(base, seq=4, timestamp_us=7_500, gyro_dps=(1.0, 0.0, 0.0)),
        ]

        stats = centered_drift_stats(samples, settle_seconds=2.0 / 400.0)

        self.assertIsNotNone(stats)
        assert stats is not None
        self.assertEqual(stats.sample_count, 1)
        self.assertEqual(stats.axes[0].mean_dps, 1.0)

    def test_longest_flight_window_excludes_ground_and_short_throttle_runs(self) -> None:
        base = replace(sample(1, 10, 0), flags=3, throttle=600, flight_id=4)
        samples = [
            replace(base, seq=1, flags=2, throttle=0),
            replace(base, seq=2),
            replace(base, seq=3),
            replace(base, seq=4, throttle=400),
            replace(base, seq=5),
            replace(base, seq=6),
            replace(base, seq=7),
        ]

        selected = longest_flight_window(samples, 500)

        self.assertEqual([value.seq for value in selected], [5, 6, 7])

    def test_frequency_search_retains_high_frequency_coverage_on_long_logs(self) -> None:
        sample_rate_hz = 400.0
        values = [
            math.sin(2.0 * math.pi * 12.0 * index / sample_rate_hz)
            for index in range(8_000)
        ]

        peak = strongest_frequency(values, sample_rate_hz)

        self.assertIsNotNone(peak)
        assert peak is not None
        self.assertAlmostEqual(peak[0], 12.0, delta=0.3)


class ControlRateTests(unittest.TestCase):
    def test_the_rate_comes_from_the_logs_own_millisecond_timestamps(self) -> None:
        # Every second record of a 2 kHz loop, stamped to the whole millisecond.
        samples = [sample(seq, seq, (seq * 500) // 1000 * 1000) for seq in range(0, 8000, 2)]

        self.assertAlmostEqual(measured_control_rate_hz(samples), 2000.0, delta=1.0)

    def test_a_timestamp_wrap_still_measures(self) -> None:
        start = 0xFFFF_FFFF - 400_000
        samples = [
            sample(seq, seq, (start + seq * 2500) & 0xFFFF_FFFF) for seq in range(0, 1000)
        ]

        self.assertAlmostEqual(measured_control_rate_hz(samples), 400.0, delta=0.5)

    def test_without_a_second_of_timestamps_there_is_no_measurement(self) -> None:
        self.assertIsNone(measured_control_rate_hz([sample(seq, seq) for seq in range(100)]))
        self.assertIsNone(
            measured_control_rate_hz([sample(seq, seq, seq * 500) for seq in range(100)])
        )


if __name__ == "__main__":
    unittest.main()
