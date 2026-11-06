#!/usr/bin/env python3
"""Talk to a flight controller over the SpeedyBee F405 V5 Bluetooth bridge.

The board's onboard Bluetooth module is a transparent BLE-to-UART bridge on
UART1; a host that can speak the flight controller's protocol can therefore
use it wirelessly. This tool is that host for bench work: it scans for
advertising bridges, frames MSPv2 queries, decodes the common answers, and
can send or watch raw bytes when something new needs watching. The link, its
GATT interface and its security posture are documented in
`project_meta/speedybee_f405v5_bluetooth.md`.

Read-only by default: `info` and `msp` ask the controller questions, `scan`
and `monitor` only observe, and `raw` sends exactly the bytes given, visibly.
Nothing here can arm, actuate or configure. The bridge itself authenticates
nobody, so only talk to hardware you own.

Requires `dbus` (python3-dbus) and BlueZ; no pairing is needed.
"""

from __future__ import annotations

import argparse
import errno
import os
import select
import struct
import sys
import time

# The bridge's GATT service and characteristics, from the reference chapter.
BRIDGE_SERVICE_UUID = "0000abf0-0000-1000-8000-00805f9b34fb"
BRIDGE_WRITE_UUID = "0000abf1-0000-1000-8000-00805f9b34fb"  # host -> UART
BRIDGE_NOTIFY_UUID = "0000abf2-0000-1000-8000-00805f9b34fb"  # UART -> host
# The name the module advertises; used to find it when no address is given.
BRIDGE_ADVERTISED_NAME = "F405 V5"

# MSPv2 framing: $X< flags:u8 function:u16le length:u16le payload crc8.
MSP_RESPONSE_PREAMBLE = b"$X>"
REQUEST_PREAMBLE = b"$X<"
MAX_PAYLOAD = 1024
MAX_STREAM_BUFFER = 4096

# `MspStream` uses this internally to say a frame is still incomplete.
_INCOMPLETE = object()


def crc8_dvb_s2(data: bytes) -> int:
    """CRC-8/DVB-S2, the MSPv2 checksum (not MSPv1's XOR)."""
    crc = 0
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = ((crc << 1) ^ 0xD5) & 0xFF if crc & 0x80 else (crc << 1) & 0xFF
    return crc


def encode_request(function: int, payload: bytes = b"") -> bytes:
    """One MSPv2 request frame for `function` with `payload`."""
    if not 0 <= function <= 0xFFFF:
        raise ValueError(f"function {function} does not fit MSPv2's 16 bits")
    if len(payload) > MAX_PAYLOAD:
        raise ValueError(f"payload of {len(payload)} exceeds {MAX_PAYLOAD} bytes")
    body = struct.pack("<BHH", 0, function, len(payload)) + payload
    return REQUEST_PREAMBLE + body + bytes([crc8_dvb_s2(body)])


class MspStream:
    """Bounded MSPv2 response parser: resynchronises, verifies, never hangs.

    Bytes may arrive split across many notifications, so `feed` accumulates
    a buffer, returns every complete frame it finds, and resynchronises on
    the preamble after garbage. A frame with a bad checksum is dropped whole;
    the stream keeps parsing after it.
    """

    def __init__(self) -> None:
        self._buffer = bytearray()

    def feed(self, chunk: bytes) -> list[tuple[int, bytes]]:
        """Add `chunk`, return (function, payload) for each complete frame."""
        self._buffer.extend(chunk)
        frames = []
        while True:
            start = self._buffer.find(MSP_RESPONSE_PREAMBLE)
            if start < 0:
                self._discard_garbage()
                break
            if start > 0:
                del self._buffer[:start]
            outcome = self._take_frame()
            if outcome is _INCOMPLETE:
                break
            if outcome is not None:
                frames.append(outcome)
        return frames

    def _take_frame(self):
        """One frame from the buffer: its tuple, `None` if a corrupt frame
        was dropped whole, or `_INCOMPLETE` when more bytes are needed."""
        if len(self._buffer) < 9:
            return _INCOMPLETE
        _, function, length = struct.unpack_from("<BHH", self._buffer, 3)
        if length > MAX_PAYLOAD:
            # Not a frame start after all; skip this preamble.
            del self._buffer[:1]
            return None
        end = 9 + length
        if len(self._buffer) < end:
            return _INCOMPLETE
        body = bytes(self._buffer[3 : 8 + length])
        crc = self._buffer[8 + length]
        del self._buffer[:end]
        if crc8_dvb_s2(body) != crc:
            return None  # Dropped whole; the next bytes may hold a good one.
        return function, body[5:]

    def _discard_garbage(self) -> None:
        if len(self._buffer) > MAX_STREAM_BUFFER:
            del self._buffer[: len(self._buffer) - MAX_PAYLOAD]


def decode_api_version(payload: bytes) -> str:
    """`API_VERSION` (1): protocol version, then MSP API major.minor."""
    if len(payload) < 3:
        raise ValueError(f"API_VERSION answer of {len(payload)} bytes is short")
    return f"MSP protocol {payload[0]}, API {payload[1]}.{payload[2]}"


def decode_fc_version(payload: bytes) -> str:
    """`FC_VERSION` (3): semver, optionally after a 4-byte identifier.

    Betaflight answers the MSPv2 form with the semver triplet only; the
    older MSPv1 form prefixes a 4-byte identifier and appends a git hash.
    """
    if len(payload) == 3:
        return ".".join(str(part) for part in payload)
    if len(payload) < 7:
        raise ValueError(f"FC_VERSION answer of {len(payload)} bytes is short")
    ident = payload[:4].decode("ascii", "replace")
    version = ".".join(str(part) for part in payload[4:7])
    git = payload[7:].split(b"\x00", 1)[0].decode("ascii", "replace")
    return f"{ident} {version} ({git})" if git else f"{ident} {version}"


def decode_uid(payload: bytes) -> str:
    """`UID` (160): three little-endian 32-bit words."""
    if len(payload) < 12:
        raise ValueError(f"UID answer of {len(payload)} bytes is short")
    words = struct.unpack_from("<III", payload, 0)
    return " ".join(f"0x{word:08x}" for word in words)


def decode_boxids(payload: bytes) -> str:
    """`BOXIDS` (119): the build's permanent box IDs, one byte each.

    The bridge module itself polls this command as its flight-controller
    heartbeat, which is why it is worth a decoder here.
    """
    return "box ids " + " ".join(str(byte) for byte in payload)


def decode_status(payload: bytes) -> str:
    """`STATUS` (101), Betaflight 4.5.x layout: cycle time, I2C error count,
    present-sensor mask, flight-mode flags, PID profile, CPU load; the
    build-specific remainder is shown as hex rather than guessed."""
    if len(payload) < 13:
        raise ValueError(f"STATUS answer of {len(payload)} bytes is short")
    cycle_time, i2c_errors, sensors = struct.unpack_from("<HHH", payload, 0)
    flags, pid_profile, cpu_load = struct.unpack_from("<IBH", payload, 6)
    parts = [
        f"cycle time {cycle_time} us",
        f"i2c errors {i2c_errors}",
        f"sensors 0x{sensors:02x}",
        f"flags 0x{flags:08x}",
        f"pid profile {pid_profile}",
        f"cpu load {cpu_load}%",
    ]
    tail = payload[13:]
    if tail:
        parts.append(f"tail {tail.hex()}")
    return ", ".join(parts)


KNOWN_FUNCTIONS = {
    1: ("API_VERSION", decode_api_version),
    3: ("FC_VERSION", decode_fc_version),
    101: ("STATUS", decode_status),
    119: ("BOXIDS", decode_boxids),
    160: ("UID", decode_uid),
}


def describe_response(function: int, payload: bytes) -> str:
    """A one-line human rendering of one response frame."""
    if function in KNOWN_FUNCTIONS:
        name, decoder = KNOWN_FUNCTIONS[function]
        try:
            return f"{name} ({function}): {decoder(payload)}"
        except ValueError as error:
            return f"{name} ({function}): {payload.hex()} ({error})"
    return f"function {function}: {payload.hex()}"


class BlueZError(RuntimeError):
    """The Bluetooth stack refused something; the message says what."""


class BlueZBridge:
    """A connected SpeedyBee Bluetooth bridge, over BlueZ's D-Bus API.

    Reading uses the notify socket `AcquireNotify` returns, so every
    notification is received in order and none are missed - a long MSPv2
    answer arrives split across several of them. Nothing needs a main loop.
    """

    def __init__(self, address: str) -> None:
        try:
            import dbus  # noqa: F401 - present only where BlueZ is
        except ImportError as error:
            raise BlueZError(
                "python3-dbus is required (the tool talks to BlueZ through D-Bus)"
            ) from error
        self.address = address.upper()
        self._bus = dbus.SystemBus()
        self._device = None
        self._notify_fd: int | None = None

    @staticmethod
    def _objects(bus) -> dict:
        import dbus

        manager = dbus.Interface(
            bus.get_object("org.bluez", "/"), "org.freedesktop.DBus.ObjectManager"
        )
        return manager.GetManagedObjects()

    @staticmethod
    def _device_path(address: str) -> str:
        return "/org/bluez/hci0/dev_" + address.replace(":", "_")

    def _resolve_device(self) -> object:
        import dbus

        objects = self._objects(self._bus)
        path = self._device_path(self.address)
        if path not in objects:
            raise BlueZError(f"no BlueZ device object for {self.address}; scan first")
        return dbus.Interface(
            self._bus.get_object("org.bluez", path), "org.bluez.Device1"
        )

    def _wait_for_characteristic(self, uuid: str, timeout: float) -> object:
        """The GATT characteristic object once services are resolved."""
        import dbus

        want = uuid.lower()
        deadline = time.monotonic() + timeout
        while True:
            for path, interfaces in self._objects(self._bus).items():
                info = interfaces.get("org.bluez.GattCharacteristic1", {})
                if str(info.get("UUID", "")).lower() == want:
                    return dbus.Interface(
                        self._bus.get_object("org.bluez", path),
                        "org.bluez.GattCharacteristic1",
                    )
            if time.monotonic() > deadline:
                raise BlueZError(f"characteristic {uuid} did not appear")
            time.sleep(0.1)

    def connect(self, timeout: float = 8.0) -> None:
        """Connect the device and open the bridge's two characteristics."""
        import dbus

        device = self._resolve_device()
        try:
            device.Connect(timeout=int(timeout * 1000))
        except Exception as error:  # dbus raises dbus.DBusException
            raise BlueZError(f"could not connect {self.address}: {error}") from error
        self._device = device
        write = self._wait_for_characteristic(BRIDGE_WRITE_UUID, timeout)
        notify = self._wait_for_characteristic(BRIDGE_NOTIFY_UUID, timeout)
        self._write_char = write
        try:
            fd_handle, _mtu = notify.AcquireNotify(
                dbus.Dictionary({}, signature="sv"), byte_arrays=True
            )
        except Exception as error:
            self.disconnect()
            raise BlueZError(
                f"could not subscribe to UART notifications: {error}"
            ) from error
        self._notify_fd = fd_handle.take()
        os.set_blocking(self._notify_fd, False)

    def write(self, data: bytes) -> None:
        """Send `data` to UART through the bridge, unchanged."""
        import dbus

        if self._device is None:
            raise BlueZError("not connected")
        try:
            self._write_char.WriteValue(
                dbus.Array(bytearray(data), signature="y"),
                dbus.Dictionary({}, signature="sv"),
            )
        except Exception as error:
            raise BlueZError(f"write failed: {error}") from error

    def read(self, timeout: float) -> bytes:
        """Whatever the UART sent through the bridge, within `timeout`."""
        if self._notify_fd is None:
            raise BlueZError("not connected")
        collected = bytearray()
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            wait = max(0.0, min(0.05, deadline - time.monotonic()))
            readable, _, _ = select.select([self._notify_fd], [], [], wait)
            if readable:
                try:
                    chunk = os.read(self._notify_fd, 1024)
                except OSError as error:
                    if error.errno == errno.EAGAIN:
                        continue
                    raise BlueZError(f"notify socket failed: {error}") from error
                collected.extend(chunk)
                # Drain what is already readable before waiting again.
                continue
        return bytes(collected)

    def disconnect(self) -> None:
        """Release the subscription and the link; safe to call twice."""
        if self._notify_fd is not None:
            os.close(self._notify_fd)
            self._notify_fd = None
        if self._device is not None:
            try:
                self._device.Disconnect()
            except Exception:
                pass  # Already gone; the link is down either way.
            self._device = None


def scan_bus(duration: float = 3.0) -> list[tuple[str, str, int]]:
    """Advertised LE devices as (address, name, RSSI)."""
    import dbus

    bus = dbus.SystemBus()
    adapter = dbus.Interface(
        bus.get_object("org.bluez", "/org/bluez/hci0"), "org.bluez.Adapter1"
    )
    try:
        adapter.SetDiscoveryFilter(
            dbus.Dictionary({"Transport": dbus.String("le")}, signature="sv")
        )
        adapter.StartDiscovery()
    except Exception as error:
        raise BlueZError(f"could not scan: {error}") from error
    try:
        time.sleep(duration)
        found = []
        for path, interfaces in BlueZBridge._objects(bus).items():
            info = interfaces.get("org.bluez.Device1", {})
            address = str(info.get("Address", ""))
            name = str(info.get("Name", "")) or str(info.get("Alias", ""))
            if not address:
                continue
            rssi = int(info.get("RSSI", 0) or 0)
            found.append((address, name, rssi))
        return sorted(found)
    finally:
        try:
            adapter.StopDiscovery()
        except Exception:
            pass


def find_bridge_address(name: str, duration: float = 5.0) -> str:
    """Scan for an advertising bridge whose name contains `name`."""
    for address, advertised, _ in scan_bus(duration):
        if name.lower() in advertised.lower():
            return address
    raise BlueZError(f'no device advertising "{name}" was found')


def query(bridge: BlueZBridge, function: int, payload: bytes, timeout: float):
    """Send one MSPv2 request; return (function, payload) of the answer."""
    bridge.write(encode_request(function, payload))
    stream = MspStream()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frames = stream.feed(bridge.read(0.5))
        for got_function, got_payload in frames:
            if got_function == function:
                return got_function, got_payload
    return None


def parse_hex(text: str, what: str) -> bytes:
    """Bytes from a hex string, rejecting partial bytes visibly."""
    cleaned = text.replace(":", "").replace(" ", "").replace("-", "")
    try:
        return bytes.fromhex(cleaned)
    except ValueError:
        raise SystemExit(f"{what}: {text!r} is not a hex string")


def cmd_scan(args: argparse.Namespace) -> int:
    devices = scan_bus(args.duration)
    if not devices:
        print("no advertising devices found")
        return 1
    for address, name, rssi in devices:
        marker = "  <- bridge" if BRIDGE_ADVERTISED_NAME.lower() in name.lower() else ""
        print(f"{address}  {rssi:4d} dBm  {name or '(no name)'}{marker}")
    return 0


def open_bridge(args: argparse.Namespace) -> BlueZBridge:
    address = args.address or find_bridge_address(args.name)
    bridge = BlueZBridge(address)
    bridge.connect()
    print(f"connected {address}", file=sys.stderr)
    return bridge


def cmd_info(args: argparse.Namespace) -> int:
    bridge = open_bridge(args)
    # API_VERSION, FC_VERSION, UID, then STATUS, the liveliest of the set.
    try:
        for function in (1, 3, 160, 101):
            answer = query(bridge, function, b"", args.timeout)
            if answer is None:
                print(f"function {function}: no answer within {args.timeout:g} s")
                continue
            print(describe_response(answer[0], answer[1]))
    finally:
        bridge.disconnect()
    return 0


def cmd_msp(args: argparse.Namespace) -> int:
    payload = parse_hex(args.payload, "payload") if args.payload else b""
    bridge = open_bridge(args)
    try:
        answer = query(bridge, args.function, payload, args.timeout)
        if answer is None:
            print(f"function {args.function}: no answer within {args.timeout:g} s")
            return 1
        print(describe_response(answer[0], answer[1]))
        return 0
    finally:
        bridge.disconnect()


def cmd_raw(args: argparse.Namespace) -> int:
    data = parse_hex(args.data, "data")
    bridge = open_bridge(args)
    try:
        bridge.write(data)
        print(f"sent {len(data)} bytes: {data.hex()}")
        deadline = time.monotonic() + args.duration
        while time.monotonic() < deadline:
            chunk = bridge.read(0.5)
            if chunk:
                print(f"got {len(chunk)} bytes: {chunk.hex()}")
    finally:
        bridge.disconnect()
    return 0


def cmd_monitor(args: argparse.Namespace) -> int:
    bridge = open_bridge(args)
    total = 0
    try:
        print(f"listening for {args.duration:g} s, Ctrl+C stops")
        deadline = time.monotonic() + args.duration
        while time.monotonic() < deadline:
            chunk = bridge.read(0.5)
            if chunk:
                total += len(chunk)
                print(chunk.hex())
    except KeyboardInterrupt:
        pass
    finally:
        bridge.disconnect()
    print(f"{total} bytes total")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--address",
        help="bridge MAC address; without it, the first device advertising"
        f' "{BRIDGE_ADVERTISED_NAME}" is used',
    )
    parser.add_argument(
        "--name",
        default=BRIDGE_ADVERTISED_NAME,
        help=f'advertisement name to look for (default "{BRIDGE_ADVERTISED_NAME}")',
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=3.0,
        help="seconds to wait for one answer (default 3)",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    scan = subparsers.add_parser("scan", help="list advertising devices")
    scan.add_argument("--duration", type=float, default=3.0, help="scan seconds")
    scan.set_defaults(handler=cmd_scan)

    info = subparsers.add_parser("info", help="identify the controller over MSPv2")
    info.set_defaults(handler=cmd_info)

    msp = subparsers.add_parser(
        "msp", help="send one MSPv2 query and decode the answer"
    )
    msp.add_argument("function", type=lambda text: int(text, 0), help="function number")
    msp.add_argument("payload", nargs="?", default="", help="hex payload, optional")
    msp.set_defaults(handler=cmd_msp)

    raw = subparsers.add_parser("raw", help="send raw bytes, print what returns")
    raw.add_argument("data", help="hex bytes to send")
    raw.add_argument("--duration", type=float, default=3.0, help="listen seconds")
    raw.set_defaults(handler=cmd_raw)

    monitor = subparsers.add_parser("monitor", help="hexdump UART traffic until stopped")
    monitor.add_argument("--duration", type=float, default=30.0, help="listen seconds")
    monitor.set_defaults(handler=cmd_monitor)

    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        return args.handler(args)
    except BlueZError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
