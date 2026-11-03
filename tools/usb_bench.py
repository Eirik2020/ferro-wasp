#!/usr/bin/env python3
"""Run a flight board's bench session over USB alone, with ESC power off.

`session --board <target>` builds the board's image from the checkout, flashes
it over the ROM's DFU, and checks what USB can show: that it comes back, sits
idle and disarmed with its loop at rate, keeps a live change only until a
reboot, refuses bindings the board cannot carry, restarts on command, and, for
the Foxeer, calibrates and falls back from a broken save. It needs no probe.
The operator confirms the bench state, and handles the board where a step says
so.

A failed check is reported and the session goes on; an armed board or an unknown
device ends it. Captures land in a new directory under `logs/bench/`. A Foxeer
session that passes is recorded with reqtool against its procedure, for review;
nothing is committed. A Lucid session is captures only: the board cannot arm,
and its card is out.

What USB cannot show stays open: the first second of a boot, the motor pins,
and why arming would be refused, since USB itself blocks arming and the reasons
go to RTT. `boot-idle` in `tools/bench_rig.py` needs a probe and is not replaced.
"""

from __future__ import annotations

import argparse
from collections import deque
import json
import re
import shutil
import subprocess
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

if __package__:
    from . import foxeer_bench_gates as gates
    from . import reqtool_bridge
else:
    import foxeer_bench_gates as gates
    import reqtool_bridge

REPO_ROOT = Path(__file__).resolve().parents[1]
BENCH_LOGS = REPO_ROOT / "logs/bench"
TARGET_TRIPLE = "thumbv7em-none-eabihf"
USB_VID = 0x16C0
USB_PID = 0x27DD
DFU_DEVICE = "0483:df11"
SETTLE_QUIET_S = 0.3
HANDS_OFF_S = 3.0
PROCEDURE = "project_meta/testing/targets/foxeer-usb-session.md#Foxeer USB Session"
BENCH_FEATURE = "bench_config_fault"
# How long a board takes to enumerate after leaving the ROM or restarting.
PORT_RETURN_S = 15.0
# How long a board may take to reach the ROM, as the reboot gate allows.
DFU_ENTRY_S = 12.0
IDLE_SECONDS = 30.0
# How far the measured control rate may sit from the rate the image reports.
CONTROL_RATE_TOLERANCE = 0.05


@dataclass(frozen=True)
class Board:
    target: str
    elf: str
    usb_serial: str
    # The serial number a FerroHAL bench image on this board enumerates with.
    hal_bench_serial: str
    # The flash layout its ROM reports, so one board's image can't reach another.
    dfu_layout: str
    flash_bytes: int
    has_store: bool

    @property
    def app_dir(self) -> Path:
        return REPO_ROOT / "firmware" / self.target


BOARDS = {
    "foxeer-f405-v2": Board(
        target="foxeer-f405-v2",
        elf="FerroWaspFoxeerF405V2",
        usb_serial="FW-FOX-F405V2",
        hal_bench_serial="foxeer-f405-v2",
        dfu_layout="04*016Kg",
        flash_bytes=1024 * 1024,
        has_store=True,
    ),
    "tbs-lucid-h7": Board(
        target="tbs-lucid-h7",
        elf="FerroWaspTbsLucidH7",
        usb_serial="FW-TBS-LUCID-H7",
        hal_bench_serial="lucid-h7",
        dfu_layout="16*128Kg",
        flash_bytes=2 * 1024 * 1024,
        has_store=False,
    ),
}


class StepFailed(Exception):
    """A check that failed: reported, and the session goes on."""


class SessionStop(Exception):
    """A stop condition: the session ends here."""


# --- Reading what the board says ------------------------------------------------


def split_lines(raw: str) -> list[str]:
    """A status line can land inside a command's answer, so each starts its own
    line here."""
    return [line.strip() for line in raw.replace("FWDBG1 ", "\nFWDBG1 ").splitlines() if line.strip()]


def parse_status(line: str) -> dict[str, str] | None:
    if not line.startswith("FWDBG1 "):
        return None
    return dict(word.split("=", 1) for word in line.split()[1:] if "=" in word)


def idle_failures(statuses: list[dict[str, str]]) -> list[str]:
    """Why these status lines do not show a healthy idle board; empty when they do."""
    if len(statuses) < 2:
        return [f"{len(statuses)} status lines, too few to judge"]
    problems = []
    if any(status.get("armed") != "0" for status in statuses):
        problems.append("the board reported armed")
    if any(status.get("ready") != "1" for status in statuses):
        problems.append("the IMU was not ready")
    if any(status.get("stale") != "0" for status in statuses):
        problems.append("the IMU reported stale")
    if any(status.get("vbat_stale") != "0" for status in statuses):
        problems.append("the battery reading was stale or not reported")
    uptimes = [int(status["ms"]) for status in statuses]
    if uptimes != sorted(uptimes):
        problems.append("uptime went backwards: the board restarted")
    first, last = statuses[0], statuses[-1]
    if int(last["seq"]) <= int(first["seq"]):
        problems.append("the IMU sequence did not advance")
    rate = control_rate(statuses)
    target = int(first["ctl_hz"])
    if rate is None:
        problems.append("no time passed between status lines")
    elif abs(rate - target) > target * CONTROL_RATE_TOLERANCE:
        problems.append(f"control loop at {rate:.0f} Hz against {target} Hz")
    return problems


def control_rate(statuses: list[dict[str, str]]) -> float | None:
    first, last = statuses[0], statuses[-1]
    elapsed_ms = int(last["ms"]) - int(first["ms"])
    if elapsed_ms <= 0:
        return None
    return (int(last["ctl"]) - int(first["ctl"])) * 1000.0 / elapsed_ms


def fallback_state(answer: str) -> str | None:
    match = re.match(r"OK config fallback=([a-z]+)", answer)
    return match.group(1) if match else None


def config_value(answer: str, key: str) -> float | None:
    match = re.fullmatch(rf"OK {re.escape(key)}=(-?[0-9.]+)", answer)
    return float(match.group(1)) if match else None


def dfu_serial(listing: str, layout: str) -> str | None:
    """The serial number of the ROM showing this board's flash layout."""
    for line in listing.splitlines():
        if layout in line:
            match = re.search(r'serial="([^"]*)"', line)
            if match:
                return match.group(1)
    return None


def port_table(document: dict[str, Any]) -> dict[str, str]:
    return {entry["port"]: entry["function"] for entry in document["result"]["ports"]}


# --- Talking to the board ---------------------------------------------------------


class Transcript:
    """Everything said to and by the board, in order, as evidence."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def write(self, text: str) -> None:
        stamp = datetime.now(UTC).strftime("%H:%M:%S.%f")[:-3]
        with self.path.open("a") as log:
            log.write(f"{stamp} {text}\n")


def runs_revision(answer: str, head: str) -> bool:
    """Whether a `build` answer names this commit. An image carries a short hash."""
    reported = re.search(r"rev=([0-9a-f]{7,40})\b", answer)
    return reported is not None and head.startswith(reported.group(1))


def refused_by_board(output: str) -> bool:
    """Whether a failed configurator command failed by the board's own refusal,
    rather than for not reaching the board at all."""
    return "device rejected" in output


def compare_baselines(before: Path, after: Path) -> None:
    if not before.is_file():
        raise StepFailed("no configuration was exported at the start, so there is nothing to compare")
    if after.read_text() != before.read_text():
        raise StepFailed("the configuration at the end differs from the one at the start")


def find_port(usb_serial: str) -> str | None:
    for port in gates.stable_comports():
        if port.vid == USB_VID and port.pid == USB_PID and port.serial_number == usb_serial:
            return port.device
    return None


def wait_for_port(usb_serial: str, present: bool, timeout_s: float) -> str | None:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        port = find_port(usb_serial)
        if (port is not None) == present:
            return port
        time.sleep(0.1)
    raise TimeoutError


class Link:
    """The board's command line, for what FerroConfigurator has no command for."""

    def __init__(self, port: str, transcript: Transcript) -> None:
        import serial

        self.port = serial.Serial(port, 115200, timeout=0.05)
        self.transcript = transcript
        self.buffer = ""
        self.statuses: list[dict[str, str]] = []
        self.pending: deque[str] = deque()
        self.settle()

    def settle(self) -> None:
        """Ends whatever the board has half-read, and drops what that says. A port
        fresh from enumeration echoes what the board sends until it is raw, and the
        board reads the echo as the start of the next command, which then fails."""
        self.port.write(b"\r\n")
        quiet_from = time.monotonic()
        while time.monotonic() - quiet_from < SETTLE_QUIET_S:
            if self.port.read(512):
                quiet_from = time.monotonic()

    def __enter__(self) -> Link:
        return self

    def __exit__(self, *_: object) -> None:
        self.port.close()

    def _lines(self, seconds: float) -> list[str]:
        deadline = time.monotonic() + seconds
        lines: list[str] = []
        while time.monotonic() < deadline and not lines:
            self.buffer += self.port.read(512).decode("ascii", errors="replace")
            *whole, self.buffer = self.buffer.split("\n")
            lines = [line for raw in whole for line in split_lines(raw)]
        for line in lines:
            self.transcript.write(f"< {line}")
            if (status := parse_status(line)) is not None:
                self.statuses.append(status)
        return lines

    def send(self, command: str) -> None:
        self.transcript.write(f"> {command}")
        self.pending.clear()
        self.port.write(f"{command}\r\n".encode("ascii"))

    def wait_for(self, done: Callable[[str], bool], seconds: float, what: str) -> str:
        # A read can hold several lines, and the rest of them are the next answer.
        deadline = time.monotonic() + seconds
        while True:
            while self.pending:
                line = self.pending.popleft()
                if done(line):
                    return line
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise StepFailed(f"no answer to {what} within {seconds:.0f} s")
            self.pending.extend(self._lines(remaining))

    def ask(self, command: str, seconds: float = 3.0) -> str:
        """Sends one command and gives its first answer line."""
        self.send(command)
        return self.wait_for(lambda line: line.startswith(("OK ", "ERR ")), seconds, f"`{command}`")

    def expect(self, command: str, prefix: str, seconds: float = 3.0) -> str:
        answer = self.ask(command, seconds)
        if not answer.startswith(prefix):
            raise StepFailed(f"`{command}` answered `{answer}`, expected `{prefix}`")
        return answer

    def wait_prefix(self, prefix: str, seconds: float, what: str) -> str:
        answer = self.wait_for(lambda line: line.startswith((prefix, "ERR ")), seconds, what)
        if not answer.startswith(prefix):
            raise StepFailed(f"{what} answered `{answer}`, expected `{prefix}`")
        return answer

    def collect_statuses(self, seconds: float) -> list[dict[str, str]]:
        self.statuses = []
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self._lines(deadline - time.monotonic())
        return self.statuses


# --- A session -----------------------------------------------------------------


@dataclass
class Step:
    name: str
    passed: bool
    detail: str = ""


@dataclass
class Session:
    board: Board
    capture: Path
    transcript: Transcript
    commands: list[dict[str, Any]] = field(default_factory=list)
    steps: list[Step] = field(default_factory=list)
    measurements: dict[str, Any] = field(default_factory=dict)

    def run(self, argv: list[str], shown: str) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(argv, capture_output=True, text=True, cwd=REPO_ROOT)
        self.commands.append({"command": shown, "exit_code": result.returncode})
        self.transcript.write(f"$ {shown}\n{result.stdout}{result.stderr}")
        return result

    def step(self, name: str, action: Callable[[], None]) -> None:
        print(f"\n--- {name}")
        try:
            action()
        except StepFailed as failure:
            self.steps.append(Step(name, False, str(failure)))
            print(f"FAIL: {failure}")
        else:
            self.steps.append(Step(name, True))
            print("pass")

    # --- The board's connection ---

    def port(self) -> str:
        port = find_port(self.board.usb_serial)
        if port is None:
            raise StepFailed(f"no {self.board.target} running FerroWasp on USB")
        return port

    def link(self) -> Link:
        return Link(self.port(), self.transcript)

    def configurator(self, *args: str) -> subprocess.CompletedProcess[str]:
        argv = [str(gates.CONFIGURATOR), "--port", self.port(), *args]
        return self.run(argv, " ".join(["ferro-configurator", *args]))

    def configurator_ok(self, *args: str) -> str:
        result = self.configurator(*args)
        if result.returncode != 0:
            raise StepFailed(f"`{' '.join(args)}` failed: {result.stderr.strip()}")
        return result.stdout

    def wait_returned(self, what: str, then_settle_s: float = 4.0) -> None:
        """Waits out a restart: the port goes, then the same board comes back,
        and its gyro calibration finishes."""
        try:
            wait_for_port(self.board.usb_serial, False, 5.0)
            wait_for_port(self.board.usb_serial, True, PORT_RETURN_S)
        except TimeoutError:
            raise StepFailed(f"the board did not come back after {what}") from None
        time.sleep(then_settle_s)

    def reboot(self) -> None:
        with self.link() as link:
            link.expect("reboot", "OK rebooting")
        self.wait_returned("`reboot`")

    # --- Flashing ---

    def dfu_serial(self) -> str | None:
        listing = subprocess.run(
            ["dfu-util", "-d", DFU_DEVICE, "-l"], capture_output=True, text=True
        ).stdout
        return dfu_serial(listing, self.board.dfu_layout)

    def wait_dfu(self, timeout_s: float) -> str | None:
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            if (serial := self.dfu_serial()) is not None:
                return serial
            time.sleep(0.3)
        return None

    def enter_rom(self) -> tuple[str, bool]:
        """Brings the board to its ROM from whatever it runs; the serial number
        there, and whether that took the BOOT button."""
        if (serial := self.dfu_serial()) is not None:
            return serial, False
        hal_port = find_port(self.board.hal_bench_serial)
        if hal_port is not None:
            import serial as pyserial

            with pyserial.Serial(hal_port, 115200, timeout=0.5) as port:
                port.write(b"BENCHDFU")
            self.transcript.write("> BENCHDFU, to the FerroHAL bench image")
        elif find_port(self.board.usb_serial) is not None:
            with self.link() as link:
                link.expect("reboot dfu", "OK rebooting to DFU")
        if (serial := self.wait_dfu(DFU_ENTRY_S)) is not None:
            return serial, False
        print(f"\nThe {self.board.target} did not reach its ROM.")
        gates.ask("Hold BOOT while plugging its USB in, then press Enter:")
        if (serial := self.wait_dfu(30.0)) is not None:
            return serial, True
        raise SessionStop(f"no {self.board.target} in DFU")

    def flash(self, elf: Path) -> None:
        """Flashes an image and requires the board back on USB without a power
        cycle. That return is the VTOR question: an F405 image that leaves its
        ROM by a jump, with no reset, has no USB until one."""
        serial, took_boot_button = self.enter_rom()
        binary = elf.with_suffix(".bin")
        objcopy = self.run(
            ["arm-none-eabi-objcopy", "-O", "binary", str(elf), str(binary)],
            f"arm-none-eabi-objcopy {elf.name}",
        )
        if objcopy.returncode != 0:
            raise SessionStop(f"objcopy failed: {objcopy.stderr.strip()}")
        if binary.stat().st_size > self.board.flash_bytes:
            raise SessionStop(f"{binary.name} is larger than the board's flash")
        # dfu-util exits nonzero when the board leaves before its last status,
        # so the board coming back is what counts.
        self.run(
            ["dfu-util", "-d", DFU_DEVICE, "-S", serial, "-a", "0", "-s", "0x08000000:leave", "-D", str(binary)],
            f"dfu-util -S <{self.board.target}> -s 0x08000000:leave -D {binary.name}",
        )
        try:
            wait_for_port(self.board.usb_serial, True, PORT_RETURN_S)
        except TimeoutError:
            print("\nThe board did not return on USB after DFU.")
            gates.ask("Unplug its USB, plug it back in, then press Enter:")
            if find_port(self.board.usb_serial) is None:
                time.sleep(5)
            if find_port(self.board.usb_serial) is None:
                raise SessionStop("the board does not come back on USB, even after a power cycle") from None
            raise StepFailed(
                "USB came back only after a power cycle: the image needs set-vtor"
            ) from None
        time.sleep(4.0)
        if took_boot_button:
            raise StepFailed("the image is flashed, but the board reached its ROM only through the BOOT button")

    # --- Shared steps ---

    def require_identity(self, features: tuple[str, ...]) -> None:
        """The board runs this commit, built with exactly these features. An
        image lists its features other than its board and the default."""
        head = reqtool_bridge.git("rev-parse", "HEAD")
        with self.link() as link:
            answer = link.expect("build", "OK build", seconds=3.0)
            listed = re.search(r"features=(\d+)", answer)
            reported = [
                link.wait_prefix("OK feature ", 2.0, "`build`'s features").removeprefix("OK feature ")
                for _ in range(int(listed.group(1)) if listed else 0)
            ]
        if not runs_revision(answer, head):
            raise StepFailed(f"the board reports `{answer}`, not revision {head[:12]}")
        if sorted(reported) != sorted(features):
            raise StepFailed(f"the board reports features {reported}, not {list(features)}")

    def idle(self) -> None:
        with self.link() as link:
            statuses = link.collect_statuses(IDLE_SECONDS)
            live = link.expect("live", "OK live")
        if any(status.get("armed") != "0" for status in statuses):
            raise SessionStop("the controller reports armed")
        (self.capture / "idle-status.json").write_text(json.dumps(statuses, indent=1) + "\n")
        self.measurements["idle_status_lines"] = len(statuses)
        if len(statuses) >= 2 and (rate := control_rate(statuses)) is not None:
            self.measurements["control_rate_hz"] = round(rate, 1)
        problems = idle_failures(statuses)
        if not re.search(r"armed=0 .* mot=0$", live):
            problems.append(f"`live` reported `{live}`")
        if problems:
            raise StepFailed("; ".join(problems))

    def live_change_lasts_until_reboot(self) -> None:
        with self.link() as link:
            before = config_value(link.expect("config get roll_p", "OK roll_p="), "roll_p")
            if before is None:
                raise StepFailed("`config get roll_p` gave no value")
            changed = before + 0.1
            link.expect(f"config set roll_p {changed:.4f}", "OK applied")
            now = config_value(link.expect("config get roll_p", "OK roll_p="), "roll_p")
        if now is None or abs(now - changed) > gates.VALUE_TOLERANCE:
            raise StepFailed(f"roll_p read {now} after being set to {changed:.4f}")
        self.reboot()
        with self.link() as link:
            after = config_value(link.expect("config get roll_p", "OK roll_p="), "roll_p")
        if after is None or abs(after - before) > gates.VALUE_TOLERANCE:
            raise StepFailed(f"an unsaved roll_p read {after} after a reboot, not {before}")


# --- The Foxeer ----------------------------------------------------------------


def foxeer_session(
    session: Session, candidate: Path, bench: Path, allow_dirty: bool, hands_free: bool
) -> None:
    def storage() -> None:
        with session.link() as link:
            link.expect("flash info", "OK jedec=")
            link.expect("logs list", "OK u=")
            link.expect("flash test CONFIRM", "OK flash scratch test started")
            link.wait_prefix("OK flash scratch erase/program/read verified", 15.0, "the scratch test")

    def config_gate() -> None:
        argv = [
            sys.executable,
            str(Path(__file__).with_name("foxeer_bench_gates.py")),
            "usb-config",
            "--elf",
            str(candidate),
            "--state-confirmed",
        ]
        if allow_dirty:
            argv.append("--allow-dirty")
        if hands_free:
            argv.append("--soft-reboot")
        code = subprocess.run(argv, cwd=REPO_ROOT).returncode
        session.commands.append({"command": "foxeer_bench_gates.py usb-config", "exit_code": code})
        if code != 0:
            raise StepFailed("the USB configuration gate did not pass")
        time.sleep(4.0)

    def bindings() -> None:
        table = lambda: port_table(json.loads(session.configurator_ok("--format", "json", "config", "ports")))  # noqa: E731
        before = table()
        for port, function, why in (
            ("uart3", "rc", "SBUS needs an inverter this port does not have"),
            ("uart2", "none", "RC would be left unbound"),
        ):
            refused = session.configurator("config", "bind", port, function)
            if refused.returncode == 0:
                raise StepFailed(f"`config bind {port} {function}` was accepted: {why}")
            if not refused_by_board(refused.stdout + refused.stderr):
                raise StepFailed(f"`config bind {port} {function}` failed, but not by the board's refusal: {refused.stderr.strip()}")
        if table() != before:
            raise StepFailed("a refused bind changed the table")
        session.configurator_ok("config", "bind", "uart4", "esc_telemetry")
        moved = table()
        if moved.get("uart4") != "esc_telemetry" or moved.get("uart1") != "none":
            raise StepFailed(f"binding esc_telemetry to uart4 left {moved}")
        session.configurator_ok("config", "bind", "uart1", "esc_telemetry")
        session.configurator_ok("config", "bind", "uart4", "osd")
        if table() != before:
            raise StepFailed(f"the table was not restored: {table()} against {before}")

    def reboot_and_dfu() -> None:
        with session.link() as link:
            before = int(link.collect_statuses(3.0)[-1]["ms"])
        session.reboot()
        with session.link() as link:
            statuses = link.collect_statuses(3.0)
        if not statuses or int(statuses[-1]["ms"]) >= before:
            raise StepFailed("`reboot` did not restart the board")
        session.flash(candidate)

    def calibration() -> None:
        print(
            "\nThis overwrites the Foxeer's stored accelerometer level, which is fine "
            "on a bench board."
        )
        gates.ask("Set the board flat and still, then press Enter:")
        print(f"Hands off: calibrating in {HANDS_OFF_S:.0f} s.")
        time.sleep(HANDS_OFF_S)
        with session.link() as link:
            link.expect("imu calibrate", "OK imu calibration started")
            result = link.wait_prefix("OK imu calibrated", 5.0, "the calibration")
            link.wait_prefix("OK config saved", 10.0, "the calibration's save")
            level = link.expect("imu level", "OK imu level")
        if result.split("calibrated ")[1] != level.split("level ")[1]:
            raise StepFailed(f"calibrated `{result}` but `imu level` reads `{level}`")
        session.reboot()
        with session.link() as link:
            if link.expect("imu level", "OK imu level") != level:
                raise StepFailed("the level did not survive a reboot")
        print("\nNow pick the board up and keep rocking it until the prompt returns.")
        gates.ask("Start rocking, then press Enter:")
        with session.link() as link:
            link.expect("imu calibrate", "OK imu calibration started")
            refused = link.wait_for(lambda line: line.startswith(("OK imu calibrated", "ERR ")), 5.0, "the moved calibration")
            if "the craft moved" not in refused:
                raise StepFailed(f"a moving board answered `{refused}`")
            if link.expect("imu level", "OK imu level") != level:
                raise StepFailed("a refused calibration changed the level")
            # Its inhibit was released, or this would wait on it and be refused.
            value = config_value(link.expect("config get roll_p", "OK roll_p="), "roll_p")
            link.expect(f"config set roll_p {value:.4f}", "OK applied")
        print("You can set the board down.")

    def fallback() -> None:
        session.flash(bench)
        session.require_identity((BENCH_FEATURE,))
        for command, state, finished in (
            ("bench invalid-config CONFIRM", "invalid", "OK config saved"),
            ("bench damage-config CONFIRM", "damaged", None),
        ):
            with session.link() as link:
                link.expect("config save", "OK config save started")
                link.wait_prefix("OK config saved", 10.0, "the save")
                value = link.expect("config get roll_p", "OK roll_p=")
                link.expect(command, "OK ")
                if finished is not None:
                    link.wait_prefix(finished, 10.0, f"`{command}`")
            session.reboot()
            with session.link() as link:
                reported = fallback_state(link.expect("config fallback", "OK config fallback="))
                if reported != state:
                    raise StepFailed(f"after `{command}` the boot reports fallback={reported}, not {state}")
                if link.expect("config get roll_p", "OK roll_p=") != value:
                    raise StepFailed(f"after `{command}` the boot did not fly the previous save")
                link.expect("config fallback ack", "OK fallback acknowledged")
                if fallback_state(link.expect("config fallback", "OK config fallback=")) != "none":
                    raise StepFailed("the acknowledgement did not clear the fallback")
                link.expect("config save", "OK config save started")
                link.wait_prefix("OK config saved", 10.0, "the repairing save")
            session.reboot()
            with session.link() as link:
                reported = fallback_state(link.expect("config fallback", "OK config fallback="))
                if reported != "none":
                    raise StepFailed(f"one save did not repair the slot: fallback={reported}")
        session.flash(candidate)

    baseline = session.capture / "baseline-before.toml"

    def identity() -> None:
        session.require_identity(())
        session.configurator_ok("config", "export", str(baseline))
        session.configurator_ok("blackbox", "flights")

    def configuration_kept() -> None:
        after = session.capture / "baseline-after.toml"
        session.configurator_ok("config", "export", str(after))
        compare_baselines(baseline, after)

    session.step("Flash the candidate over DFU; USB returns on its own", lambda: session.flash(candidate))
    session.step("Identity, configuration export and flight list", identity)
    session.step("Idle for 30 s, disarmed, with the loop at rate", session.idle)
    session.step("Storage: flash info, log list, scratch test", storage)
    def needs_hands(name: str, action: Callable[[], None]) -> None:
        if hands_free:
            print(f"\n--- {name}\nnot run: it needs hands")
            session.measurements.setdefault("not_run", []).append(name)
        else:
            session.step(name, action)

    session.step("USB configuration gate", config_gate)
    session.step("A live change lasts until a reboot unless saved", session.live_change_lasts_until_reboot)
    session.step("Bindings: refused, moved and restored", bindings)
    session.step("Reboot, and reboot to DFU", reboot_and_dfu)
    needs_hands("Calibration: a still board passes, a moving one is refused", calibration)
    session.step("Fallback: invalid and damaged saves, acknowledged and repaired", fallback)
    session.step("The configuration is as the session found it", configuration_kept)


# --- The Lucid ------------------------------------------------------------------


def lucid_session(session: Session, candidate: Path) -> None:
    def no_store() -> None:
        with session.link() as link:
            state = fallback_state(link.expect("config fallback", "OK config fallback="))
            if state != "unreadable":
                raise StepFailed(f"with no card the boot reports fallback={state}, not unreadable")
            link.expect("config save", "ERR storage unavailable")
            link.expect("config fallback ack", "OK fallback acknowledged")
            if fallback_state(link.expect("config fallback", "OK config fallback=")) != "none":
                raise StepFailed("the acknowledgement did not clear the fallback")
        session.live_change_lasts_until_reboot()
        with session.link() as link:
            state = fallback_state(link.expect("config fallback", "OK config fallback="))
        if state != "unreadable":
            raise StepFailed(f"after a reboot the boot reports fallback={state}, not unreadable")

    def reboot_to_dfu() -> None:
        # The handover's warning: this reboot's request may not survive the
        # reset on the H7. A failure here ends at the BOOT button.
        session.flash(candidate)

    session.step("Flash the candidate over DFU; USB returns on its own", lambda: session.flash(candidate))
    session.step("Identity", lambda: session.require_identity(()))
    session.step("Idle for 30 s, disarmed, with the loop at rate", session.idle)
    session.step("No card: an unreadable store falls back, and commands still answer", no_store)
    session.step("Reboot to DFU, with no store", reboot_to_dfu)


# --- Building, and the record ------------------------------------------------------


def build(board: Board, capture: Path, name: str, features: tuple[str, ...]) -> Path:
    print(f"Building {board.target} {name}...")
    argv = ["cargo", "build", "--release", "--locked", "--target", TARGET_TRIPLE]
    if features:
        argv += ["--features", " ".join(features)]
    result = subprocess.run(argv, cwd=board.app_dir, capture_output=True, text=True)
    if result.returncode != 0:
        raise SessionStop(f"the build failed:\n{result.stderr[-2000:]}")
    built = board.app_dir / "target" / TARGET_TRIPLE / "release" / board.elf
    # Keep the exact image: its hash depends on the checkout path, so a rebuild
    # elsewhere is not the same evidence.
    kept = capture / f"{board.elf}-{name}.elf"
    shutil.copy2(built, kept)
    return kept


def write_summary(session: Session) -> Path:
    path = session.capture / "summary.json"
    path.write_text(
        json.dumps(
            {
                "board": session.board.target,
                "steps": [step.__dict__ for step in session.steps],
                "measurements": session.measurements,
            },
            indent=2,
        )
        + "\n"
    )
    return path


def record_foxeer(session: Session, candidate: Path, bench: Path, notes: str, summary: Path) -> int:
    run_details = reqtool_bridge.details(
        firmware_sha256=gates.sha256_file(candidate),
        features=["board-foxeer-f405-v2"],
        target_triple=TARGET_TRIPLE,
        propellers="removed",
        actuator_power="disconnected",
        equipment=["USB host power", "no SWD probe", "no oscilloscope or logic analyzer"],
        environment=["bench", "battery disconnected", "no RC link"],
        commands=session.commands,
        artifacts=[
            (candidate, "firmware"),
            (bench, "firmware"),
            (session.transcript.path, "log"),
            (summary, "log"),
        ],
        measurements=session.measurements,
        observations=[f"{step.name}: passed." for step in session.steps]
        + ([f"Operator note: {notes}"] if notes else [])
        + ["Run by tools/usb_bench.py session."],
        limitations=[
            "USB shows a boot only once the port is up; motor outputs were not measured at the pins.",
            "Arming refusals were not observed: USB itself blocks arming, and the reasons go to RTT.",
            f"The fallback checks ran on the same commit built with {BENCH_FEATURE}.",
        ],
    )
    if notes:
        details_path = session.capture / reqtool_bridge.DETAILS_FILE
        details_path.write_text(json.dumps(run_details, indent=2) + "\n")
        print("\nSession INCONCLUSIVE: not recorded. After review, record it with:")
        print("  " + " ".join(reqtool_bridge.command(PROCEDURE, session.board.target, "pass", details_path)))
        return 1
    try:
        report = reqtool_bridge.record(PROCEDURE, session.board.target, "pass", run_details, session.capture)
    except reqtool_bridge.RecordError as error:
        print(f"\nSession PASSED, but reqtool did not record it: {error}")
        return 1
    print(f"\nSession PASS, recorded for review. {report}")
    return 0


def run_session(args: argparse.Namespace) -> int:
    board = BOARDS[args.board]
    before = reqtool_bridge.source_state()
    if before[1] and not args.allow_dirty:
        print("SESSION STOPPED: the checkout has uncommitted changes; commit them or pass --allow-dirty")
        return 1
    started = gates.utc_now()
    capture = BENCH_LOGS / f"{started:%Y%m%dT%H%M%SZ}-{board.target}-usb-session"
    capture.mkdir(parents=True, exist_ok=False)
    session = Session(board, capture, Transcript(capture / "session.log"))
    try:
        built = subprocess.run(
            ["cargo", "build", "--release", "--locked"],
            cwd=REPO_ROOT / "tools/ferro-configurator",
            capture_output=True,
            text=True,
        )
        if built.returncode != 0:
            raise SessionStop(f"the configurator did not build:\n{built.stderr[-2000:]}")
        candidate = build(board, capture, "candidate", ())
        bench = build(board, capture, "bench", (BENCH_FEATURE,)) if board.has_store else None
        if args.operator_confirmed_state:
            print("\nThe operator confirmed the bench state ahead of the run.")
            session.transcript.write("operator confirmed the bench state ahead of the run")
        else:
            gates.confirm_bench_state()
        if board.has_store:
            assert bench is not None
            foxeer_session(session, candidate, bench, args.allow_dirty, args.operator_confirmed_state)
        else:
            if not args.operator_confirmed_state and not gates.ask_yes_no("Is this board's SD card out?"):
                raise SessionStop("this session expects the card out, for the no-store checks")
            lucid_session(session, candidate)
        # An agent running this for the operator asks them afterwards.
        notes = "" if args.operator_confirmed_state else gates.operator_notes()
        if reqtool_bridge.source_state() != before:
            raise SessionStop("the source changed during the run")
    except (SessionStop, gates.GateStop) as stop:
        write_summary(session)
        print(f"\nSESSION STOPPED: {stop}")
        print(f"Captures kept in {capture.relative_to(REPO_ROOT)}")
        return 1

    summary = write_summary(session)
    print("\n=== Summary")
    for step in session.steps:
        print(f"{'pass' if step.passed else 'FAIL'}  {step.name}" + (f": {step.detail}" if step.detail else ""))
    print(f"Captures kept in {capture.relative_to(REPO_ROOT)}")
    if not all(step.passed for step in session.steps):
        print("\nSession FAILED: not recorded.")
        return 1
    if bench is None:
        print("\nSession passed. A Lucid session is captures only: nothing recorded.")
        return 0 if not notes else 1
    if args.operator_confirmed_state:
        print("\nSession passed without the steps that need hands: captures only, nothing recorded.")
        return 0
    return record_foxeer(session, candidate, bench, notes, summary)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    session = commands.add_parser("session", help="Run the USB session on the connected board.")
    session.add_argument("--board", choices=sorted(BOARDS), required=True)
    session.add_argument(
        "--operator-confirmed-state",
        action="store_true",
        help="The operator has already confirmed the bench state, and for a Lucid that its card is out, "
        "so nobody is asked. A Foxeer leaves out the steps that need hands and records nothing.",
    )
    session.add_argument("--allow-dirty", action="store_true", help="Run from a checkout with uncommitted changes.")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    return run_session(args)


if __name__ == "__main__":
    raise SystemExit(main())
