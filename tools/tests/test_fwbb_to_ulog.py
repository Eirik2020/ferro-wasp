import binascii
import struct
import unittest
from dataclasses import replace

from tools.blackbox_analyzer import BlackboxSample
from tools.fwbb_to_ulog import (
    DATA_RECORD,
    TOPIC_FORMAT,
    TOPIC_MESSAGE_ID,
    ULOG_HEADER,
    ULOG_MAGIC,
    build_ulog,
    convert_fwbb,
)


def sample(
    seq: int,
    timestamp_us: int,
    *,
    flight_id: int = 7,
) -> BlackboxSample:
    return BlackboxSample(
        version=2,
        seq=seq,
        imu_seq=(seq * 2) & 0xFFFF_FFFF,
        flags=3,
        raw_dps=(1.0, 2.0, 3.0),
        gyro_dps=(4.0, 5.0, 6.0),
        command_dps=(7.0, 8.0, 9.0),
        pid=(10, 11, 12),
        throttle=500,
        motors=(501, 502, 503, 504),
        timestamp_us=timestamp_us,
        flight_id=flight_id,
    )


def messages(data: bytes) -> list[tuple[str, bytes]]:
    magic, version, start_timestamp = ULOG_HEADER.unpack_from(data)
    if magic != ULOG_MAGIC or version != 1 or start_timestamp != 0:
        raise AssertionError("invalid test ULog header")
    result: list[tuple[str, bytes]] = []
    offset = ULOG_HEADER.size
    while offset < len(data):
        size, message_type = struct.unpack_from("<HB", data, offset)
        offset += 3
        payload = data[offset : offset + size]
        if len(payload) != size:
            raise AssertionError("truncated test ULog message")
        result.append((chr(message_type), payload))
        offset += size
    return result


def flash_page(records: list[BlackboxSample], flight_id: int) -> bytes:
    page = bytearray(b"\xff" * 256)
    record_struct = struct.Struct("<IIIH3h3h3h3hH4H")
    for index, value in enumerate(records):
        assert value.raw_dps is not None
        encoded = record_struct.pack(
            value.timestamp_us,
            value.seq,
            value.imu_seq,
            value.flags,
            *(round(axis * 10) for axis in value.raw_dps),
            *(round(axis * 10) for axis in value.gyro_dps),
            *(round(axis * 10) for axis in value.command_dps),
            *value.pid,
            value.throttle,
            *value.motors,
        )
        page[index * 48 : (index + 1) * 48] = encoded
    page[240:252] = (
        b"FB\x01"
        + bytes((len(records),))
        + flight_id.to_bytes(4, "little")
        + (0).to_bytes(4, "little")
    )
    page[252:] = binascii.crc32(page[:252]).to_bytes(4, "little")
    return bytes(page)


class ULogWriterTests(unittest.TestCase):
    def test_writes_required_header_flags_format_and_subscription(self) -> None:
        encoded, summary = build_ulog([sample(10, 100_000)], 7)
        parsed = messages(encoded)

        self.assertEqual(encoded[:7], ULOG_MAGIC)
        self.assertEqual(parsed[0], ("B", bytes(40)))
        self.assertIn(("F", TOPIC_FORMAT.encode("ascii")), parsed)
        subscription = next(payload for kind, payload in parsed if kind == "A")
        self.assertEqual(subscription[:3], struct.pack("<BH", 0, TOPIC_MESSAGE_ID))
        self.assertEqual(subscription[3:], b"ferrowasp_rate_control")
        self.assertEqual(summary.sample_count, 1)
        self.assertEqual(summary.duration_us, 0)

    def test_round_trips_topic_values_and_units(self) -> None:
        encoded, _ = build_ulog([sample(10, 100_000)], 7)
        payload = next(payload for kind, payload in messages(encoded) if kind == "D")
        message_id = struct.unpack_from("<H", payload)[0]
        values = DATA_RECORD.unpack_from(payload, 2)

        self.assertEqual(message_id, TOPIC_MESSAGE_ID)
        self.assertEqual(values[:4], (0, 7, 10, 20))
        self.assertEqual(values[4:21], tuple(float(value) for value in range(1, 13)) + (500.0, 501.0, 502.0, 503.0, 504.0))
        self.assertEqual(values[21:], (1, 1, 3))

    def test_unwraps_timestamp_and_sequence_wrap(self) -> None:
        values = [
            sample(0xFFFF_FFFF, 0xFFFF_FF00),
            sample(0, 0x0000_08C4),
        ]
        encoded, summary = build_ulog(values, 7)
        records = [
            DATA_RECORD.unpack_from(payload, 2)
            for kind, payload in messages(encoded)
            if kind == "D"
        ]

        self.assertEqual([record[0] for record in records], [0, 2_500])
        self.assertEqual(summary.duration_us, 2_500)
        self.assertEqual(summary.dropout_count, 0)

    def test_emits_dropout_for_missing_control_records(self) -> None:
        encoded, summary = build_ulog(
            [sample(10, 100_000), sample(13, 107_500)],
            7,
        )
        dropouts = [
            struct.unpack("<H", payload)[0]
            for kind, payload in messages(encoded)
            if kind == "O"
        ]

        self.assertEqual(dropouts, [5])
        self.assertEqual(summary.dropout_count, 1)

    def test_output_is_deterministic(self) -> None:
        values = [sample(10, 100_000), sample(11, 102_500)]
        first, _ = build_ulog(values, 7)
        second, _ = build_ulog(values, 7)
        self.assertEqual(first, second)

    def test_converts_only_requested_flash_flight(self) -> None:
        from pathlib import Path

        root = Path(__file__).parent
        source = root / ".fwbb-to-ulog-test-input.fwbb"
        output = root / ".fwbb-to-ulog-test-output.ulg"
        try:
            source.write_bytes(
                flash_page([sample(10, 100_000, flight_id=6)], 6)
                + flash_page([sample(20, 200_000, flight_id=7)], 7)
            )

            summary = convert_fwbb(source, output, "latest")
            data_messages = [
                payload
                for kind, payload in messages(output.read_bytes())
                if kind == "D"
            ]

            self.assertEqual(summary.flight_id, 7)
            self.assertEqual(summary.sample_count, 1)
            self.assertEqual(DATA_RECORD.unpack_from(data_messages[0], 2)[2], 20)
        finally:
            source.unlink(missing_ok=True)
            output.unlink(missing_ok=True)

    def test_rejects_missing_timestamp(self) -> None:
        with self.assertRaisesRegex(ValueError, "no control timestamp"):
            build_ulog([replace(sample(10, 100_000), timestamp_us=None)], 7)


if __name__ == "__main__":
    unittest.main()
