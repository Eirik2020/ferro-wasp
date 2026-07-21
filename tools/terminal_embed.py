#!/usr/bin/env python3
"""Build, flash/run with probe-rs, and print decoded defmt RTT lines."""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
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


def commands_from_args(
    raw_command: Sequence[str],
    *,
    target: FirmwareTarget,
    release: bool,
    locked: bool,
    features: str,
) -> tuple[list[str] | None, list[list[str]]]:
    if not raw_command:
        build_command = ["cargo", "build", "--quiet"]
        if release:
            build_command.append("--release")
        if locked:
            build_command.append("--locked")
        if features:
            build_command.extend(["--features", features])

        return (
            build_command,
            [
                [
                    "probe-rs",
                    "run",
                    "--chip",
                    target.chip,
                    "--protocol",
                    "swd",
                    "--no-location",
                    "--no-timestamps",
                    str(default_elf(target, release=release)),
                ]
            ],
        )
    if raw_command[0] == "--":
        return (None, [list(raw_command[1:])])
    return (None, [list(raw_command)])


def is_firmware_line(text: str) -> bool:
    clean = ANSI_ESCAPE_RE.sub("", text).strip()
    return DEFMT_LINE_RE.match(clean) is not None or any(
        marker in clean for marker in FIRMWARE_MARKERS
    )


def run_and_prefix(command: Sequence[str], logger: Logger) -> int:
    if not command:
        logger.line("No command provided.", stderr=True)
        return 2

    logger.line(f"HOST: running {' '.join(command)}")

    process: subprocess.Popen[str] | None = None
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
        )

        assert process.stdout is not None
        for line in process.stdout:
            text = ANSI_ESCAPE_RE.sub("", line.rstrip())
            if text:
                prefix = "DRONE" if is_firmware_line(text) else "HOST"
                logger.line(f"{prefix}: {text}")

        return process.wait()

    except FileNotFoundError as error:
        logger.line(f"Could not start RTT command: {error}", stderr=True)
        return 1
    except KeyboardInterrupt:
        logger.line("")
        logger.line("Stopping RTT reader.")
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
        return 0


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
    target = FIRMWARE_TARGETS[args.board]
    build_command, commands = commands_from_args(
        args.command,
        target=target,
        release=args.release,
        locked=args.locked,
        features=args.features,
    )
    logger = Logger(args.log_file or default_log_file())

    try:
        logger.line(f"HOST: logging to {logger.log_file}")
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
            exit_code = run_and_prefix(command, logger)
            if exit_code != 0:
                return exit_code

        return 0
    finally:
        logger.close()


if __name__ == "__main__":
    raise SystemExit(main())
