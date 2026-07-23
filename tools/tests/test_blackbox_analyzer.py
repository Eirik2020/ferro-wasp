import unittest
import binascii
import struct

from tools.blackbox_analyzer import (
    BlackboxSample,
    flash_log_stats,
    samples_from_flash_bytes,
    select_flight_samples,
    sequence_stats,
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

    def test_reads_crc_valid_onboard_flash_page(self) -> None:
        page = bytearray(b"\xff" * 256)
        record = struct.pack(
            "<IIIH3h3h3h3hH4H",
            123_000,
            44,
            110,
            3,
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            321,
            100,
            101,
            102,
            103,
        )
        page[:48] = record
        page[240:252] = b"FB\x01\x01" + (7).to_bytes(4, "little") + (9).to_bytes(4, "little")
        page[252:] = binascii.crc32(page[:252]).to_bytes(4, "little")
        samples = samples_from_flash_bytes(bytes(page))

        self.assertEqual(len(samples), 1)
        self.assertEqual(samples[0].seq, 44)
        self.assertEqual(samples[0].timestamp_us, 123_000)
        self.assertEqual(samples[0].flight_id, 7)
        self.assertEqual(samples[0].imu_seq, 110)
        self.assertEqual(samples[0].motors, (100, 101, 102, 103))
        stats = flash_log_stats(bytes(page))
        self.assertEqual(stats.page_count, 1)
        self.assertEqual(stats.record_count, 1)
        self.assertEqual(stats.flight_ids, (7,))
        self.assertEqual(stats.first_page_sequence, 9)
        self.assertEqual(stats.last_page_sequence, 9)
        self.assertEqual(stats.partial_page_count, 1)
        self.assertEqual(stats.final_page_records, 1)

    def test_selects_latest_onboard_flight(self) -> None:
        samples = [
            sample(1, 10),
            BlackboxSample(**{**sample(2, 12).__dict__, "flight_id": 3}),
            BlackboxSample(**{**sample(3, 15).__dict__, "flight_id": 4}),
        ]

        selected, flight_id = select_flight_samples(samples, "latest")

        self.assertEqual(flight_id, 4)
        self.assertEqual([value.seq for value in selected], [3])

    def test_rejects_unavailable_onboard_flight(self) -> None:
        samples = [BlackboxSample(**{**sample(2, 12).__dict__, "flight_id": 3})]

        with self.assertRaisesRegex(ValueError, "available IDs: 3"):
            select_flight_samples(samples, "9")

    def test_rejects_crc_invalid_onboard_flash_page(self) -> None:
        page = bytearray(b"\xff" * 256)
        page[240:252] = (
            b"FB\x01\x00" + (1).to_bytes(4, "little") + (0).to_bytes(4, "little")
        )
        page[252:] = binascii.crc32(page[:252]).to_bytes(4, "little")
        page[0] ^= 1

        with self.assertRaisesRegex(OSError, "CRC mismatch"):
            samples_from_flash_bytes(bytes(page))

    def test_rejects_non_page_data_in_onboard_flash_download(self) -> None:
        with self.assertRaisesRegex(OSError, "invalid flash page magic"):
            samples_from_flash_bytes(bytes(256))


if __name__ == "__main__":
    unittest.main()
