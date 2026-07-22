#!/usr/bin/env python3
"""Build, flash/run with probe-rs, and print decoded defmt RTT lines."""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import signal
import subprocess
import sys
import threading
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

ANSI_ESCAPE_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
DEFMT_LINE_RE = re.compile(
    r"^(?:\d+\.\d+\s+)?(?:\[(?:TRACE|DEBUG|INFO|WARN|ERROR)\s*\]|(?:TRACE|DEBUG|INFO|WARN|ERROR)\b)"
)
FIRMWARE_MARKERS = (
    "Begin system init",
    "System init successful",
    "FerroWasp RTT hello from drone",
    "Running heartbeat",
)
REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOG_DIR = REPO_ROOT / "logs" / "terminal_embed"


@dataclass(frozen=True)
class FirmwareTarget:
    app_root: Path
    binary_name: str
    chip: str


@dataclass
class FoxeerSmokeEvidence:
    init_ok: bool = False
    rtt_hello: bool = False
    arming_inhibited: bool = False
    imu_ready: bool = False
    drdy_in_range: int = 0
    fatal_line: str | None = None

    def observe(self, line: str) -> None:
        clean = ANSI_ESCAPE_RE.sub("", line)
        self.init_ok |= "Foxeer F405 V2 system init successful" in clean
        self.rtt_hello |= "FerroWasp RTT hello from Foxeer" in clean
        self.arming_inhibited |= "Flight arming inhibited" in clean
        self.imu_ready |= "Foxeer MPU6500 ready" in clean or "Foxeer ICM42688-P ready" in clean
        match = re.search(r"IMU DRDY IRQ \d+, delta (\d+)", clean)
        if match is not None and 1_500 <= int(match.group(1)) <= 2_500:
            self.drdy_in_range += 1
        if any(marker in clean.lower() for marker in ("panicked at", "hardfault", "[error")):
            self.fatal_line = clean

    def failures(self) -> list[str]:
        failures: list[str] = []
        if not self.init_ok:
            failures.append("missing successful Foxeer initialization")
        if not self.rtt_hello:
            failures.append("missing Foxeer RTT hello")
        if not self.arming_inhibited:
            failures.append("arming inhibit was not reported")
        if not self.imu_ready:
            failures.append("supported IMU did not become ready")
        if self.drdy_in_range < 2:
            failures.append("fewer than two approximately 1 kHz IMU DRDY intervals")
        if self.fatal_line is not None:
            failures.append(f"fatal firmware output: {self.fatal_line}")
        return failures


FIRMWARE_TARGETS = {
    "fcu3": FirmwareTarget(
        app_root=REPO_ROOT / "apps/stm32f405-flight",
        binary_name="FerroWasp",
        chip="STM32F405RG",
    ),
    "foxeer-f405-v2": FirmwareTarget(
        app_root=REPO_ROOT / "apps/foxeer-f405-v2",
        binary_name="FerroWaspFoxeerF405V2",
        chip="STM32F405RG",
    ),
}


@dataclass
class Logger:
    log_file: Path

    def __post_init__(self) -> None:
        self.log_file.parent.mkdir(parents=True, exist_ok=True)
        self._stream = self.log_file.open("a", encoding="utf-8", errors="replace")

    def close(self) -> None:
        self._stream.close()

    def line(self, text: str = "", *, stderr: bool = False) -> None:
        print(text, file=sys.stderr if stderr else sys.stdout, flush=True)
        self._stream.write(f"{text}\n")
        self._stream.flush()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build firmware, launch probe-rs run, read decoded defmt RTT output, and print "
            "firmware lines with a DRONE prefix."
        )
    )
    parser.add_argument(
        "--board",
        choices=tuple(FIRMWARE_TARGETS),
        default="fcu3",
        help="Firmware board/app to build and run over SWD. Defaults to fcu3.",
    )
    parser.add_argument(
        "--release",
        action="store_true",
        help="Build and run the release firmware image.",
    )
    parser.add_argument(
        "--locked",
        action="store_true",
        help="Pass --locked to the default cargo build.",
    )
    parser.add_argument(
        "--features",
        default="",
        metavar="FEATURES",
        help="Space- or comma-separated Cargo features for the default build.",
    )
    parser.add_argument(
        "--no-default-features",
        action="store_true",
        help="Pass --no-default-features to the default cargo build.",
    )
    parser.add_argument(
        "--foxeer-smoke",
        action="store_true",
        help=(
            "Run a 14-second arming-inhibited Foxeer SWD/RTT smoke test and "
            "validate boot, IMU identity, and data-ready progress."
        ),
    )
    parser.add_argument(
        "--probe-speed-khz",
        type=int,
        default=None,
        metavar="KHZ",
        help="Override the SWD/JTAG clock in kHz for the default probe-rs command.",
    )
    parser.add_argument(
        "--connect-under-reset",
        action="store_true",
        help="Assert NRST while attaching with the default probe-rs command.",
    )
    parser.add_argument(
        "--log-file",
        type=Path,
        default=None,
        help="Path to write the terminal log. Defaults to logs/terminal_embed/<timestamp>_rtt.log.",
    )
    parser.add_argument(
        "command",
        nargs=argparse.REMAINDER,
        help="Optional command to run after '--'. Defaults to build + probe-rs run.",
    )
    return parser.parse_args()


def default_log_file() -> Path:
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    return DEFAULT_LOG_DIR / f"{timestamp}_rtt.log"


def default_elf(target: FirmwareTarget, *, release: bool) -> Path:
    profile = "release" if release else "debug"
    return target.app_root / f"target/thumbv7em-none-eabihf/{profile}/{target.binary_name}"


def firmware_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest().upper()


def add_required_feature(features: str, required: str) -> str:
    enabled = [feature for feature in re.split(r"[\s,]+", features) if feature]
    if required not in enabled:
        enabled.append(required)
    return " ".join(enabled)


def commands_from_args(
    raw_command: Sequence[str],
    *,
    target: FirmwareTarget,
    release: bool,
    locked: bool,
    features: str,
    no_default_features: bool,
    probe_speed_khz: int | None,
    connect_under_reset: bool,
) -> tuple[list[str] | None, list[list[str]]]:
    if not raw_command:
        build_command = ["cargo", "build", "--quiet"]
        if release:
            build_command.append("--release")
        if locked:
            build_command.append("--locked")
        if no_default_features:
            build_command.append("--no-default-features")
        if features:
            build_command.extend(["--features", features])

        probe_command = [
            "probe-rs",
            "run",
            "--chip",
            target.chip,
            "--protocol",
            "swd",
        ]
        if probe_speed_khz is not None:
            probe_command.extend(["--speed", str(probe_speed_khz)])
        if connect_under_reset:
            probe_command.append("--connect-under-reset")
        probe_command.extend(
            [
                "--no-location",
                "--no-timestamps",
                str(default_elf(target, release=release)),
            ]
        )

        return (
            build_command,
            [probe_command],
        )
    if raw_command[0] == "--":
        return (None, [list(raw_command[1:])])
    return (None, [list(raw_command)])


def is_firmware_line(text: str) -> bool:
    clean = ANSI_ESCAPE_RE.sub("", text).strip()
    return DEFMT_LINE_RE.match(clean) is not None or any(
        marker in clean for marker in FIRMWARE_MARKERS
    )


def is_probe_run_command(command: Sequence[str]) -> bool:
    if len(command) < 2:
        return False
    executable = Path(command[0]).name.lower()
    return executable in {"probe-rs", "probe-rs.exe"} and command[1] == "run"


def is_probe_flash_activity_line(text: str) -> bool:
    clean = ANSI_ESCAPE_RE.sub("", text).lower()
    return "erasing" in clean or "programming" in clean


def is_probe_programming_complete_line(text: str) -> bool:
    clean = ANSI_ESCAPE_RE.sub("", text).strip()
    return re.search(r"\bFinished in \d", clean) is not None


def run_and_prefix(
    command: Sequence[str],
    logger: Logger,
    *,
    duration_seconds: float | None = None,
    foxeer_smoke: bool = False,
) -> int:
    if not command:
        logger.line("No command provided.", stderr=True)
        return 2

    logger.line(f"HOST: running {' '.join(command)}")
    probe_run = is_probe_run_command(command)
    if probe_run:
        logger.line("HOST: === FLASH STARTED: connecting, erasing, and programming ===")

    process: subprocess.Popen[str] | None = None
    smoke = FoxeerSmokeEvidence() if foxeer_smoke else None
    duration_expired = threading.Event()
    startup_expired = threading.Event()
    observation_timer: threading.Timer | None = None
    startup_timer: threading.Timer | None = None
    force_stop_timer: threading.Timer | None = None
    flash_activity_observed = False
    programming_completed = False
    firmware_boot_observed = False

    def force_stop() -> None:
        if process is not None and process.poll() is None:
            process.kill()

    def interrupt_process() -> None:
        nonlocal force_stop_timer
        if process is None or process.poll() is not None:
            return
        try:
            if os.name == "nt":
                process.send_signal(signal.CTRL_BREAK_EVENT)
            else:
                process.send_signal(signal.SIGINT)
        except OSError:
            process.terminate()
        force_stop_timer = threading.Timer(3.0, force_stop)
        force_stop_timer.daemon = True
        force_stop_timer.start()

    def stop_after_duration() -> None:
        duration_expired.set()
        interrupt_process()

    def stop_after_startup_timeout() -> None:
        startup_expired.set()
        interrupt_process()

    try:
        process = subprocess.Popen(
            command,
            cwd=REPO_ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
            creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0),
        )

        if foxeer_smoke:
            startup_timer = threading.Timer(90.0, stop_after_startup_timeout)
            startup_timer.daemon = True
            startup_timer.start()
        elif duration_seconds is not None:
            observation_timer = threading.Timer(duration_seconds, stop_after_duration)
            observation_timer.daemon = True
            observation_timer.start()

        assert process.stdout is not None
        for line in process.stdout:
            text = ANSI_ESCAPE_RE.sub("", line.rstrip())
            if text:
                firmware_line = is_firmware_line(text)
                prefix = "DRONE" if firmware_line else "HOST"
                logger.line(f"{prefix}: {text}")

                if probe_run and not flash_activity_observed and is_probe_flash_activity_line(text):
                    flash_activity_observed = True
                    logger.line("HOST: === FLASH ACTIVE: erase/program transfer observed ===")
                if (
                    probe_run
                    and not programming_completed
                    and is_probe_programming_complete_line(text)
                ):
                    programming_completed = True
                    logger.line("HOST: === FLASH PROGRAMMED: waiting for firmware boot/RTT ===")
                if probe_run and firmware_line and not firmware_boot_observed:
                    firmware_boot_observed = True
                    logger.line("HOST: === FLASH SUCCEEDED: firmware boot and RTT observed ===")

                if smoke is not None:
                    smoke.observe(text)
                    if smoke.init_ok and observation_timer is None:
                        if startup_timer is not None:
                            startup_timer.cancel()
                        if duration_seconds is not None:
                            logger.line(
                                f"HOST: firmware boot observed; starting {duration_seconds:.0f}-second RTT window"
                            )
                            observation_timer = threading.Timer(
                                duration_seconds, stop_after_duration
                            )
                            observation_timer.daemon = True
                            observation_timer.start()

        exit_code = process.wait()
        if startup_expired.is_set():
            if probe_run and not firmware_boot_observed:
                logger.line(
                    "HOST: === FLASH FAILED: firmware boot/RTT was not observed before timeout ===",
                    stderr=True,
                )
            failures = smoke.failures() if smoke is not None else []
            failures.append("firmware boot was not observed within the 90-second startup timeout")
            logger.line("HOST: Foxeer SWD/RTT smoke test FAILED", stderr=True)
            for failure in failures:
                logger.line(f"HOST:   - {failure}", stderr=True)
            return 1
        if not duration_expired.is_set():
            if smoke is not None:
                if probe_run and not firmware_boot_observed:
                    detail = (
                        "programming completed, but firmware boot/RTT was not observed"
                        if programming_completed
                        else "probe-rs exited before programming and firmware boot completed"
                    )
                    logger.line(
                        f"HOST: === FLASH FAILED: {detail} (code {exit_code}) ===",
                        stderr=True,
                    )
                failures = smoke.failures()
                failures.append(
                    f"probe-rs exited before the 14-second observation completed (code {exit_code})"
                )
                logger.line("HOST: Foxeer SWD/RTT smoke test FAILED", stderr=True)
                for failure in failures:
                    logger.line(f"HOST:   - {failure}", stderr=True)
                return 1
            if probe_run and not firmware_boot_observed:
                detail = (
                    "programming completed, but firmware boot/RTT was not observed"
                    if programming_completed
                    else "probe-rs exited before programming and firmware boot completed"
                )
                logger.line(
                    f"HOST: === FLASH FAILED: {detail} (code {exit_code}) ===",
                    stderr=True,
                )
                return exit_code if exit_code != 0 else 1
            return exit_code
        if smoke is None:
            return 0
        failures = smoke.failures()
        if failures:
            logger.line("HOST: Foxeer SWD/RTT smoke test FAILED", stderr=True)
            for failure in failures:
                logger.line(f"HOST:   - {failure}", stderr=True)
            return 1
        logger.line("HOST: Foxeer SWD/RTT smoke test PASSED")
        return 0

    except FileNotFoundError as error:
        if probe_run:
            logger.line(f"HOST: === FLASH FAILED: {error} ===", stderr=True)
        logger.line(f"Could not start RTT command: {error}", stderr=True)
        return 1
    except KeyboardInterrupt:
        logger.line("")
        logger.line("Stopping RTT reader.")
        if process is not None and process.poll() is None:
            interrupt_process()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
        return 0
    finally:
        if observation_timer is not None:
            observation_timer.cancel()
        if startup_timer is not None:
            startup_timer.cancel()
        if force_stop_timer is not None:
            force_stop_timer.cancel()


def run_quiet_build(command: Sequence[str], target: FirmwareTarget, logger: Logger) -> int:
    logger.line(f"HOST: building firmware with {' '.join(command)}")
    try:
        result = subprocess.run(
            command,
            cwd=target.app_root,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
    except FileNotFoundError as error:
        logger.line(f"Could not start build command: {error}", stderr=True)
        return 1

    if result.returncode == 0:
        logger.line("HOST: build finished")
        return 0

    logger.line("HOST: build failed; showing compiler output")
    for line in result.stdout.splitlines():
        logger.line(f"HOST: {line}")
    return result.returncode


def main() -> int:
    args = parse_args()
    if args.foxeer_smoke:
        if args.command:
            print("--foxeer-smoke cannot be combined with a custom command", file=sys.stderr)
            return 2
        args.board = "foxeer-f405-v2"
        args.release = True
        args.locked = True
        args.features = add_required_feature(args.features, "imu_transport_rtt")
        args.features = add_required_feature(args.features, "smoke_actuator_inhibit")
        if args.probe_speed_khz is None:
            args.probe_speed_khz = 1_800
    target = FIRMWARE_TARGETS[args.board]
    build_command, commands = commands_from_args(
        args.command,
        target=target,
        release=args.release,
        locked=args.locked,
        features=args.features,
        no_default_features=args.no_default_features,
        probe_speed_khz=args.probe_speed_khz,
        connect_under_reset=args.connect_under_reset,
    )
    logger = Logger(args.log_file or default_log_file())

    try:
        logger.line(f"HOST: logging to {logger.log_file}")
        if args.foxeer_smoke:
            logger.line("HOST: running 14-second arming-inhibited Foxeer SWD/RTT smoke test")
        else:
            logger.line("Press Ctrl+C to stop.")
        if build_command is not None:
            exit_code = run_quiet_build(build_command, target, logger)
            if exit_code != 0:
                return exit_code
            firmware = default_elf(target, release=args.release)
            try:
                logger.line(f"HOST: firmware ELF {firmware}")
                logger.line(f"HOST: firmware SHA-256 {firmware_sha256(firmware)}")
            except OSError as error:
                logger.line(f"Could not identify built firmware: {error}", stderr=True)
                return 1

        for command in commands:
            exit_code = run_and_prefix(
                command,
                logger,
                duration_seconds=14.0 if args.foxeer_smoke else None,
                foxeer_smoke=args.foxeer_smoke,
            )
            if exit_code != 0:
                return exit_code

        return 0
    finally:
        logger.close()


if __name__ == "__main__":
    raise SystemExit(main())
