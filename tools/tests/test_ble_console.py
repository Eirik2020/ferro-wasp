"""Host tests for the SpeedyBee Bluetooth bridge console.

The wire facts the codec is tested against were measured on a production
board: the module's own MSPv2 poll frame and the API_VERSION request that the
flight controller answered over the air. The BlueZ layer needs hardware and
stays untested here.
"""

import unittest

from tools.ble_console import (
    MspStream,
    crc8_dvb_s2,
    decode_api_version,
    decode_boxids,
    decode_fc_version,
    decode_status,
    decode_uid,
    describe_response,
    encode_request,
    parse_hex,
)

# The bridge module's own MSPv2 BOXIDS poll (function 0x77 = 119), captured
# off the UART: length 1, zero payload, CRC 0xFD.
CAPTURED_BRIDGE_POLL = "24 58 3C 00 77 00 01 00 00 FD"
# The API_VERSION request the flight controller answered over Bluetooth.
CAPTURED_VERSION_REQUEST = "24 58 3C 00 01 00 00 00 45"


def response_frame(function: int, payload: bytes) -> bytes:
    """A well-formed $X> frame, the way a controller sends one."""
    import struct

    body = struct.pack("<BHH", 0, function, len(payload)) + payload
    return b"$X>" + body + bytes([crc8_dvb_s2(body)])


class CrcTest(unittest.TestCase):
    def test_matches_the_modules_captured_poll(self):
        frame = bytes.fromhex(CAPTURED_BRIDGE_POLL)
        self.assertEqual(crc8_dvb_s2(frame[3:-1]), frame[-1])

    def test_an_xor_checksum_is_not_accepted(self):
        body = bytes.fromhex("00 77 00 01 00 00")
        self.assertNotEqual(crc8_dvb_s2(body), 0x76)  # the XOR answer


class EncodeRequestTest(unittest.TestCase):
    def test_matches_the_frame_the_controller_answered(self):
        self.assertEqual(encode_request(1).hex().upper(), CAPTURED_VERSION_REQUEST.replace(" ", ""))

    def test_rejects_functions_that_do_not_fit(self):
        with self.assertRaises(ValueError):
            encode_request(0x10000)

    def test_rejects_oversized_payloads(self):
        with self.assertRaises(ValueError):
            encode_request(1, b"\x00" * 1025)

    def test_carries_the_payload(self):
        frame = encode_request(119, b"\x00")
        self.assertEqual(frame, bytes.fromhex("24 58 3C 00 77 00 01 00 00 FD"))

    def test_uid_and_boxids_have_distinct_function_numbers(self):
        # 160 is MSP_UID; 119 is MSP_BOXIDS, the command the module polls.
        self.assertNotEqual(encode_request(160).hex(), encode_request(119).hex())


class MspStreamTest(unittest.TestCase):
    def test_round_trips_one_frame(self):
        stream = MspStream()
        self.assertEqual(stream.feed(response_frame(1, b"\x00\x01.")), [(1, b"\x00\x01.")])

    def test_reassembles_across_single_byte_chunks(self):
        stream = MspStream()
        frame = response_frame(100, bytes(range(40)))
        seen = []
        for byte in frame:
            seen.extend(stream.feed(bytes([byte])))
        self.assertEqual(seen, [(100, bytes(range(40)))])

    def test_resynchronises_after_garbage(self):
        stream = MspStream()
        noise = b"some uart garbage without meaning\x00\x24X"
        frames = stream.feed(noise + response_frame(119, b"\x01" * 12))
        self.assertEqual(frames, [(119, b"\x01" * 12)])

    def test_drops_a_corrupt_frame_whole_and_keeps_going(self):
        stream = MspStream()
        good = response_frame(1, b"\x00\x01\x2e")
        corrupt = bytearray(good)
        corrupt[-1] ^= 0xFF
        frames = stream.feed(bytes(corrupt) + good)
        self.assertEqual(frames, [(1, b"\x00\x01\x2e")])

    def test_returns_two_frames_in_one_chunk(self):
        stream = MspStream()
        first = response_frame(1, b"\x00\x01\x2e")
        second = response_frame(3, b"BTFL" + bytes([4, 5, 2]))
        self.assertEqual(stream.feed(first + second), [
            (1, b"\x00\x01\x2e"),
            (3, b"BTFL\x04\x05\x02"),
        ])

    def test_a_truncated_frame_waits_for_the_rest(self):
        stream = MspStream()
        frame = response_frame(119, b"\x00" * 12)
        cut = len(frame) - 4
        self.assertEqual(stream.feed(frame[:cut]), [])
        self.assertEqual(stream.feed(frame[cut:]), [(119, b"\x00" * 12)])


class DecodeTest(unittest.TestCase):
    def test_api_version(self):
        self.assertEqual(decode_api_version(b"\x00\x01\x2e"), "MSP protocol 0, API 1.46")

    def test_fc_version_v2_form_is_the_semver_alone(self):
        self.assertEqual(decode_fc_version(bytes([4, 5, 2])), "4.5.2")

    def test_fc_version_v1_form_keeps_the_identifier(self):
        answer = b"BTFL" + bytes([4, 5, 2]) + b"abcdef" + b"\x00extra"
        self.assertEqual(decode_fc_version(answer), "BTFL 4.5.2 (abcdef)")

    def test_uid(self):
        self.assertEqual(decode_uid(bytes(range(12))), "0x03020100 0x07060504 0x0b0a0908")

    def test_boxids_lists_the_ids(self):
        answer = bytes([0, 1, 2, 6, 27, 7, 8])
        self.assertEqual(decode_boxids(answer), "box ids 0 1 2 6 27 7 8")

    def test_status_decodes_the_betaflight_45_prefix(self):
        answer = (
            (5000).to_bytes(2, "little")
            + (3).to_bytes(2, "little")
            + (0x23).to_bytes(2, "little")
            + (0).to_bytes(4, "little")
            + bytes([2])
            + (26).to_bytes(2, "little")
            + b"\x01\x02"
        )
        self.assertEqual(
            decode_status(answer),
            "cycle time 5000 us, i2c errors 3, sensors 0x23, flags 0x00000000,"
            " pid profile 2, cpu load 26%, tail 0102",
        )

    def test_short_answers_are_rejected(self):
        for decoder in (decode_api_version, decode_fc_version, decode_uid, decode_status):
            with self.assertRaises(ValueError):
                decoder(b"\x00\x01")

    def test_unknown_functions_are_shown_as_hex(self):
        self.assertEqual(describe_response(230, b"\xaa"), "function 230: aa")


class ParseHexTest(unittest.TestCase):
    def test_accepts_plain_hex(self):
        self.assertEqual(parse_hex("2405", "data"), b"\x24\x05")

    def test_accepts_separators(self):
        self.assertEqual(parse_hex("24:05 6e", "data"), b"\x24\x05\x6e")

    def test_rejects_partial_bytes(self):
        with self.assertRaises(SystemExit):
            parse_hex("24f", "data")


if __name__ == "__main__":
    unittest.main()
