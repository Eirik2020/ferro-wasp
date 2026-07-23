#!/usr/bin/env python3
"""USB CDC CLI for FerroWasp onboard flash logs and configuration."""

from __future__ import annotations

import argparse
import binascii
import re
import sys
import time
from pathlib import Path

try:
    import serial
except ImportError:  # pragma: no cover - depends on the host environment
    serial = None


PAGE_LINE_RE = re.compile(rb"^PAGE (\d+) (\d{3}) ([0-9a-f]{32})$")
LIST_RE = re.compile(
    rb"^OK used_pages=(\d+) next_flight=(\d+) total_pages=(\d+) writable=([01])$"
)
PAGE_SIZE = 256


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

    read = subparsers.add_parser("read", help="Download every stored raw flash page")
    read.add_argument("--output", type=Path, required=True)
    read.add_argument(
        "--resume",
        action="store_true",
        help="Validate and continue an existing page-aligned partial download",
    )

    erase = subparsers.add_parser("erase")
    erase.add_argument("--confirm", action="store_true")

    config_get = subparsers.add_parser("config-get")
    config_get.add_argument("key")

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
            if line.startswith(prefixes):
                return line
        raise TimeoutError("timed out waiting for FerroWasp storage response")

    def one(self, command: str) -> bytes:
        self.command(command)
        return self.response_line()


def list_logs(device: Device) -> tuple[int, int, int, bool]:
    line = device.one("logs list")
    match = LIST_RE.match(line)
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
        if page[240:242] != b"FB":
            raise RuntimeError(
                f"cannot resume: invalid flash-page magic at page {page_index}"
            )
        expected_crc = int.from_bytes(page[-4:], "little")
        if binascii.crc32(page[:-4]) != expected_crc:
            raise RuntimeError(
                f"cannot resume: flash-page CRC mismatch at page {page_index}"
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
        elif args.operation == "read":
            used, _, _, _ = list_logs(device)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            start_page = (
                validated_resume_page_count(args.output, used) if args.resume else 0
            )
            if start_page:
                print(f"resuming at page {start_page}/{used}", file=sys.stderr)
            mode = "ab" if args.resume else "wb"
            with args.output.open(mode) as output:
                for page_index in range(start_page, used):
                    output.write(read_page(device, page_index))
                    if page_index % 128 == 0 or page_index + 1 == used:
                        print(f"downloaded {page_index + 1}/{used} pages", file=sys.stderr)
            print(args.output)
        elif args.operation == "erase":
            if not args.confirm:
                raise RuntimeError("refusing to erase without --confirm")
            print(device.one("logs erase CONFIRM").decode())
            print(device.response_line(timeout=max(args.timeout, 600.0)).decode())
        elif args.operation == "config-get":
            print(device.one(f"config get {args.key}").decode())
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
