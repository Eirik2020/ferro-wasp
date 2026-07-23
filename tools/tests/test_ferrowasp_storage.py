import binascii
import unittest

from tools.ferrowasp_storage import PAGE_SIZE, validated_resume_page_count_from_bytes


def valid_page() -> bytes:
    page = bytearray(b"\xff" * PAGE_SIZE)
    page[240:252] = b"FB\x01\x00" + (1).to_bytes(4, "little") + (0).to_bytes(4, "little")
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


if __name__ == "__main__":
    unittest.main()
