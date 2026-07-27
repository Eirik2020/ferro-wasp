#!/usr/bin/env python3
"""USB CDC CLI for FerroWasp onboard flash logs and configuration."""

from __future__ import annotations

import argparse
import binascii
import re
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

try:
    import serial
except ImportError:  # pragma: no cover - depends on the host environment
    serial = None


PAGE_LINE_RE = re.compile(rb"^PAGE (\d+) (\d{3}) ([0-9a-f]{32})$")
LIST_RE_LONG = re.compile(
    rb"^OK used_pages=(\d+) next_flight=(\d+) total_pages=(\d+) writable=([01])$"
)
LIST_RE_SHORT = re.compile(rb"^OK u=(\d+) n=(\d+) t=(\d+) w=([01])$")
PAGE_SIZE = 256
PAGE_DATA_SIZE = 240
RECORD_SIZE = 48
RECORDS_PER_PAGE = PAGE_DATA_SIZE // RECORD_SIZE
RECORD_FLAGS_OFFSET = 12
BOOT_SESSION_START_FLAG = 1 << 2
CONFIG_KEYS = (
    "roll_p",
    "roll_i",
    "roll_d",
    "pitch_p",
    "pitch_i",
    "pitch_d",
    "yaw_p",
    "yaw_i",
    "yaw_d",
    "imu_lpf_alpha",
    "log_rate_divisor",
    "rc_deadband",
    "roll_center_rate",
    "roll_max_rate",
    "roll_expo",
    "pitch_center_rate",
    "pitch_max_rate",
    "pitch_expo",
    "yaw_center_rate",
    "yaw_max_rate",
    "yaw_expo",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", required=True, help="USB CDC serial port, for example COM7")
    parser.add_argument("--baud", type=int, default=115200, help="Nominal CDC line rate")
    parser.add_argument("--timeout", type=float, default=3.0, help="Response timeout in seconds")
    subparsers = parser.add_subparsers(dest="operation", required=True)

    subparsers.add_parser("info")
    flash_test = subparsers.add_parser(
        "test", help="Destructively verify the dedicated scratch sector"
    )
    flash_test.add_argument("--confirm", action="store_true")
    subparsers.add_parser("list")
    subparsers.add_parser(
        "flights",
        help="List stored flights grouped by recorded MCU boot session",
    )

    read = subparsers.add_parser(
        "read",
        help="Download every stored page, or one flight selected by ID",
    )
    read.add_argument("--output", type=Path, required=True)
    read.add_argument(
        "--flight-id",
        help="Download only this numeric flight ID, or 'latest'",
    )
    read.add_argument(
        "--resume",
        action="store_true",
        help="Validate and continue an existing page-aligned partial download",
    )

    erase = subparsers.add_parser("erase")
    erase.add_argument("--confirm", action="store_true")

    config_get = subparsers.add_parser("config-get")
    config_get.add_argument("key")

    subparsers.add_parser("config-show", help="Read every supported configuration value")

    config_set = subparsers.add_parser("config-set")
    config_set.add_argument("key")
    config_set.add_argument("value", type=float)

    subparsers.add_parser("config-save")
    return parser.parse_args()


class Device:
    def __init__(self, port: str, baud: int, timeout: float) -> None:
        if serial is None:
            raise RuntimeError("pyserial is required: python -m pip install pyserial")
        self.timeout = timeout
        self.serial = serial.Serial(port, baudrate=baud, timeout=0.1, write_timeout=timeout)
        time.sleep(0.2)
        self.serial.reset_input_buffer()

    def close(self) -> None:
        self.serial.close()

    def command(self, command: str) -> None:
        self.serial.write(command.encode("ascii") + b"\r\n")
        self.serial.flush()

    def response_line(
        self,
        *,
        prefixes: tuple[bytes, ...] = (b"OK ", b"ERR "),
        timeout: float | None = None,
    ) -> bytes:
        deadline = time.monotonic() + (self.timeout if timeout is None else timeout)
        while time.monotonic() < deadline:
            line = self.serial.readline().strip()
            if b"FWDBG1 " in line and not line.startswith(b"FWDBG1 "):
                line = line.partition(b"FWDBG1 ")[0]
            if line.startswith(prefixes):
                return line
        raise TimeoutError("timed out waiting for FerroWasp storage response")

    def one(self, command: str) -> bytes:
        self.command(command)
        return self.response_line()


def list_logs(device: Device) -> tuple[int, int, int, bool]:
    line = device.one("logs list")
    return parse_log_info_line(line)


def parse_log_info_line(line: bytes) -> tuple[int, int, int, bool]:
    response = line.partition(b"FWDBG1 ")[0]
    match = LIST_RE_LONG.match(response) or LIST_RE_SHORT.match(response)
    if match is None:
        raise RuntimeError(line.decode("ascii", errors="replace"))
    used, next_flight, total, writable = (int(value) for value in match.groups())
    return used, next_flight, total, bool(writable)


def read_page(device: Device, page_index: int) -> bytes:
    device.command(f"logs read-page {page_index}")
    page = bytearray(PAGE_SIZE)
    seen: set[int] = set()
    deadline = time.monotonic() + device.timeout
    while len(seen) < PAGE_SIZE // 16 and time.monotonic() < deadline:
        line = device.response_line(prefixes=(b"PAGE ", b"ERR "))
        if line.startswith(b"ERR "):
            raise RuntimeError(line.decode("ascii", errors="replace"))
        match = PAGE_LINE_RE.match(line)
        if match is None or int(match.group(1)) != page_index:
            continue
        offset = int(match.group(2))
        if offset % 16 or offset >= PAGE_SIZE:
            raise RuntimeError(f"invalid page offset from device: {offset}")
        page[offset : offset + 16] = binascii.unhexlify(match.group(3))
        seen.add(offset)
    if len(seen) != PAGE_SIZE // 16:
        raise TimeoutError(f"incomplete page {page_index}: received {len(seen)}/16 chunks")
    return bytes(page)


@dataclass(frozen=True)
class PageMetadata:
    flight_id: int
    page_sequence: int
    record_count: int
    boot_session_start: bool


@dataclass(frozen=True)
class FlightSpan:
    flight_id: int
    start_page: int
    end_page: int
    boot_session_start: bool

    @property
    def page_count(self) -> int:
        return self.end_page - self.start_page


PageReader = Callable[[int], bytes]


def validated_page_metadata(page: bytes, page_index: int | None = None) -> PageMetadata:
    label = "" if page_index is None else f" at page {page_index}"
    if len(page) != PAGE_SIZE:
        raise RuntimeError(f"invalid flash-page length {len(page)}{label}")
    if page[PAGE_DATA_SIZE : PAGE_DATA_SIZE + 2] != b"FB":
        raise RuntimeError(f"invalid flash-page magic{label}")
    if page[PAGE_DATA_SIZE + 2] != 1:
        raise RuntimeError(f"unsupported flash-page version{label}")
    record_count = page[PAGE_DATA_SIZE + 3]
    if record_count > RECORDS_PER_PAGE:
        raise RuntimeError(f"invalid flash-page record count {record_count}{label}")
    expected_crc = int.from_bytes(page[-4:], "little")
    if binascii.crc32(page[:-4]) != expected_crc:
        raise RuntimeError(f"flash-page CRC mismatch{label}")
    flags = (
        int.from_bytes(
            page[RECORD_FLAGS_OFFSET : RECORD_FLAGS_OFFSET + 2],
            "little",
        )
        if record_count
        else 0
    )
    return PageMetadata(
        flight_id=int.from_bytes(
            page[PAGE_DATA_SIZE + 4 : PAGE_DATA_SIZE + 8],
            "little",
        ),
        page_sequence=int.from_bytes(
            page[PAGE_DATA_SIZE + 8 : PAGE_DATA_SIZE + 12],
            "little",
        ),
        record_count=record_count,
        boot_session_start=bool(flags & BOOT_SESSION_START_FLAG),
    )


def cached_device_page_reader(device: Device) -> PageReader:
    cache: dict[int, bytes] = {}

    def reader(page_index: int) -> bytes:
        if page_index not in cache:
            page = read_page(device, page_index)
            validated_page_metadata(page, page_index)
            cache[page_index] = page
        return cache[page_index]

    return reader


def lower_bound_flight_id(
    page_count: int,
    flight_id: int,
    read: PageReader,
) -> int:
    low = 0
    high = page_count
    while low < high:
        middle = low + (high - low) // 2
        metadata = validated_page_metadata(read(middle), middle)
        if metadata.flight_id < flight_id:
            low = middle + 1
        else:
            high = middle
    return low


def resolve_flight_span(
    used_pages: int,
    requested_flight_id: str,
    read: PageReader,
) -> FlightSpan:
    if used_pages == 0:
        raise RuntimeError("no stored flights")
    if requested_flight_id.lower() == "latest":
        selected_id = validated_page_metadata(
            read(used_pages - 1),
            used_pages - 1,
        ).flight_id
    else:
        try:
            selected_id = int(requested_flight_id, 10)
        except ValueError as error:
            raise RuntimeError("--flight-id must be an integer or 'latest'") from error
        if not 1 <= selected_id <= 0xFFFF_FFFF:
            raise RuntimeError("--flight-id must be in the range 1..4294967295")

    start = lower_bound_flight_id(used_pages, selected_id, read)
    if start == used_pages:
        raise RuntimeError(f"flight ID {selected_id} is not present")
    first = validated_page_metadata(read(start), start)
    if first.flight_id != selected_id:
        raise RuntimeError(f"flight ID {selected_id} is not present")
    end = (
        used_pages
        if selected_id == 0xFFFF_FFFF
        else lower_bound_flight_id(used_pages, selected_id + 1, read)
    )
    return FlightSpan(
        flight_id=selected_id,
        start_page=start,
        end_page=end,
        boot_session_start=first.boot_session_start,
    )


def catalog_flights(used_pages: int, read: PageReader) -> list[FlightSpan]:
    spans: list[FlightSpan] = []
    cursor = used_pages
    while cursor:
        last = validated_page_metadata(read(cursor - 1), cursor - 1)
        start = lower_bound_flight_id(cursor, last.flight_id, read)
        first = validated_page_metadata(read(start), start)
        spans.append(
            FlightSpan(
                flight_id=last.flight_id,
                start_page=start,
                end_page=cursor,
                boot_session_start=first.boot_session_start,
            )
        )
        cursor = start
    spans.reverse()
    return spans


def boot_session_numbers(spans: list[FlightSpan]) -> list[int | None]:
    sessions: list[int | None] = []
    current: int | None = None
    next_session = 1
    for span in spans:
        if span.boot_session_start:
            current = next_session
            next_session += 1
        sessions.append(current)
    return sessions


def validated_resume_page_count(path: Path, used_pages: int) -> int:
    if not path.exists():
        return 0
    return validated_resume_page_count_from_bytes(path.read_bytes(), used_pages)


def validated_resume_page_count_from_bytes(data: bytes, used_pages: int) -> int:
    size = len(data)
    if size % PAGE_SIZE:
        raise RuntimeError(
            f"cannot resume: existing file has {size % PAGE_SIZE} trailing bytes"
        )
    page_count = size // PAGE_SIZE
    if page_count > used_pages:
        raise RuntimeError(
            f"cannot resume: existing file has {page_count} pages, device has {used_pages}"
        )

    for page_index in range(page_count):
        start = page_index * PAGE_SIZE
        page = data[start : start + PAGE_SIZE]
        try:
            validated_page_metadata(page, page_index)
        except RuntimeError as error:
            raise RuntimeError(f"cannot resume: {error}") from error
    return page_count


def validated_flight_resume_page_count(path: Path, span: FlightSpan) -> int:
    if not path.exists():
        return 0
    data = path.read_bytes()
    if len(data) % PAGE_SIZE:
        raise RuntimeError(
            f"cannot resume: existing file has {len(data) % PAGE_SIZE} trailing bytes"
        )
    page_count = len(data) // PAGE_SIZE
    if page_count > span.page_count:
        raise RuntimeError(
            f"cannot resume: existing file has {page_count} pages, "
            f"flight {span.flight_id} has {span.page_count}"
        )
    for local_index in range(page_count):
        start = local_index * PAGE_SIZE
        try:
            metadata = validated_page_metadata(
                data[start : start + PAGE_SIZE],
                local_index,
            )
        except RuntimeError as error:
            raise RuntimeError(f"cannot resume: {error}") from error
        if metadata.flight_id != span.flight_id:
            raise RuntimeError(
                f"cannot resume: page {local_index} belongs to flight "
                f"{metadata.flight_id}, expected {span.flight_id}"
            )
        if metadata.page_sequence != local_index:
            raise RuntimeError(
                f"cannot resume: flight page sequence {metadata.page_sequence} "
                f"at local page {local_index}"
            )
    return page_count


def run(args: argparse.Namespace) -> int:
    device = Device(args.port, args.baud, args.timeout)
    try:
        if args.operation == "info":
            print(device.one("flash info").decode())
        elif args.operation == "test":
            if not args.confirm:
                raise RuntimeError("refusing to alter the scratch sector without --confirm")
            print(device.one("flash test CONFIRM").decode())
            print(device.response_line(timeout=max(args.timeout, 10.0)).decode())
        elif args.operation == "list":
            used, next_flight, total, writable = list_logs(device)
            print(
                f"used pages: {used}; next flight: {next_flight}; "
                f"capacity pages: {total}; writable: {writable}"
            )
        elif args.operation == "flights":
            used, _, _, _ = list_logs(device)
            if used == 0:
                print("no stored flights")
                return 0
            read = cached_device_page_reader(device)
            spans = catalog_flights(used, read)
            sessions = boot_session_numbers(spans)
            unseen_session = object()
            previous_session: int | None | object = unseen_session
            for span, session in zip(spans, sessions):
                if session != previous_session:
                    if session is None:
                        print("boot unknown (recorded before boot markers):")
                    else:
                        print(f"boot {session}:")
                    previous_session = session
                print(
                    f"  flight {span.flight_id}: pages "
                    f"{span.start_page}..{span.end_page - 1} "
                    f"({span.page_count} pages, {span.page_count * PAGE_SIZE} bytes)"
                )
        elif args.operation == "read":
            used, _, _, _ = list_logs(device)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            metadata_read = cached_device_page_reader(device)
            span = (
                resolve_flight_span(used, args.flight_id, metadata_read)
                if args.flight_id is not None
                else None
            )
            total_pages = span.page_count if span is not None else used
            local_start = (
                (
                    validated_flight_resume_page_count(args.output, span)
                    if span is not None
                    else validated_resume_page_count(args.output, used)
                )
                if args.resume
                else 0
            )
            if local_start:
                detail = (
                    f" for flight {span.flight_id}" if span is not None else ""
                )
                print(
                    f"resuming at page {local_start}/{total_pages}{detail}",
                    file=sys.stderr,
                )
            mode = "ab" if args.resume else "wb"
            with args.output.open(mode) as output:
                for local_index in range(local_start, total_pages):
                    device_index = (
                        span.start_page + local_index
                        if span is not None
                        else local_index
                    )
                    page = read_page(device, device_index)
                    metadata = validated_page_metadata(page, device_index)
                    if span is not None and (
                        metadata.flight_id != span.flight_id
                        or metadata.page_sequence != local_index
                    ):
                        raise RuntimeError(
                            "flight changed or page order is inconsistent during download"
                        )
                    output.write(page)
                    if local_index % 128 == 0 or local_index + 1 == total_pages:
                        detail = (
                            f" (flight {span.flight_id})" if span is not None else ""
                        )
                        print(
                            f"downloaded {local_index + 1}/{total_pages} pages{detail}",
                            file=sys.stderr,
                        )
            print(args.output)
        elif args.operation == "erase":
            if not args.confirm:
                raise RuntimeError("refusing to erase without --confirm")
            print(device.one("logs erase CONFIRM").decode())
            print(device.response_line(timeout=max(args.timeout, 600.0)).decode())
        elif args.operation == "config-get":
            print(device.one(f"config get {args.key}").decode())
        elif args.operation == "config-show":
            for key in CONFIG_KEYS:
                print(device.one(f"config get {key}").decode())
        elif args.operation == "config-set":
            print(device.one(f"config set {args.key} {args.value}").decode())
        elif args.operation == "config-save":
            print(device.one("config save").decode())
            print(device.response_line(timeout=max(args.timeout, 10.0)).decode())
        return 0
    finally:
        device.close()


def main() -> int:
    args = parse_args()
    try:
        return run(args)
    except (OSError, RuntimeError, TimeoutError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
