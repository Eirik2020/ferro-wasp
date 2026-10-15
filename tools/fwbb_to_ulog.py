#!/usr/bin/env python3
"""Convert one FerroWasp onboard BB2 flight into a minimal ULog file."""

from __future__ import annotations

import argparse
import math
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from tools.blackbox_analyzer import (
    BlackboxSample,
    flash_log_stats,
    samples_from_flash_bytes,
    select_flight_samples,
    u32_forward_delta,
)

ULOG_MAGIC = b"ULog\x01\x12\x35"
ULOG_VERSION = 1
SCHEMA_VERSION = 1
TOPIC_NAME = "ferrowasp_rate_control"
TOPIC_MESSAGE_ID = 0
MAX_REASONABLE_INTERVAL_US = 60_000_000
# The shortest flight whose control rate is measured: millisecond timestamps
# then cost well under a tick. Rates round to 10 Hz. Both match
# FerroConfigurator's exports.
MIN_MEASURED_DURATION_US = 2_000_000
RATE_ROUNDING_HZ = 10

ULOG_HEADER = struct.Struct("<7sBQ")
MESSAGE_HEADER = struct.Struct("<HB")
DATA_PREFIX = struct.Struct("<H")
DATA_RECORD = struct.Struct("<QIII17f3B")

TOPIC_FORMAT = (
    f"{TOPIC_NAME}:"
    "uint64_t timestamp;"
    "uint32_t flight_id;"
    "uint32_t seq;"
    "uint32_t imu_seq;"
    "float raw_roll_dps;"
    "float raw_pitch_dps;"
    "float raw_yaw_dps;"
    "float gyro_roll_dps;"
    "float gyro_pitch_dps;"
    "float gyro_yaw_dps;"
    "float setpoint_roll_dps;"
    "float setpoint_pitch_dps;"
    "float setpoint_yaw_dps;"
    "float pid_roll;"
    "float pid_pitch;"
    "float pid_yaw;"
    "float throttle_command;"
    "float motor1_command;"
    "float motor2_command;"
    "float motor3_command;"
    "float motor4_command;"
    "uint8_t armed;"
    "uint8_t imu_fresh;"
    "uint8_t flags;"
    "uint8_t _padding0;"
)


@dataclass(frozen=True)
class ConversionSummary:
    flight_id: int
    sample_count: int
    duration_us: int
    dropout_count: int
    output_bytes: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Convert one CRC-validated FerroWasp .fwbb flight to a minimal "
            "PlotJuggler/PyULog-compatible .ulg file."
        )
    )
    parser.add_argument("input", type=Path, help="Downloaded onboard .fwbb archive.")
    parser.add_argument(
        "--flight-id",
        default="latest",
        help="Flight ID to convert, or 'latest' (default: latest).",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Output .ulg path. Defaults beside the input with the flight ID appended.",
    )
    return parser.parse_args()


def ulog_message(message_type: str, payload: bytes) -> bytes:
    if len(message_type) != 1 or ord(message_type) > 0x7F:
        raise ValueError("ULog message type must be one ASCII character")
    if len(payload) > 0xFFFF:
        raise ValueError("ULog message payload exceeds uint16 length")
    return MESSAGE_HEADER.pack(len(payload), ord(message_type)) + payload


def info_string(name: str, value: str) -> bytes:
    encoded_value = value.encode("utf-8")
    key = f"char[{len(encoded_value)}] {name}".encode("ascii")
    if len(key) > 0xFF:
        raise ValueError("ULog info key exceeds uint8 length")
    return ulog_message("I", bytes((len(key),)) + key + encoded_value)


def info_u32(name: str, value: int) -> bytes:
    key = f"uint32_t {name}".encode("ascii")
    if len(key) > 0xFF:
        raise ValueError("ULog info key exceeds uint8 length")
    return ulog_message("I", bytes((len(key),)) + key + struct.pack("<I", value))


def elapsed_timestamps(samples: list[BlackboxSample]) -> list[int]:
    if not samples:
        return []
    if samples[0].timestamp_us is None:
        raise ValueError("BB2 sample has no control timestamp")

    elapsed = [0]
    previous = samples[0].timestamp_us
    total = 0
    for sample in samples[1:]:
        if sample.timestamp_us is None:
            raise ValueError("BB2 sample has no control timestamp")
        interval = u32_forward_delta(previous, sample.timestamp_us)
        # Two records can share a millisecond stamp; only a leap is invalid.
        if interval > MAX_REASONABLE_INTERVAL_US:
            raise ValueError(
                f"invalid BB2 control timestamp interval {interval} us "
                f"at sequence {sample.seq}"
            )
        total += interval
        elapsed.append(total)
        previous = sample.timestamp_us
    return elapsed


def encode_data_record(
    sample: BlackboxSample, timestamp_us: int, selected_flight_id: int
) -> bytes:
    raw = sample.raw_dps
    if raw is None:
        raw = (math.nan, math.nan, math.nan)
    floats: tuple[float, ...] = (
        *raw,
        *sample.gyro_dps,
        *sample.command_dps,
        *(float(value) for value in sample.pid),
        float(sample.throttle),
        *(float(value) for value in sample.motors),
    )
    return DATA_RECORD.pack(
        timestamp_us,
        selected_flight_id,
        sample.seq,
        sample.imu_seq,
        *floats,
        int(sample.armed),
        int(sample.imu_fresh),
        sample.flags & 0xFF,
    )


@dataclass(frozen=True)
class Timing:
    """How a flight's records sit in time."""

    # Control ticks per second, rounded to 10 Hz; None under 2 s of log.
    control_rate_hz: int | None
    # Control ticks between consecutive records: the log rate divisor.
    record_step: int


def measure_timing(samples: list[BlackboxSample], elapsed: list[int]) -> Timing:
    """The control rate over the whole flight, and the usual record step.

    The log stores no rate, and its timestamps tick in whole milliseconds,
    so the rate comes from the span of the whole flight.
    """
    steps: dict[int, int] = {}
    ticks = 0
    for previous, current in zip(samples, samples[1:]):
        step = u32_forward_delta(previous.seq, current.seq)
        if step == 0:
            raise ValueError(f"BB2 control sequence {current.seq} repeats")
        ticks += step
        steps[step] = steps.get(step, 0) + 1
    # The usual step is the divisor; the smallest wins a tie.
    record_step = min(steps, key=lambda step: (-steps[step], step)) if steps else 1
    rate = None
    if elapsed and elapsed[-1] >= MIN_MEASURED_DURATION_US:
        measured = ticks * 1_000_000 / elapsed[-1]
        rate = round(measured / RATE_ROUNDING_HZ) * RATE_ROUNDING_HZ
    return Timing(control_rate_hz=rate, record_step=record_step)


def build_ulog(
    samples: Iterable[BlackboxSample], selected_flight_id: int
) -> tuple[bytes, ConversionSummary]:
    selected = list(samples)
    if not selected:
        raise ValueError("selected flight contains no BB2 samples")
    millisecond_clock = elapsed_timestamps(selected)
    timing = measure_timing(selected, millisecond_clock)
    # Timestamps count control ticks at the measured rate; a flight too short
    # to measure keeps the records' own milliseconds.
    if timing.control_rate_hz is None:
        timestamps = millisecond_clock
    else:
        timestamps = [0]
        ticks = 0
        for previous, current in zip(selected, selected[1:]):
            ticks += u32_forward_delta(previous.seq, current.seq)
            timestamps.append(ticks * 1_000_000 // timing.control_rate_hz)

    output = bytearray(ULOG_HEADER.pack(ULOG_MAGIC, ULOG_VERSION, 0))
    output += ulog_message("B", bytes(40))
    output += ulog_message("F", TOPIC_FORMAT.encode("ascii"))
    output += info_string("sys_name", "FerroWasp")
    output += info_string("log_type", "converted_fwbb")
    output += info_string("source_format", "FerroWasp BB2")
    output += info_u32("schema_version", SCHEMA_VERSION)
    output += info_u32("flight_id", selected_flight_id)
    if timing.control_rate_hz is not None:
        output += info_u32("control_rate_hz", timing.control_rate_hz)
    output += ulog_message(
        "A",
        struct.pack("<BH", 0, TOPIC_MESSAGE_ID) + TOPIC_NAME.encode("ascii"),
    )

    dropout_count = 0
    for index, (sample, timestamp_us) in enumerate(zip(selected, timestamps)):
        if index:
            step = u32_forward_delta(selected[index - 1].seq, sample.seq)
            if step > timing.record_step:
                if timing.control_rate_hz is None:
                    missing_us = timestamp_us - timestamps[index - 1]
                else:
                    missing_us = (
                        (step - timing.record_step) * 1_000_000 // timing.control_rate_hz
                    )
                duration_ms = min(0xFFFF, max(1, round(missing_us / 1_000)))
                output += ulog_message("O", struct.pack("<H", duration_ms))
                dropout_count += 1
        record = encode_data_record(sample, timestamp_us, selected_flight_id)
        output += ulog_message("D", DATA_PREFIX.pack(TOPIC_MESSAGE_ID) + record)

    summary = ConversionSummary(
        flight_id=selected_flight_id,
        sample_count=len(selected),
        duration_us=timestamps[-1],
        dropout_count=dropout_count,
        output_bytes=len(output),
    )
    return bytes(output), summary


def convert_fwbb(
    input_path: Path, output_path: Path, requested_flight_id: str
) -> ConversionSummary:
    source = input_path.read_bytes()
    stats = flash_log_stats(source)
    if not stats.page_count:
        raise ValueError("input .fwbb archive contains no flash pages")
    samples = samples_from_flash_bytes(source)
    selected, flight_id = select_flight_samples(samples, requested_flight_id)
    if flight_id is None:
        raise ValueError("conversion requires one selected onboard flight")

    encoded, summary = build_ulog(selected, flight_id)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(encoded)
    return summary


def default_output_path(input_path: Path, requested_flight_id: str) -> Path:
    suffix = requested_flight_id.lower()
    return input_path.with_name(f"{input_path.stem}-flight-{suffix}.ulg")


def main() -> int:
    args = parse_args()
    output = args.output or default_output_path(args.input, args.flight_id)
    try:
        summary = convert_fwbb(args.input, output, args.flight_id)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    print(
        f"{output}: flight {summary.flight_id}, {summary.sample_count} samples, "
        f"{summary.duration_us / 1_000_000:.3f} s, "
        f"{summary.dropout_count} dropouts, {summary.output_bytes} bytes"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
