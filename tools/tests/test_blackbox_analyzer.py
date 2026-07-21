import unittest
import binascii
import struct

from tools.blackbox_analyzer import (
    BlackboxSample,
    samples_from_flash_bytes,
    sequence_stats,
    u32_forward_delta,
)


def sample(seq: int, imu_seq: int) -> BlackboxSample:
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
        self.assertEqual(samples[0].imu_seq, 110)
        self.assertEqual(samples[0].motors, (100, 101, 102, 103))


if __name__ == "__main__":
    unittest.main()
