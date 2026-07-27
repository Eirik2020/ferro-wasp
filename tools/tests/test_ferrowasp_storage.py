import binascii
import unittest

from tools.ferrowasp_storage import PAGE_SIZE, validated_resume_page_count_from_bytes
from tools.ferrowasp_storage import (
    BOOT_SESSION_START_FLAG,
    FlightSpan,
    boot_session_numbers,
    catalog_flights,
    parse_log_info_line,
    resolve_flight_span,
    validated_flight_resume_page_count,
    validated_page_metadata,
)


def valid_page(
    flight_id: int = 1,
    page_sequence: int = 0,
    *,
    boot_session_start: bool = False,
) -> bytes:
    page = bytearray(b"\xff" * PAGE_SIZE)
    page[12:14] = (
        BOOT_SESSION_START_FLAG if boot_session_start else 0
    ).to_bytes(2, "little")
    page[240:252] = (
        b"FB\x01\x01"
        + flight_id.to_bytes(4, "little")
        + page_sequence.to_bytes(4, "little")
    )
    page[-4:] = binascii.crc32(page[:-4]).to_bytes(4, "little")
    return bytes(page)


class ResumeDownloadTests(unittest.TestCase):
    def test_empty_download_starts_at_zero(self) -> None:
        self.assertEqual(validated_resume_page_count_from_bytes(b"", 4), 0)

    def test_accepts_crc_valid_complete_pages(self) -> None:
        self.assertEqual(
            validated_resume_page_count_from_bytes(valid_page() * 2, 4), 2
        )

    def test_rejects_crc_invalid_page(self) -> None:
        page = bytearray(valid_page())
        page[0] ^= 1
        with self.assertRaisesRegex(RuntimeError, "CRC mismatch"):
            validated_resume_page_count_from_bytes(bytes(page), 4)

    def test_rejects_trailing_partial_page(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "trailing bytes"):
            validated_resume_page_count_from_bytes(valid_page() + b"x", 4)


class LogInfoTests(unittest.TestCase):
    def test_accepts_bounded_and_legacy_field_names(self) -> None:
        self.assertEqual(
            parse_log_info_line(b"OK u=55316 n=38 t=65488 w=1"),
            (55316, 38, 65488, True),
        )
        self.assertEqual(
            parse_log_info_line(
                b"OK used_pages=551 next_flight=2 total_pages=65488 writable=0"
            ),
            (551, 2, 65488, False),
        )

    def test_recovers_legacy_response_concatenated_with_status(self) -> None:
        self.assertEqual(
            parse_log_info_line(
                b"OK used_pages=55316 next_flight=38 total_pages=65488 writable=1"
                b"FWDBG1 ms=44032 imu=icm42688p ready=1"
            ),
            (55316, 38, 65488, True),
        )


class FlightSelectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.pages = [
            valid_page(1, 0),
            valid_page(1, 1),
            valid_page(2, 0),
            valid_page(2, 1),
            valid_page(3, 0, boot_session_start=True),
            valid_page(3, 1),
            valid_page(4, 0),
            valid_page(5, 0, boot_session_start=True),
        ]

    def read(self, page_index: int) -> bytes:
        return self.pages[page_index]

    def test_resolves_latest_flight_without_scanning_every_page(self) -> None:
        span = resolve_flight_span(len(self.pages), "latest", self.read)

        self.assertEqual(
            span,
            FlightSpan(
                flight_id=5,
                start_page=7,
                end_page=8,
                boot_session_start=True,
            ),
        )

    def test_resolves_numeric_flight_bounds(self) -> None:
        span = resolve_flight_span(len(self.pages), "2", self.read)

        self.assertEqual(span.flight_id, 2)
        self.assertEqual((span.start_page, span.end_page), (2, 4))
        self.assertFalse(span.boot_session_start)

    def test_rejects_unavailable_flight(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "flight ID 9 is not present"):
            resolve_flight_span(len(self.pages), "9", self.read)

    def test_catalog_groups_new_flights_by_explicit_boot_markers(self) -> None:
        spans = catalog_flights(len(self.pages), self.read)

        self.assertEqual([span.flight_id for span in spans], [1, 2, 3, 4, 5])
        self.assertEqual(boot_session_numbers(spans), [None, None, 1, 1, 2])

    def test_metadata_reads_boot_marker_from_first_record(self) -> None:
        metadata = validated_page_metadata(self.pages[4])

        self.assertEqual(metadata.flight_id, 3)
        self.assertTrue(metadata.boot_session_start)

    def test_selective_resume_requires_matching_flight_and_sequence(self) -> None:
        import tempfile
        from pathlib import Path

        span = FlightSpan(2, 2, 4, False)
        with tempfile.NamedTemporaryFile(
            dir=Path(__file__).parent,
            delete=False,
        ) as output:
            path = Path(output.name)
            output.write(self.pages[2])
        try:
            self.assertEqual(validated_flight_resume_page_count(path, span), 1)
            path.write_bytes(self.pages[0])
            with self.assertRaisesRegex(RuntimeError, "belongs to flight 1"):
                validated_flight_resume_page_count(path, span)
        finally:
            path.unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
