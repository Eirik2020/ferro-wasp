#!/usr/bin/env python3
"""Run BENCH-COMMON-001, the boot and idle gate, on any flight board over SWD.

`boot-idle --board <target>` builds the board's release image from the
checkout, flashes it and captures a boot from SWD reset, checks the status
LEDs at the pins where the board drives any, then asks the operator for one
USB power cycle, notices it over SWD and captures that boot too. The operator
confirms the bench state once and unplugs and replugs USB once; everything
else is driven and judged here. Captures land in a new directory under
`logs/bench/` and a schema 2 run record naming the components that booted is
written for review; nothing is committed.

`check-boot-idle` applies the same judgement to captures that already exist.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

if __package__:
    from . import components as component_ids
else:
    import components as component_ids

REPO_ROOT = Path(__file__).resolve().parents[1]
CATALOG = REPO_ROOT / "project_meta/testing/TEST_CATALOG.json"
COMPONENTS = REPO_ROOT / "project_meta/testing/COMPONENTS.json"
RUNS_DIR = REPO_ROOT / "project_meta/testing/evidence/runs"
BENCH_LOGS = REPO_ROOT / "logs/bench"
TEST_ID = "BENCH-COMMON-001"
TARGET_TRIPLE = "thumbv7em-none-eabihf"
# The Cortex-M CPUID register: readable whenever the core has power.
CPUID = "0xE000ED00"


@dataclass(frozen=True)
class Leds:
    """A board's status LEDs, by bit in one GPIO output data register."""

    odr_address: int
    heartbeat_bit: int
    armed_bit: int
    active_low: bool


@dataclass(frozen=True)
class Board:
    target: str
    name: str
    chip: str
    elf: str
    backend: str
    features: tuple[str, ...]
    # How the app names itself in its IMU bring-up lines.
    imu_log_prefix: str
    # Warnings a healthy disarmed boot on the bench prints on this board.
    expected_warnings: tuple[str, ...] = ()
    leds: Leds | None = None

    @property
    def app_dir(self) -> Path:
        return REPO_ROOT / "firmware" / self.target


# Warnings any board prints on a healthy bench boot: the control loop's first
# ticks run before the IMU's first sample, and ESC telemetry retries while no
# ESC has power.
COMMON_EXPECTED_WARNINGS = (
    r"IMU stale in control loop: seq 0, stale ticks \d+$",
    r"Foxeer ESC telemetry stopped after (actuator acknowledgement|response) timeout for "
    r"physical output \d \(logical M\d\), request \d+; retrying while disarmed$",
)

BOARDS = {
    "foxeer-f405-v2": Board(
        target="foxeer-f405-v2",
        name="Foxeer F405 V2",
        chip="STM32F405RG",
        elf="FerroWaspFoxeerF405V2",
        backend="stm32f4",
        features=("board-foxeer-f405-v2",),
        imu_log_prefix="Foxeer",
    ),
    "tbs-lucid-h7": Board(
        target="tbs-lucid-h7",
        name="TBS Lucid H7",
        chip="STM32H743VITx",
        elf="FerroWaspTbsLucidH7",
        backend="stm32h7",
        features=("board-tbs-lucid-h7",),
        imu_log_prefix="TBS Lucid H7",
        expected_warnings=(
            r"Flight arming inhibited: TBS Lucid H7 IMU orientation, motor order, and ADC "
            r"scale are not bench-verified$",
        ),
        # GPIOE ODR: PE4 green heartbeat, PE3 blue armed, both active low.
        leds=Leds(odr_address=0x58021014, heartbeat_bit=4, armed_bit=3, active_low=True),
    ),
}

IMU_DRIVERS = {
    "MPU-6000": "driver-mpu6000",
    "ICM42688-P": "driver-icm42688p",
    "MPU6500": "driver-mpu6500",
}
# What a boot and idle run exercises besides the board's app, its backend and
# its IMU driver.
SHARED_COMPONENTS = (
    "stm32",
    "stm32-tasks",
    "core",
    "drivers",
    "flight",
    "flight-arming",
    "flight-control",
    "flight-esc",
    "flight-links",
    "io-core",
)
STORAGE_UNAVAILABLE = r"(SD storage unavailable|SPI2 flash JEDEC probe failed).*storage"
# A boot's first DShot report comes 2 s after the scheduler starts; one later
# than this is not a fresh boot.
FRESH_BOOT_FIRST_REPORT_MS = 6000

DSHOT_REPORT = re.compile(
    r"DShot values \[(?P<values>[^\]]*)\].*?expired (?P<expired>\d+), "
    r"timeouts (?P<timeouts>\d+), faults (?P<faults>\d+), at (?P<at_ms>\d+) ms"
)
DEFMT_LINE = re.compile(r"^(?:DRONE: )?\[(?P<level>TRACE|DEBUG|INFO|WARN|ERROR)\s*\]\s*(?P<text>.*)$")


class GateStop(Exception):
    """A stop condition, or a check that failed: the gate ends here."""


# --- Judging a capture ---------------------------------------------------------


@dataclass
class BootEvidence:
    """What one RTT capture of the flight image shows."""

    board: Board
    allow_no_storage: bool = False
    init_ok: bool = False
    imu: str | None = None
    heartbeat: bool = False
    dshot_reports: int = 0
    dshot_first_ms: int | None = None
    dshot_bad: list[str] = field(default_factory=list)
    armed: bool = False
    storage_unavailable: bool = False
    fatal: list[str] = field(default_factory=list)
    unexpected_warnings: list[str] = field(default_factory=list)

    def observe(self, raw: str) -> None:
        match = DEFMT_LINE.match(raw.rstrip("\n"))
        if match is None:
            if "panicked" in raw.lower() or "hardfault" in raw.lower():
                self.fatal.append(raw.strip())
            return
        level, text = match.group("level"), match.group("text")
        name = self.board.name
        self.init_ok |= text == f"{name} system init successful"
        prefix = re.escape(self.board.imu_log_prefix)
        imu = re.match(rf"^{prefix} (MPU-6000|ICM42688-P|MPU6500) ready", text)
        if imu:
            self.imu = imu.group(1)
        self.heartbeat |= text == "Running heartbeat!"
        self.armed |= text == "SYSTEM ARMED"
        if level == "ERROR" or "panicked" in text.lower() or "hardfault" in text.lower():
            self.fatal.append(text)
        if level == "WARN":
            if re.search(STORAGE_UNAVAILABLE, text):
                self.storage_unavailable = True
                if not self.allow_no_storage:
                    self.unexpected_warnings.append(text)
            elif not any(
                re.search(pattern, text)
                for pattern in COMMON_EXPECTED_WARNINGS + self.board.expected_warnings
            ):
                self.unexpected_warnings.append(text)
        report = DSHOT_REPORT.search(text)
        if report:
            self.dshot_reports += 1
            at_ms = int(report.group("at_ms"))
            if self.dshot_first_ms is None:
                self.dshot_first_ms = at_ms
            values = [int(value) for value in report.group("values").split(",") if value.strip()]
            counters = [int(report.group(name)) for name in ("expired", "timeouts", "faults")]
            if any(values) or any(counters):
                self.dshot_bad.append(text)

    def failures(self, *, minimum_reports: int, from_reset: bool) -> list[str]:
        """Why this capture fails the gate; empty when it passes.

        A capture from reset must show the whole boot. One attached after a
        power cycle may have lost its first lines, so it must instead show
        that the board has just booted.
        """

        problems = []
        if from_reset:
            if not self.init_ok:
                problems.append("no successful system init")
            if self.imu is None:
                problems.append("the IMU never reported ready")
            if not self.heartbeat:
                problems.append("no heartbeat")
        elif self.dshot_first_ms is None or self.dshot_first_ms > FRESH_BOOT_FIRST_REPORT_MS:
            problems.append(
                f"first DShot report at {self.dshot_first_ms} ms: not a fresh boot"
            )
        if self.dshot_reports < minimum_reports:
            problems.append(f"{self.dshot_reports} DShot reports, fewer than {minimum_reports}")
        problems.extend(f"DShot not at stop: {line}" for line in self.dshot_bad)
        if self.armed:
            problems.append("the system armed")
        problems.extend(f"fatal output: {line}" for line in self.fatal)
        problems.extend(f"unexpected warning: {line}" for line in self.unexpected_warnings)
        return problems


def read_capture(path: Path, board: Board, *, allow_no_storage: bool) -> BootEvidence:
    evidence = BootEvidence(board, allow_no_storage=allow_no_storage)
    for line in path.read_text(errors="replace").splitlines():
        evidence.observe(line)
    return evidence


def led_failures(leds: Leds, samples: list[int]) -> list[str]:
    """The heartbeat must change level between samples and the armed LED must
    stay dark while the board is disarmed."""

    heartbeat = {(sample >> leds.heartbeat_bit) & 1 for sample in samples}
    armed_dark_level = 1 if leds.active_low else 0
    armed = {(sample >> leds.armed_bit) & 1 for sample in samples}
    problems = []
    if len(heartbeat) < 2:
        problems.append("the heartbeat LED pin never changed level")
    if armed != {armed_dark_level}:
        problems.append("the armed LED pin was lit while disarmed")
    return problems


# --- The probe and the operator ---------------------------------------------------


def probe_args(board: Board) -> list[str]:
    return ["--chip", board.chip, "--protocol", "swd"]


def core_powered(board: Board) -> bool:
    result = subprocess.run(
        ["probe-rs", "read", *probe_args(board), "b32", CPUID, "1"],
        capture_output=True,
        text=True,
    )
    return result.returncode == 0


def wait_for_power(board: Board, powered: bool, timeout_s: float) -> None:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if core_powered(board) == powered:
            return
        time.sleep(0.2)
    state = "come back" if powered else "lose power"
    raise GateStop(f"the board did not {state} within {timeout_s:.0f} s")


def capture(argv: list[str], path: Path, seconds: int) -> int:
    with path.open("w") as output:
        process = subprocess.Popen(argv, stdout=output, stderr=subprocess.STDOUT, cwd=REPO_ROOT)
        try:
            return process.wait(timeout=seconds)
        except subprocess.TimeoutExpired:
            process.terminate()
            return process.wait(timeout=30)


def sample_leds(board: Board, leds: Leds, count: int, interval_s: float) -> list[int]:
    samples = []
    for _ in range(count):
        result = subprocess.run(
            ["probe-rs", "read", *probe_args(board), "b32", hex(leds.odr_address), "1"],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise GateStop(f"could not read the LED pins: {result.stderr.strip()}")
        samples.append(int(result.stdout.split()[-1], 16))
        time.sleep(interval_s)
    return samples


def ask(prompt: str) -> str:
    if not sys.stdin.isatty():
        raise GateStop("this gate needs an operator at the terminal")
    return input(f"\n>>> {prompt} ").strip()


def ask_yes_no(question: str) -> bool:
    """A yes or a no; anything else is asked again, never guessed."""

    while True:
        answer = ask(f"{question} [yes/no]").lower()
        if answer in ("yes", "y"):
            return True
        if answer in ("no", "n"):
            return False
        print("Please answer yes or no.")


def confirm_bench_state() -> None:
    print("\nBefore anything runs, confirm the bench state for this gate:")
    print("  - propellers removed")
    print("  - ESC power and flight battery disconnected")
    print("  - board powered from USB only, sitting still")
    if not ask_yes_no("Do all three hold?"):
        raise GateStop("the operator did not confirm the bench state")


def operator_notes() -> str:
    """What the operator saw, or empty when they saw nothing abnormal."""

    if not ask_yes_no("Did you see anything abnormal (motor activity, heat, smell, LEDs)?"):
        return ""
    while not (description := ask("Describe what you saw:")):
        print("A description is needed, since something was abnormal.")
    return description


# --- Building and recording --------------------------------------------------------


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def utc_now() -> datetime:
    return datetime.now(UTC).replace(microsecond=0)


def iso(moment: datetime) -> str:
    return moment.strftime("%Y-%m-%dT%H:%M:%SZ")


def git(*args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(REPO_ROOT), *args], capture_output=True, text=True, check=True
    ).stdout.strip()


def build(board: Board, capture_dir: Path) -> Path:
    print(f"Building {board.target} (release, locked)...")
    result = subprocess.run(
        ["cargo", "build", "--release", "--locked", "--target", TARGET_TRIPLE],
        cwd=board.app_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise GateStop(f"the build failed:\n{result.stderr[-2000:]}")
    built = board.app_dir / "target" / TARGET_TRIPLE / "release" / board.elf
    # Keep the exact image: its hash depends on the checkout path, so a
    # rebuild elsewhere is not the same evidence.
    kept = capture_dir / f"{board.elf}.elf"
    shutil.copy2(built, kept)
    return kept


def run_components(board: Board, imu: str | None) -> list[str]:
    names = [f"app-{board.target}", board.backend, *SHARED_COMPONENTS]
    if imu is not None:
        names.append(IMU_DRIVERS[imu])
    return names


def content_ids() -> dict[str, str]:
    data = json.loads(COMPONENTS.read_text(encoding="utf-8"))
    current, uncovered = component_ids.current_content_ids(REPO_ROOT, data)
    if uncovered:
        raise GateStop(f"source files belong to no component: {', '.join(uncovered)}")
    return current


def catalog_entry() -> dict[str, Any]:
    for entry in json.loads(CATALOG.read_text())["tests"]:
        if entry["id"] == TEST_ID:
            return entry
    raise GateStop(f"{TEST_ID} is not in the test catalog")


def artifact_id(path: Path) -> str:
    words = re.sub(r"[^a-z0-9]+", "-", path.stem.lower()).strip("-")
    if not words[:1].isalpha():
        words = f"capture-{words}"
    return words[:64].rstrip("-")


def write_record(
    *,
    board: Board,
    started: datetime,
    completed: datetime,
    result: str,
    revision: str,
    working_tree: str,
    elf: Path,
    components: dict[str, str],
    commands: list[dict[str, Any]],
    artifacts: list[tuple[Path, str]],
    measurements: dict[str, Any],
    observations: list[str],
    limitations: list[str],
) -> Path:
    entry = catalog_entry()
    month_dir = RUNS_DIR / f"{completed:%Y}" / f"{completed:%m}"
    month_dir.mkdir(parents=True, exist_ok=True)
    stamp = completed.strftime("%Y%m%dT%H%M%SZ")
    sequence = 1
    while (month_dir / f"{stamp}__{TEST_ID}__{board.target}__{sequence:02d}.json").exists():
        sequence += 1
    run_id = f"{stamp}__{TEST_ID}__{board.target}__{sequence:02d}"
    record = {
        "schema_version": 2,
        "run_id": run_id,
        "test_id": TEST_ID,
        "started_at_utc": iso(started),
        "completed_at_utc": iso(completed),
        "target": board.target,
        "tier": entry["tier"],
        "result": result,
        "definition": {
            "catalog_sha256": sha256_file(CATALOG),
            "procedure_path": entry["procedure"]["path"],
            "procedure_heading": entry["procedure"]["heading"],
            "procedure_sha256": sha256_file(REPO_ROOT / entry["procedure"]["path"]),
        },
        "execution": {"performed_by": "user", "user_confirmed": True},
        "candidate": {
            "revision": revision,
            "working_tree": working_tree,
            "firmware_sha256": sha256_file(elf),
            "features": list(board.features),
            "target_triple": TARGET_TRIPLE,
            "configuration": {"profile": "release", "firmware_sha256_of": "ELF"},
        },
        "components": [
            {"id": name, "content_sha256": digest} for name, digest in components.items()
        ],
        "conditions": {
            "propellers": "removed",
            "actuator_power": "disconnected",
            "equipment": ["SWD probe", "USB host power", "no logic analyzer"],
            "environment": ["bench", "battery disconnected", "no RC link"],
        },
        "commands": commands,
        "artifacts": [
            {
                "id": artifact_id(path),
                "kind": kind,
                "path": path.relative_to(REPO_ROOT).as_posix(),
                "bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            }
            for path, kind in artifacts
        ],
        "measurements": measurements,
        "observations": observations,
        "limitations": limitations,
        "stop_conditions_triggered": [],
    }
    path = month_dir / f"{run_id}.json"
    path.write_text(json.dumps(record, indent=2) + "\n")
    return path


# --- The gate --------------------------------------------------------------------


def run_boot_idle(args: argparse.Namespace) -> int:
    board = BOARDS[args.board]
    working_tree = "clean" if not git("status", "--porcelain") else "dirty"
    if working_tree == "dirty" and not args.allow_dirty:
        print("GATE STOPPED: the checkout has uncommitted changes; commit them or pass --allow-dirty")
        return 1
    revision = git("rev-parse", "HEAD")
    started = utc_now()
    capture_dir = BENCH_LOGS / f"{started:%Y%m%dT%H%M%SZ}-{board.target}-boot-idle"
    capture_dir.mkdir(parents=True, exist_ok=False)
    commands: list[dict[str, Any]] = []
    try:
        before = content_ids()
        elf = build(board, capture_dir)
        commands.append({"command": "cargo build --release --locked", "exit_code": 0})
        confirm_bench_state()
        minimum_reports = max(1, args.seconds // 2 - 1)

        reset_log = capture_dir / "boot-1-swd-reset.log"
        print(f"\nBoot 1: flashing over SWD and capturing for {args.seconds} s.")
        code = capture(
            ["probe-rs", "run", *probe_args(board), "--no-location", str(elf)],
            reset_log,
            args.seconds + 15,
        )
        commands.append({"command": "probe-rs run <elf> (flash, SWD reset, capture)", "exit_code": code})
        reset = read_capture(reset_log, board, allow_no_storage=args.allow_no_storage)
        if problems := reset.failures(minimum_reports=minimum_reports, from_reset=True):
            raise GateStop("boot 1: " + "; ".join(problems))
        print(f"Boot 1: {reset.imu} ready, {reset.dshot_reports} DShot reports at stop.")

        led_samples: list[int] = []
        if board.leds is not None:
            led_samples = sample_leds(board, board.leds, count=12, interval_s=0.25)
            commands.append({"command": "probe-rs read <LED output register> x12", "exit_code": 0})
            if problems := led_failures(board.leds, led_samples):
                raise GateStop("LEDs: " + "; ".join(problems))
            print("LEDs: heartbeat pin toggling, armed pin dark.")

        print("\nCold boot: unplug the board's USB now.")
        wait_for_power(board, False, 120)
        print("Unplugged. Wait a few seconds, then plug it back in and keep it still.")
        wait_for_power(board, True, 120)
        cold_log = capture_dir / "boot-2-usb-power-cycle.log"
        code = capture(
            ["probe-rs", "attach", *probe_args(board), "--no-location", str(elf)],
            cold_log,
            args.seconds,
        )
        commands.append({"command": "probe-rs attach <elf> (no reset, capture)", "exit_code": code})
        cold = read_capture(cold_log, board, allow_no_storage=args.allow_no_storage)
        if problems := cold.failures(minimum_reports=minimum_reports, from_reset=False):
            raise GateStop("boot 2: " + "; ".join(problems))
        print(f"Boot 2: fresh boot, first DShot report at {cold.dshot_first_ms} ms, outputs at stop.")
        notes = operator_notes()
        if content_ids() != before:
            raise GateStop("the source changed during the run")
    except GateStop as stop:
        print(f"\nGATE STOPPED: {stop}")
        print(f"Captures kept in {capture_dir.relative_to(REPO_ROOT)}")
        return 1

    completed = utc_now()
    result = "pass" if not notes else "inconclusive"
    names = run_components(board, reset.imu)
    observations = [
        f"Boot from SWD reset: system init, {reset.imu} ready, heartbeat; every DShot report "
        "[0, 0, 0, 0] with no expired commands, timeouts or faults; not armed.",
        f"Boot from a USB power cycle, noticed over SWD: first DShot report at "
        f"{cold.dshot_first_ms} ms, every report at stop, not armed.",
        "Run by tools/bench_rig.py boot-idle.",
    ]
    if led_samples:
        observations.append(
            "Status LED pins read over SWD: the heartbeat pin toggled and the armed pin stayed dark."
        )
    if notes:
        observations.append(f"Operator note: {notes}")
    limitations = [
        "Reset is the SWD system reset after flashing; brownout was not tested.",
        "Motor outputs at stop are the firmware's own DShot reports, not measured at the pins.",
        "The power-cycle capture attaches after USB returns, so its first lines may be missing; "
        "the fresh boot is shown by the first DShot report's timestamp.",
    ]
    if board.leds is None:
        limitations.append("This board drives no status LED; the RTT heartbeat stands in.")
    if reset.storage_unavailable or cold.storage_unavailable:
        limitations.append("Storage was unavailable (no card or flash fitted); storage not exercised.")
    record = write_record(
        board=board,
        started=started,
        completed=completed,
        result=result,
        revision=revision,
        working_tree=working_tree,
        elf=elf,
        components={name: before[name] for name in names},
        commands=commands,
        artifacts=[(elf, "firmware"), (reset_log, "log"), (cold_log, "log")],
        measurements={
            "dshot_zero_output_reports": reset.dshot_reports + cold.dshot_reports,
            "dshot_nonzero_output_reports": 0,
            "power_cycle_first_dshot_report_ms": cold.dshot_first_ms,
            "led_samples": len(led_samples),
        },
        observations=observations,
        limitations=limitations,
    )
    print(f"\n{TEST_ID} {result.upper()}. Draft record: {record.relative_to(REPO_ROOT)}")
    return 0 if result == "pass" else 1


def check_boot_idle(args: argparse.Namespace) -> int:
    board = BOARDS[args.board]
    failed = False
    for path in args.logs:
        evidence = read_capture(path, board, allow_no_storage=args.allow_no_storage)
        problems = evidence.failures(
            minimum_reports=args.minimum_reports,
            from_reset=not args.power_cycle,
        )
        failed |= bool(problems)
        print(f"{path}: {'FAIL' if problems else 'pass'}")
        for problem in problems:
            print(f"  - {problem}")
    return 1 if failed else 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name, help_text in (
        ("boot-idle", f"Run {TEST_ID} on the connected board."),
        ("check-boot-idle", "Judge existing captures."),
    ):
        command = commands.add_parser(name, help=help_text)
        command.add_argument("--board", choices=sorted(BOARDS), required=True)
        command.add_argument(
            "--allow-no-storage",
            action="store_true",
            help="accept a board with no storage card or flash; recorded as a limitation",
        )
        if name == "boot-idle":
            command.add_argument("--seconds", type=int, default=20, help="capture length per boot")
            command.add_argument(
                "--allow-dirty",
                action="store_true",
                help="run on uncommitted changes; the record says so",
            )
        else:
            command.add_argument("logs", type=Path, nargs="+")
            command.add_argument("--minimum-reports", type=int, default=5)
            command.add_argument(
                "--power-cycle",
                action="store_true",
                help="the captures attached after a power cycle rather than from reset",
            )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.command == "boot-idle":
        return run_boot_idle(args)
    return check_boot_idle(args)


if __name__ == "__main__":
    raise SystemExit(main())
