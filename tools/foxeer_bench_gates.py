#!/usr/bin/env python3
"""Run and check the Foxeer bench gates the operator only powers and cables.

`boot-idle` runs BENCH-COMMON-001 and `usb-config` runs BENCH-FOX-USB-001 on
the connected board: the operator confirms the safety state once and performs
each power cycle when asked, and everything else is driven and judged here.
Every capture lands in a new directory under `logs/bench/`, and a draft run
record is written for review; nothing is committed.

`check-boot-idle` and `check-usb-config` apply the same judgement to captures
that already exist, so a manual run and an automated one are held to the same
standard.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import signal
import subprocess
import sys
import time
import tomllib
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
CATALOG = REPO_ROOT / "project_meta/testing/TEST_CATALOG.json"
RUNS_DIR = REPO_ROOT / "project_meta/testing/evidence/runs"
BENCH_LOGS = REPO_ROOT / "logs/bench"
CONFIGURATOR = REPO_ROOT / "tools/ferro-configurator/target/release/ferro-configurator"
TERMINAL_EMBED = REPO_ROOT / "tools/terminal_embed.py"

USB_VID = 0x16C0
USB_PID = 0x27DD
USB_SERIAL = "FW-FOX-F405V2"

# The reviewed baseline in project_meta/testing/targets/foxeer-f405-v2.md.
# imu_lpf_hz and log_rate_divisor are not part of it: a gate only requires
# them to stay as it found them.
REVIEWED_BASELINE = {
    "roll_p": 2.5,
    "roll_i": 0.0,
    "roll_d": 0.0,
    "pitch_p": 2.5,
    "pitch_i": 0.0,
    "pitch_d": 0.0,
    "yaw_p": 2.0,
    "yaw_i": 0.0,
    "yaw_d": 0.0,
    "roll_center_rate": 70.0,
    "pitch_center_rate": 70.0,
    "yaw_center_rate": 70.0,
    "roll_max_rate": 300.0,
    "pitch_max_rate": 300.0,
    "yaw_max_rate": 200.0,
    "roll_expo": 0.5,
    "pitch_expo": 0.5,
    "yaw_expo": 0.5,
    "rc_deadband": 8.0,
}
TEMPORARY_ROLL = {"roll_max_rate": 250.0, "roll_center_rate": 60.0, "roll_expo": 0.4}
INVALID_SETTING = ("roll-expo", "1.1")
VALUE_TOLERANCE = 1e-3

# The procedure's snapshots, in order. A manual run used the same names.
USB_SNAPSHOTS = {
    "initial": "1-initial.txt",
    "reject": "3-reject.txt",
    "same_boot": "5-same-boot.txt",
    "cold_boot_temporary": "6-cold-boot-temp.txt",
    "restored": "7-restored.txt",
    "restored_cold_boot": "8-restored-cold.txt",
}
BASELINE_EXPORT = "foxeer-baseline.toml"

# Warnings every healthy boot on the bench prints: one stale tick before the
# first IMU sample, and ESC telemetry giving up on an ESC with no power.
EXPECTED_WARNINGS = (
    re.compile(r"IMU stale in control loop: seq 0, stale ticks 1$"),
    re.compile(
        r"Foxeer ESC telemetry manager latched fault after response timeout "
        r"for physical output \d \(logical M\d\), request \d+$"
    ),
)
DSHOT_REPORT = re.compile(
    r"Foxeer DShot values \[(?P<values>[^\]]*)\].*?expired (?P<expired>\d+), "
    r"timeouts (?P<timeouts>\d+), faults (?P<faults>\d+), at (?P<at_ms>\d+) ms"
)
DEFMT_LEVEL = re.compile(r"^\[(?P<level>TRACE|DEBUG|INFO|WARN|ERROR)\s*\]\s*(?P<text>.*)$")


class GateStop(Exception):
    """A stop condition, or a check that failed: the gate ends here."""


# --- Configuration snapshots -------------------------------------------------


def flatten_config(document: dict[str, Any]) -> dict[str, float]:
    """One flat `key -> value` map, keyed as the firmware names settings."""
    flat: dict[str, float] = {}
    for key, value in document.items():
        if key == "schema_version":
            continue
        if isinstance(value, dict):
            for term, gain in value.items():
                flat[f"{key}_{term}"] = float(gain)
        else:
            flat[key] = float(value)
    return flat


def parse_snapshot(text: str) -> tuple[int | None, dict[str, float]]:
    """A saved `config show`, optionally led by an `exit N` line."""
    exit_code = None
    first, _, rest = text.partition("\n")
    match = re.fullmatch(r"exit (-?\d+)", first.strip())
    if match:
        exit_code = int(match.group(1))
        text = rest
    return exit_code, flatten_config(tomllib.loads(text))


def mismatches(expected: dict[str, float], actual: dict[str, float]) -> list[str]:
    problems = []
    for key, want in expected.items():
        have = actual.get(key)
        if have is None:
            problems.append(f"{key} missing")
        elif abs(have - want) > VALUE_TOLERANCE:
            problems.append(f"{key} is {have:g}, expected {want:g}")
    for key in actual.keys() - expected.keys():
        problems.append(f"{key} unexpected")
    return problems


def check_usb_snapshots(snapshots: dict[str, tuple[int | None, dict[str, float]]]) -> list[str]:
    """Every requirement of BENCH-FOX-USB-001 that its snapshots carry."""
    failures: list[str] = []

    def require(step: str, expected: dict[str, float]) -> None:
        if step not in snapshots:
            failures.append(f"{step}: snapshot missing")
            return
        failures.extend(f"{step}: {problem}" for problem in mismatches(expected, snapshots[step][1]))

    if "initial" not in snapshots:
        return ["initial: snapshot missing"]
    initial = snapshots["initial"][1]
    failures.extend(
        f"initial: {problem}"
        for problem in mismatches(
            REVIEWED_BASELINE, {k: v for k, v in initial.items() if k in REVIEWED_BASELINE}
        )
    )
    for key in REVIEWED_BASELINE.keys() - initial.keys():
        failures.append(f"initial: {key} missing")

    reject_exit = snapshots.get("reject", (None, {}))[0]
    if reject_exit is None:
        failures.append("reject: exit code not recorded")
    elif reject_exit == 0:
        failures.append("reject: the out-of-range value was accepted")
    require("reject", initial)

    temporary = {**initial, **TEMPORARY_ROLL}
    require("same_boot", temporary)
    require("cold_boot_temporary", temporary)
    require("restored", initial)
    require("restored_cold_boot", initial)
    return failures


def load_usb_snapshots(directory: Path) -> dict[str, tuple[int | None, dict[str, float]]]:
    loaded = {}
    for step, name in USB_SNAPSHOTS.items():
        path = directory / name
        if path.is_file() and path.stat().st_size:
            loaded[step] = parse_snapshot(path.read_text())
    return loaded


# --- Boot and idle logs --------------------------------------------------------


@dataclass
class BootIdleEvidence:
    """What one terminal_embed capture of the flight image shows."""

    elf_sha256: str | None = None
    init_ok: bool = False
    imu_ready: bool = False
    heartbeat: bool = False
    dshot_reports: int = 0
    dshot_last_ms: int = 0
    dshot_bad: list[str] = field(default_factory=list)
    armed: bool = False
    fatal: list[str] = field(default_factory=list)
    unexpected_warnings: list[str] = field(default_factory=list)
    expected_warnings: int = 0

    def observe(self, raw: str) -> None:
        line = raw.rstrip("\n")
        if line.startswith("HOST: firmware SHA-256 "):
            self.elf_sha256 = line.rsplit(" ", 1)[1].lower()
            return
        if not line.startswith("DRONE: "):
            return
        match = DEFMT_LEVEL.match(line[len("DRONE: ") :])
        if match is None:
            return
        level, text = match.group("level"), match.group("text")
        self.init_ok |= text == "Foxeer F405 V2 system init successful"
        self.imu_ready |= text.startswith(("Foxeer ICM42688-P ready", "Foxeer MPU6500 ready"))
        self.heartbeat |= text == "Running heartbeat!"
        self.armed |= text == "SYSTEM ARMED"
        if level == "ERROR" or "panicked" in text.lower() or "hardfault" in text.lower():
            self.fatal.append(text)
        if level == "WARN":
            if any(pattern.search(text) for pattern in EXPECTED_WARNINGS):
                self.expected_warnings += 1
            else:
                self.unexpected_warnings.append(text)
        report = DSHOT_REPORT.search(text)
        if report:
            self.dshot_reports += 1
            self.dshot_last_ms = int(report.group("at_ms"))
            values = [int(v) for v in report.group("values").split(",") if v.strip()]
            counters = [int(report.group(name)) for name in ("expired", "timeouts", "faults")]
            if any(values) or any(counters):
                self.dshot_bad.append(text)

    def failures(self, expected_elf_sha256: str | None, minimum_reports: int) -> list[str]:
        problems = []
        if expected_elf_sha256 is not None and self.elf_sha256 != expected_elf_sha256.lower():
            problems.append(f"ELF SHA-256 {self.elf_sha256} is not the candidate's {expected_elf_sha256}")
        if not self.init_ok:
            problems.append("no successful system init")
        if not self.imu_ready:
            problems.append("the IMU never reported ready")
        if not self.heartbeat:
            problems.append("no heartbeat")
        if self.dshot_reports < minimum_reports:
            problems.append(f"{self.dshot_reports} DShot reports, fewer than {minimum_reports}")
        problems.extend(f"DShot not at stop: {line}" for line in self.dshot_bad)
        if self.armed:
            problems.append("the system armed")
        problems.extend(f"fatal output: {line}" for line in self.fatal)
        problems.extend(f"unexpected warning: {line}" for line in self.unexpected_warnings)
        return problems


def read_boot_log(path: Path) -> BootIdleEvidence:
    evidence = BootIdleEvidence()
    for line in path.read_text(errors="replace").splitlines():
        evidence.observe(line)
    return evidence


# --- Hardware and operator -----------------------------------------------------


def ask(prompt: str) -> str:
    if not sys.stdin.isatty():
        raise GateStop("this gate needs an operator at the terminal")
    return input(f"\n>>> {prompt} ").strip()


def confirm_bench_state() -> None:
    print("\nBefore anything runs, confirm the bench state for this gate:")
    print("  - propellers removed")
    print("  - ESC power and flight battery disconnected")
    print("  - board powered from USB only, sitting still")
    if ask("Type 'yes' if all three hold:").lower() != "yes":
        raise GateStop("the operator did not confirm the bench state")


def operator_notes() -> str:
    return ask("Anything abnormal (motor activity, heat, smell, LEDs)? Enter for none, or describe it:")


def find_port() -> str | None:
    from serial.tools import list_ports

    for port in list_ports.comports():
        if port.vid == USB_VID and port.pid == USB_PID and port.serial_number == USB_SERIAL:
            return port.device
    return None


def wait_for_port(present: bool, timeout_s: float) -> str | None:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        port = find_port()
        if (port is not None) == present:
            return port
        time.sleep(0.2)
    state = "appear" if present else "disappear"
    raise GateStop(f"the board's USB port did not {state} within {timeout_s:.0f} s")


def power_cycle(settle_s: float) -> str:
    print("\nCold boot: unplug the board's USB now.")
    wait_for_port(False, 120)
    print("Unplugged. Wait a few seconds, then plug it back in and keep it still.")
    port = wait_for_port(True, 120)
    print(f"Back on {port}; letting gyro calibration finish ({settle_s:.0f} s).")
    time.sleep(settle_s)
    assert port is not None
    return port


@dataclass
class CommandLog:
    commands: list[dict[str, Any]] = field(default_factory=list)

    def run(self, argv: list[str], shown: str) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(argv, capture_output=True, text=True, cwd=REPO_ROOT)
        self.commands.append({"command": shown, "exit_code": result.returncode})
        return result


class Configurator:
    def __init__(self, port: str, log: CommandLog) -> None:
        self.port = port
        self.log = log

    def run(self, *args: str) -> subprocess.CompletedProcess[str]:
        argv = [str(CONFIGURATOR), "--port", self.port, *args]
        shown = " ".join(["ferro-configurator", "--port", "<foxeer>", *args])
        return self.log.run(argv, shown)

    def require(self, *args: str) -> str:
        result = self.run(*args)
        if result.returncode != 0:
            raise GateStop(f"`{' '.join(args)}` failed: {result.stderr.strip()}")
        return result.stdout

    def status(self) -> dict[str, Any]:
        output = self.require("--format", "json", "device", "info")
        return json.loads(output)["result"]["status"] or {}


# --- Records -------------------------------------------------------------------


def artifact_id(path: Path) -> str:
    """A record artifact ID: lowercase words joined by hyphens, led by a letter."""
    words = re.sub(r"[^a-z0-9]+", "-", path.stem.lower()).strip("-")
    if not words[:1].isalpha():
        words = f"capture-{words}"
    return words[:64].rstrip("-")


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def utc_now() -> datetime:
    return datetime.now(UTC).replace(microsecond=0)


def iso(moment: datetime) -> str:
    return moment.strftime("%Y-%m-%dT%H:%M:%SZ")


def new_capture_dir(label: str, started: datetime) -> Path:
    directory = BENCH_LOGS / f"{started.strftime('%Y%m%dT%H%M%SZ')}-{label}"
    directory.mkdir(parents=True, exist_ok=False)
    return directory


def load_build_record(path: Path) -> dict[str, Any]:
    record = json.loads(path.read_text())
    if record.get("test_id") != "BUILD-FOX-001" or record.get("result") != "pass":
        raise GateStop(f"{path} is not a passing BUILD-FOX-001 record")
    return record


def release_elf_sha256(build_record: dict[str, Any]) -> str:
    for artifact in build_record.get("artifacts", []):
        if artifact.get("id") == "release-elf":
            return artifact["sha256"]
    raise GateStop("the build record names no release-elf artifact")


def catalog_entry(test_id: str) -> dict[str, Any]:
    catalog = json.loads(CATALOG.read_text())
    tests = catalog["tests"] if isinstance(catalog, dict) else catalog
    for entry in tests:
        if entry["id"] == test_id:
            return entry
    raise GateStop(f"{test_id} is not in the test catalog")


def write_record(
    *,
    test_id: str,
    started: datetime,
    completed: datetime,
    result: str,
    build_record: dict[str, Any],
    commands: list[dict[str, Any]],
    artifacts: list[tuple[Path, str]],
    measurements: dict[str, Any],
    observations: list[str],
    limitations: list[str],
    configuration: dict[str, Any] | None = None,
) -> Path:
    entry = catalog_entry(test_id)
    procedure = REPO_ROOT / entry["procedure"]["path"]
    candidate = build_record["candidate"]
    month_dir = RUNS_DIR / f"{completed:%Y}" / f"{completed:%m}"
    month_dir.mkdir(parents=True, exist_ok=True)
    stamp = completed.strftime("%Y%m%dT%H%M%SZ")
    sequence = 1
    while (month_dir / f"{stamp}__{test_id}__foxeer-f405-v2__{sequence:02d}.json").exists():
        sequence += 1
    run_id = f"{stamp}__{test_id}__foxeer-f405-v2__{sequence:02d}"
    record = {
        "schema_version": 1,
        "run_id": run_id,
        "test_id": test_id,
        "started_at_utc": iso(started),
        "completed_at_utc": iso(completed),
        "target": "foxeer-f405-v2",
        "tier": entry["tier"],
        "result": result,
        "definition": {
            "catalog_sha256": sha256_file(CATALOG),
            "procedure_path": entry["procedure"]["path"],
            "procedure_heading": entry["procedure"]["heading"],
            "procedure_sha256": sha256_file(procedure),
        },
        "execution": {"performed_by": "user", "user_confirmed": True},
        "candidate": {
            "revision": candidate["revision"],
            "working_tree": candidate["working_tree"],
            "firmware_sha256": candidate["firmware_sha256"],
            "features": candidate["features"],
            "target_triple": candidate["target_triple"],
            "configuration": configuration or {},
        },
        "conditions": {
            "propellers": "removed",
            "actuator_power": "disconnected",
            "equipment": ["SWD probe", "USB host power", "no oscilloscope or logic analyzer"],
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


# --- The gates -----------------------------------------------------------------


def run_boot_idle(args: argparse.Namespace) -> int:
    build_record = load_build_record(args.build_record)
    elf_sha256 = release_elf_sha256(build_record)
    confirm_bench_state()
    started = utc_now()
    capture = new_capture_dir("boot-idle", started)
    log = CommandLog()
    logs: list[Path] = []
    minimum_reports = max(1, args.seconds // 2 - 1)
    shown = "python3 tools/terminal_embed.py --board foxeer-f405-v2 --release --locked"
    try:
        for boot in range(1, args.boots + 1):
            if boot > 1:
                power_cycle(settle_s=2)
            path = capture / f"boot-{boot}.log"
            print(f"\nBoot {boot}/{args.boots}: flashing and capturing for {args.seconds} s.")
            process = subprocess.Popen(
                [sys.executable, str(TERMINAL_EMBED), "--board", "foxeer-f405-v2", "--release", "--locked",
                 "--log-file", str(path)],
                cwd=REPO_ROOT,
                stdout=subprocess.DEVNULL,
            )
            booted = wait_for_line(path, "=== FLASH SUCCEEDED", process, timeout_s=120)
            if booted:
                time.sleep(args.seconds)
            process.send_signal(signal.SIGINT)
            exit_code = process.wait(timeout=30)
            log.commands.append(
                {"command": f"{shown} --log-file <capture>/boot-{boot}.log", "exit_code": exit_code}
            )
            logs.append(path)
            evidence = read_boot_log(path)
            problems = evidence.failures(elf_sha256, minimum_reports)
            if not booted:
                problems.insert(0, "the board never reported a successful flash and boot")
            if problems:
                raise GateStop(f"boot {boot}: " + "; ".join(problems))
            print(f"Boot {boot}: {evidence.dshot_reports} DShot reports at stop, no faults, not armed.")
        notes = operator_notes()
    except GateStop as stop:
        print(f"\nGATE STOPPED: {stop}")
        print(f"Captures kept in {capture.relative_to(REPO_ROOT)}")
        return 1
    completed = utc_now()
    result = "pass" if not notes else "inconclusive"
    evidence_all = [read_boot_log(path) for path in logs]
    observations = [
        f"Every capture printed ELF SHA-256 {elf_sha256[:8]}..., the candidate's release ELF.",
        f"{len(logs)} boots each reached system init, IMU ready and the heartbeat; every DShot report "
        "was [0, 0, 0, 0] with no expired commands, timeouts or faults; none armed or panicked.",
        "The only warnings were the expected boot-time IMU stale tick and ESC telemetry latching "
        "faulted with no ESC power.",
        "Run by tools/foxeer_bench_gates.py boot-idle; each capture ended by interrupting "
        "terminal_embed, then the operator cold-cycled USB before the next.",
    ]
    if notes:
        observations.append(f"Operator note: {notes}")
    record = write_record(
        test_id="BENCH-COMMON-001",
        started=started,
        completed=completed,
        result=result,
        build_record=build_record,
        commands=log.commands,
        artifacts=[(path, "log") for path in logs],
        measurements={
            "boot_cycles": len(logs),
            "dshot_zero_output_reports": sum(e.dshot_reports for e in evidence_all),
            "dshot_nonzero_output_reports": 0,
            "longest_capture_seconds": max(e.dshot_last_ms for e in evidence_all) // 1000 + 1,
        },
        observations=observations,
        limitations=[
            "No oscilloscope or logic analyzer: pulses before the actuator task initializes were not "
            "observed electrically; actuator power was disconnected throughout.",
            "Motor outputs at stop are the firmware's own DShot reports, not measurements at the pins.",
            "Each captured boot is the reset after probe-rs programs the image; the cold USB power "
            "cycles between captures were not captured.",
            "The LED heartbeat check does not apply to this board with SWD attached; the RTT heartbeat "
            "was used instead.",
        ],
    )
    print(f"\nBENCH-COMMON-001 {result.upper()}. Draft record: {record.relative_to(REPO_ROOT)}")
    return 0 if result == "pass" else 1


def wait_for_line(path: Path, marker: str, process: subprocess.Popen[bytes], timeout_s: float) -> bool:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if path.exists() and marker in path.read_text(errors="replace"):
            return True
        if process.poll() is not None:
            return False
        time.sleep(0.5)
    return False


def run_usb_config(args: argparse.Namespace) -> int:
    build_record = load_build_record(args.build_record)
    if not CONFIGURATOR.is_file():
        raise GateStop(f"build the configurator first: {CONFIGURATOR.relative_to(REPO_ROOT)}")
    port = find_port()
    if port is None:
        raise GateStop("no Foxeer on USB; plug it in and wait for it to enumerate")
    confirm_bench_state()
    flashed = ask("Has the board been flashed with this candidate since it was built? [yes/no]")
    if flashed.lower() != "yes":
        raise GateStop("run boot-idle first so the board runs the candidate")
    started = utc_now()
    capture = new_capture_dir("usb-config", started)
    log = CommandLog()
    tool = Configurator(port, log)

    def snapshot(step: str, prefix: str = "") -> dict[str, float]:
        output = tool.require("config", "show")
        (capture / USB_SNAPSHOTS[step]).write_text(prefix + output)
        return flatten_config(tomllib.loads(output))

    try:
        status = tool.status()
        if status.get("armed"):
            raise GateStop("the controller reports armed")
        snapshot("initial")
        tool.require("config", "export", str(capture / BASELINE_EXPORT))
        rejected = tool.run("config", "set", *INVALID_SETTING)
        (capture / "3-reject-stderr.txt").write_text(rejected.stderr)
        snapshot("reject", prefix=f"exit {rejected.returncode}\n")
        for key, value in TEMPORARY_ROLL.items():
            tool.require("config", "set", key.replace("_", "-"), f"{value:g}")
        snapshot("same_boot")
        tool.port = power_cycle(settle_s=3)
        snapshot("cold_boot_temporary")
        tool.require("config", "validate", str(capture / BASELINE_EXPORT))
        tool.require("config", "apply", str(capture / BASELINE_EXPORT))
        snapshot("restored")
        tool.port = power_cycle(settle_s=3)
        snapshot("restored_cold_boot")
        failures = check_usb_snapshots(load_usb_snapshots(capture))
        if failures:
            raise GateStop("; ".join(failures))
        if tool.status().get("armed"):
            raise GateStop("the controller reports armed")
        notes = operator_notes()
    except GateStop as stop:
        print(f"\nGATE STOPPED: {stop}")
        print(f"Captures kept in {capture.relative_to(REPO_ROOT)}; restore from its baseline export.")
        return 1
    completed = utc_now()
    result = "pass" if not notes else "inconclusive"
    initial = load_usb_snapshots(capture)["initial"][1]
    observations = [
        "Initial configuration matched the reviewed baseline; imu_lpf_hz "
        f"{initial['imu_lpf_hz']:g} and log_rate_divisor {initial['log_rate_divisor']:g} stayed as found.",
        f"`config set {' '.join(INVALID_SETTING)}` exited nonzero and the readback kept the initial profile.",
        "The temporary roll profile 60/250/0.4 read back in the same boot and after a cold USB power "
        "cycle, with every other value unchanged.",
        "The exported baseline validated, applied and read back identically before and after a cold "
        "USB power cycle.",
        "The controller reported disarmed before and after. Run by tools/foxeer_bench_gates.py usb-config.",
    ]
    if notes:
        observations.append(f"Operator note: {notes}")
    artifacts = [(capture / BASELINE_EXPORT, "configuration"), (capture / "3-reject-stderr.txt", "log")]
    artifacts += [(capture / name, "configuration") for name in USB_SNAPSHOTS.values()]
    record = write_record(
        test_id="BENCH-FOX-USB-001",
        started=started,
        completed=completed,
        result=result,
        build_record=build_record,
        commands=log.commands,
        artifacts=artifacts,
        measurements={"cold_boots": 2, "control_loop_hz": status.get("control_loop_hz")},
        observations=observations,
        limitations=[
            "The image on the board is taken from the operator's confirmation; USB does not report it.",
            "Cold boots were USB power cycles with actuator power disconnected; no brownout was induced.",
        ],
        configuration={"log_rate_divisor": int(initial["log_rate_divisor"])},
    )
    print(f"\nBENCH-FOX-USB-001 {result.upper()}. Draft record: {record.relative_to(REPO_ROOT)}")
    return 0 if result == "pass" else 1


def check_boot_idle(args: argparse.Namespace) -> int:
    failed = False
    for path in args.logs:
        problems = read_boot_log(path).failures(args.elf_sha256, args.minimum_reports)
        print(f"{path}: {'PASS' if not problems else 'FAIL'}")
        for problem in problems:
            print(f"  - {problem}")
        failed |= bool(problems)
    return 1 if failed else 0


def check_usb_config(args: argparse.Namespace) -> int:
    failures = check_usb_snapshots(load_usb_snapshots(args.directory))
    print(f"{args.directory}: {'PASS' if not failures else 'FAIL'}")
    for failure in failures:
        print(f"  - {failure}")
    return 1 if failures else 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)

    boot = commands.add_parser("boot-idle", help="Run BENCH-COMMON-001 on the connected Foxeer.")
    build_help = "The candidate's BUILD-FOX-001 record."
    boot.add_argument("--build-record", type=Path, required=True, help=build_help)
    boot.add_argument("--boots", type=int, default=3, help="Cold boots to capture (default 3).")
    boot.add_argument("--seconds", type=int, default=20, help="Capture length after each boot (default 20).")
    boot.set_defaults(handler=run_boot_idle)

    usb = commands.add_parser("usb-config", help="Run BENCH-FOX-USB-001 on the connected Foxeer.")
    usb.add_argument("--build-record", type=Path, required=True, help=build_help)
    usb.set_defaults(handler=run_usb_config)

    check_boot = commands.add_parser("check-boot-idle", help="Judge existing boot captures.")
    check_boot.add_argument("logs", type=Path, nargs="+")
    check_boot.add_argument("--elf-sha256", help="Require this ELF hash in every capture.")
    check_boot.add_argument("--minimum-reports", type=int, default=5)
    check_boot.set_defaults(handler=check_boot_idle)

    check_usb = commands.add_parser("check-usb-config", help="Judge an existing snapshot directory.")
    check_usb.add_argument("directory", type=Path)
    check_usb.set_defaults(handler=check_usb_config)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        return args.handler(args)
    except GateStop as stop:
        print(f"GATE NOT STARTED: {stop}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
