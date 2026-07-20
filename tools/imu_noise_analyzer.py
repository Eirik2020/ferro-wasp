#!/usr/bin/env python3
"""Analyze IMU heartbeat logs for gyro noise before PID."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, pstdev
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parents[1]
LOG_DIR = REPO_ROOT / "logs" / "terminal_embed"
TERMINAL_EMBED = REPO_ROOT / "tools" / "terminal_embed.py"
RAW_TO_DPS = 16.4

IMU_RE = re.compile(
    r"Heartbeat: IMU(?: data stale,)? seq (?P<seq>\d+), "
    r"raw \[(?P<raw_x>-?\d+), (?P<raw_y>-?\d+), (?P<raw_z>-?\d+)\], "
    r"acc_mg \[(?P<acc_x>-?\d+), (?P<acc_y>-?\d+), (?P<acc_z>-?\d+)\], "
    r"rates \[(?P<rate_roll>-?\d+), (?P<rate_pitch>-?\d+), (?P<rate_yaw>-?\d+)\]"
)


@dataclass(frozen=True)
class ImuSample:
    seq: int
    raw_control_dps: tuple[float, float, float]
    rates_dps: tuple[float, float, float]


@dataclass(frozen=True)
class AxisStats:
    count: int
    raw_mean: float
    raw_std: float
    raw_peak_to_peak: float
    rate_mean: float
    rate_std: float
    rate_peak_to_peak: float
    attenuation: float | None
    drift: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Analyze FerroWasp RTT heartbeat logs for raw gyro noise and "
            "filtered PID measured-rate noise."
        )
    )
    parser.add_argument(
        "log_files",
        nargs="*",
        type=Path,
        help="RTT log file(s) to analyze. Defaults to --latest when omitted.",
    )
    parser.add_argument(
        "--latest",
        action="store_true",
        help="Analyze the newest log in logs/terminal_embed.",
    )
    parser.add_argument(
        "--capture",
        type=float,
        metavar="SECONDS",
        help="Run terminal_embed.py for this many seconds and analyze the captured output.",
    )
    parser.add_argument(
        "--warmup",
        type=int,
        default=3,
        help="Discard this many initial parsed IMU samples before calculating stats.",
    )
    parser.add_argument(
        "--quiet-threshold-dps",
        type=float,
        default=1.0,
        help="Filtered rate standard deviation at or below this is treated as quiet.",
    )
    parser.add_argument(
        "--noisy-threshold-dps",
        type=float,
        default=3.0,
        help="Filtered rate standard deviation at or above this is treated as noisy.",
    )
    return parser.parse_args()


def parse_imu_sample(text: str) -> ImuSample | None:
    match = IMU_RE.search(text)
    if match is None:
        return None

    raw_x = int(match.group("raw_x"))
    raw_y = int(match.group("raw_y"))
    raw_z = int(match.group("raw_z"))

    return ImuSample(
        seq=int(match.group("seq")),
        raw_control_dps=(
            raw_y / RAW_TO_DPS,
            raw_x / RAW_TO_DPS,
            -raw_z / RAW_TO_DPS,
        ),
        rates_dps=(
            float(match.group("rate_roll")),
            float(match.group("rate_pitch")),
            float(match.group("rate_yaw")),
        ),
    )


def latest_log_file() -> Path:
    candidates = sorted(LOG_DIR.glob("*_rtt.log"), key=lambda path: path.stat().st_mtime)
    if not candidates:
        raise FileNotFoundError(f"No RTT logs found in {LOG_DIR}")
    return candidates[-1]


def samples_from_lines(lines: Iterable[str]) -> list[ImuSample]:
    samples: list[ImuSample] = []
    for line in lines:
        sample = parse_imu_sample(line)
        if sample is not None:
            samples.append(sample)
    return samples


def samples_from_file(path: Path) -> list[ImuSample]:
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        return samples_from_lines(handle)


def capture_samples(duration_s: float) -> list[ImuSample]:
    command = [sys.executable, str(TERMINAL_EMBED)]
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

    deadline = time.monotonic() + max(1.0, duration_s)
    samples: list[ImuSample] = []
    try:
        while time.monotonic() < deadline:
            line = process.stdout.readline()
            if not line:
                if process.poll() is not None:
                    break
                time.sleep(0.05)
                continue
            text = line.rstrip()
            if text:
                print(text, flush=True)
            sample = parse_imu_sample(text)
            if sample is not None:
                samples.append(sample)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
    return samples


def axis_stats(samples: list[ImuSample], axis: int) -> AxisStats:
    raw = [sample.raw_control_dps[axis] for sample in samples]
    rates = [sample.rates_dps[axis] for sample in samples]
    raw_std = pstdev(raw)
    rate_std = pstdev(rates)
    attenuation = None
    if raw_std > 0.001:
        attenuation = rate_std / raw_std
    return AxisStats(
        count=len(samples),
        raw_mean=mean(raw),
        raw_std=raw_std,
        raw_peak_to_peak=max(raw) - min(raw),
        rate_mean=mean(rates),
        rate_std=rate_std,
        rate_peak_to_peak=max(rates) - min(rates),
        attenuation=attenuation,
        drift=rates[-1] - rates[0],
    )


def verdict(stats: AxisStats, quiet_threshold: float, noisy_threshold: float) -> str:
    if stats.rate_std <= quiet_threshold:
        return "quiet"
    if stats.rate_std >= noisy_threshold:
        return "noisy"
    return "watch"


def print_report(
    samples: list[ImuSample],
    source: str,
    quiet_threshold: float,
    noisy_threshold: float,
) -> None:
    first_seq = samples[0].seq
    last_seq = samples[-1].seq
    print()
    print(f"Source: {source}")
    print(f"Samples: {len(samples)}  seq: {first_seq}..{last_seq}")
    print("Raw gyro is remapped into PID roll/pitch/yaw before comparison.")
    print()
    print(
        "axis   raw_std  raw_p2p  rate_mean  rate_std  rate_p2p  attenuation  drift  verdict"
    )
    print(
        "----   -------  -------  ---------  --------  --------  -----------  -----  -------"
    )
    labels = ("roll", "pitch", "yaw")
    noisy_axes = 0
    for axis, label in enumerate(labels):
        stats = axis_stats(samples, axis)
        status = verdict(stats, quiet_threshold, noisy_threshold)
        if status == "noisy":
            noisy_axes += 1
        attenuation = "n/a"
        if stats.attenuation is not None:
            attenuation = f"{stats.attenuation * 100.0:5.1f}%"
        print(
            f"{label:<5} "
            f"{stats.raw_std:7.2f} "
            f"{stats.raw_peak_to_peak:7.2f} "
            f"{stats.rate_mean:9.2f} "
            f"{stats.rate_std:8.2f} "
            f"{stats.rate_peak_to_peak:8.2f} "
            f"{attenuation:>11} "
            f"{stats.drift:6.2f} "
            f"{status}"
        )

    print()
    if noisy_axes:
        print(
            "Bench read: filtered PID rates are still noisy on at least one axis. "
            "Consider more gyro filtering or lower D gain before motor tests."
        )
    else:
        print(
            "Bench read: filtered PID rates look usable at rest. Next checks are "
            "props-off motor output noise and motor temperature."
        )
    print(
        "Note: heartbeat logs are sparse, so this is a rest-noise screen, not a "
        "full frequency-response or vibration analysis."
    )


def main() -> int:
    args = parse_args()

    try:
        if args.capture is not None:
            samples = capture_samples(args.capture)
            source = f"live capture for {args.capture:.1f}s"
        else:
            paths = args.log_files
            if args.latest or not paths:
                paths = [latest_log_file()]
            samples = []
            source_parts: list[str] = []
            for path in paths:
                resolved = path if path.is_absolute() else REPO_ROOT / path
                samples.extend(samples_from_file(resolved))
                source_parts.append(str(resolved))
            source = ", ".join(source_parts)
    except (FileNotFoundError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    if args.warmup > 0:
        samples = samples[args.warmup :]

    if len(samples) < 5:
        print(
            "error: fewer than 5 IMU heartbeat samples found. "
            "Capture a longer still log.",
            file=sys.stderr,
        )
        return 1

    print_report(samples, source, args.quiet_threshold_dps, args.noisy_threshold_dps)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
