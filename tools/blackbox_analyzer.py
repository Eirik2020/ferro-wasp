#!/usr/bin/env python3
"""Analyze FerroWasp 400 Hz BB1/BB2 blackbox RTT logs."""

from __future__ import annotations

import argparse
import csv
import math
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, pstdev
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parents[1]
LOG_DIRS = (
    REPO_ROOT / "logs" / "remote_probe",
    REPO_ROOT / "logs" / "terminal_embed",
)
CONTROL_RATE_HZ = 400.0

BB_RE = re.compile(
    r"BB(?P<version>[12]) seq (?P<seq>\d+) imu (?P<imu_seq>\d+) flags (?P<flags>\d+) "
    r"(?:raw10 \[(?P<raw_roll>-?\d+), (?P<raw_pitch>-?\d+), (?P<raw_yaw>-?\d+)\] )?"
    r"gyro10 \[(?P<gyro_roll>-?\d+), (?P<gyro_pitch>-?\d+), (?P<gyro_yaw>-?\d+)\] "
    r"cmd10 \[(?P<cmd_roll>-?\d+), (?P<cmd_pitch>-?\d+), (?P<cmd_yaw>-?\d+)\] "
    r"pid \[(?P<pid_roll>-?\d+), (?P<pid_pitch>-?\d+), (?P<pid_yaw>-?\d+)\] "
    r"thr (?P<throttle>\d+) "
    r"motors \[(?P<motor1>\d+), (?P<motor2>\d+), (?P<motor3>\d+), (?P<motor4>\d+)\]"
)


@dataclass(frozen=True)
class BlackboxSample:
    version: int
    seq: int
    imu_seq: int
    flags: int
    raw_dps: tuple[float, float, float] | None
    gyro_dps: tuple[float, float, float]
    command_dps: tuple[float, float, float]
    pid: tuple[int, int, int]
    throttle: int
    motors: tuple[int, int, int, int]

    @property
    def armed(self) -> bool:
        return bool(self.flags & 0x01)

    @property
    def imu_fresh(self) -> bool:
        return bool(self.flags & 0x02)


@dataclass(frozen=True)
class AxisStats:
    raw_mean: float | None
    raw_std: float | None
    raw_peak_to_peak: float | None
    residual_std: float | None
    residual_peak_to_peak: float | None
    lag_samples: int | None
    correlation: float | None
    filtered_mean: float
    filtered_std: float
    filtered_peak_to_peak: float
    attenuation: float | None
    drift: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Analyze FerroWasp BB1/BB2 blackbox logs for gyro filtering and control chatter."
    )
    parser.add_argument(
        "log_files",
        nargs="*",
        type=Path,
        help="RTT log file(s). Defaults to newest .log in logs/remote_probe or logs/terminal_embed.",
    )
    parser.add_argument(
        "--csv",
        type=Path,
        help="Write parsed blackbox samples to CSV.",
    )
    parser.add_argument(
        "--warmup",
        type=int,
        default=3,
        help="Discard this many initial blackbox samples.",
    )
    parser.add_argument(
        "--trim-start",
        type=float,
        default=0.0,
        metavar="SECONDS",
        help="Discard this many seconds from the start of the parsed blackbox log.",
    )
    parser.add_argument(
        "--trim-end",
        type=float,
        default=0.0,
        metavar="SECONDS",
        help="Discard this many seconds from the end of the parsed blackbox log.",
    )
    parser.add_argument(
        "--mode",
        choices=("auto", "rest", "swing"),
        default="auto",
        help="Interpretation mode. 'rest' treats large gyro std as noise; 'swing' treats low-frequency motion as intended movement.",
    )
    parser.add_argument(
        "--quiet-threshold-dps",
        type=float,
        default=1.0,
        help="Filtered gyro standard deviation at or below this is treated as quiet.",
    )
    parser.add_argument(
        "--noisy-threshold-dps",
        type=float,
        default=3.0,
        help="Filtered gyro standard deviation at or above this is treated as noisy.",
    )
    return parser.parse_args()


def parse_blackbox_sample(text: str) -> BlackboxSample | None:
    match = BB_RE.search(text)
    if match is None:
        return None

    raw_dps = None
    if match.group("raw_roll") is not None:
        raw_dps = (
            int(match.group("raw_roll")) / 10.0,
            int(match.group("raw_pitch")) / 10.0,
            int(match.group("raw_yaw")) / 10.0,
        )

    return BlackboxSample(
        version=int(match.group("version")),
        seq=int(match.group("seq")),
        imu_seq=int(match.group("imu_seq")),
        flags=int(match.group("flags")),
        raw_dps=raw_dps,
        gyro_dps=(
            int(match.group("gyro_roll")) / 10.0,
            int(match.group("gyro_pitch")) / 10.0,
            int(match.group("gyro_yaw")) / 10.0,
        ),
        command_dps=(
            int(match.group("cmd_roll")) / 10.0,
            int(match.group("cmd_pitch")) / 10.0,
            int(match.group("cmd_yaw")) / 10.0,
        ),
        pid=(
            int(match.group("pid_roll")),
            int(match.group("pid_pitch")),
            int(match.group("pid_yaw")),
        ),
        throttle=int(match.group("throttle")),
        motors=(
            int(match.group("motor1")),
            int(match.group("motor2")),
            int(match.group("motor3")),
            int(match.group("motor4")),
        ),
    )


def latest_log_file() -> Path:
    candidates: list[Path] = []
    for log_dir in LOG_DIRS:
        if log_dir.exists():
            candidates.extend(log_dir.glob("*.log"))
    if not candidates:
        searched = ", ".join(str(path) for path in LOG_DIRS)
        raise FileNotFoundError(f"No RTT logs found in {searched}")
    return max(candidates, key=lambda path: path.stat().st_mtime)


def samples_from_lines(lines: Iterable[str]) -> list[BlackboxSample]:
    samples: list[BlackboxSample] = []
    for line in lines:
        sample = parse_blackbox_sample(line)
        if sample is not None:
            samples.append(sample)
    return samples


def samples_from_file(path: Path) -> list[BlackboxSample]:
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        return samples_from_lines(handle)


def trim_samples_by_seconds(
    samples: list[BlackboxSample],
    trim_start_s: float,
    trim_end_s: float,
) -> tuple[list[BlackboxSample], int, int]:
    start_count = max(0, int(round(trim_start_s * CONTROL_RATE_HZ)))
    end_count = max(0, int(round(trim_end_s * CONTROL_RATE_HZ)))
    if start_count + end_count >= len(samples):
        raise ValueError(
            f"trim would remove all samples: start={start_count}, end={end_count}, available={len(samples)}"
        )
    end_index = len(samples) - end_count if end_count else len(samples)
    return samples[start_count:end_index], start_count, end_count


def axis_stats(raw_values: list[float] | None, filtered_values: list[float]) -> AxisStats:
    filtered_std = pstdev(filtered_values)
    raw_mean = None
    raw_std = None
    raw_peak_to_peak = None
    residual_std = None
    residual_peak_to_peak = None
    lag_samples = None
    correlation = None
    attenuation = None
    if raw_values:
        raw_mean = mean(raw_values)
        raw_std = pstdev(raw_values)
        raw_peak_to_peak = max(raw_values) - min(raw_values)
        residual = [filtered - raw for filtered, raw in zip(filtered_values, raw_values)]
        residual_std = pstdev(residual)
        residual_peak_to_peak = max(residual) - min(residual)
        lag_samples, correlation = best_lag_correlation(raw_values, filtered_values)
        if raw_std > 0.001:
            attenuation = filtered_std / raw_std
    return AxisStats(
        raw_mean=raw_mean,
        raw_std=raw_std,
        raw_peak_to_peak=raw_peak_to_peak,
        residual_std=residual_std,
        residual_peak_to_peak=residual_peak_to_peak,
        lag_samples=lag_samples,
        correlation=correlation,
        filtered_mean=mean(filtered_values),
        filtered_std=filtered_std,
        filtered_peak_to_peak=max(filtered_values) - min(filtered_values),
        attenuation=attenuation,
        drift=filtered_values[-1] - filtered_values[0],
    )


def best_lag_correlation(
    raw_values: list[float],
    filtered_values: list[float],
    max_lag_samples: int = 20,
) -> tuple[int, float] | tuple[None, None]:
    if len(raw_values) < max_lag_samples * 2 + 1:
        return None, None

    best_lag = 0
    best_corr = -2.0
    for lag in range(-max_lag_samples, max_lag_samples + 1):
        if lag < 0:
            raw = raw_values[-lag:]
            filtered = filtered_values[: len(filtered_values) + lag]
        elif lag > 0:
            raw = raw_values[:-lag]
            filtered = filtered_values[lag:]
        else:
            raw = raw_values
            filtered = filtered_values

        raw_mean = mean(raw)
        filtered_mean = mean(filtered)
        raw_energy = sum((value - raw_mean) ** 2 for value in raw)
        filtered_energy = sum((value - filtered_mean) ** 2 for value in filtered)
        denominator = math.sqrt(raw_energy * filtered_energy)
        if denominator <= 0.0:
            continue
        correlation = sum(
            (raw_value - raw_mean) * (filtered_value - filtered_mean)
            for raw_value, filtered_value in zip(raw, filtered)
        ) / denominator
        if correlation > best_corr:
            best_lag = lag
            best_corr = correlation

    if best_corr < -1.0:
        return None, None
    return best_lag, best_corr


def strongest_frequency(values: list[float], sample_rate_hz: float) -> tuple[float, float] | None:
    count = len(values)
    if count < 16:
        return None
    center = mean(values)
    centered = [value - center for value in values]
    max_bin = min(count // 2, 220)
    best_frequency = 0.0
    best_magnitude = 0.0
    for bin_index in range(1, max_bin):
        real = 0.0
        imag = 0.0
        for sample_index, value in enumerate(centered):
            phase = 2.0 * math.pi * bin_index * sample_index / count
            real += value * math.cos(phase)
            imag -= value * math.sin(phase)
        magnitude = math.sqrt(real * real + imag * imag) / count
        if magnitude > best_magnitude:
            best_magnitude = magnitude
            best_frequency = bin_index * sample_rate_hz / count
    return best_frequency, best_magnitude


def verdict(std_dps: float, quiet_threshold: float, noisy_threshold: float) -> str:
    if std_dps <= quiet_threshold:
        return "quiet"
    if std_dps >= noisy_threshold:
        return "noisy"
    return "watch"


def interpreted_verdict(
    base: str,
    peak_hz: float | None,
    mode: str,
) -> str:
    if mode == "rest":
        return base
    if base == "noisy" and peak_hz is not None and peak_hz < 10.0:
        return "motion"
    if mode == "swing" and base == "watch":
        return "motion"
    return base


def print_axis_report(
    samples: list[BlackboxSample],
    sample_rate_hz: float,
    quiet_threshold: float,
    noisy_threshold: float,
    mode: str,
) -> int:
    raw_available = any(sample.raw_dps is not None for sample in samples)
    raw_axes: list[list[float] | None] = []
    for axis in range(3):
        if raw_available:
            raw_axes.append([sample.raw_dps[axis] for sample in samples if sample.raw_dps is not None])
        else:
            raw_axes.append(None)
    filtered_axes = [[sample.gyro_dps[axis] for sample in samples] for axis in range(3)]

    print(
        "axis   raw_mean  filt_mean  raw_std  filt_std  res_std  drift  attenuation  lag_ms    corr  peak_hz  verdict"
    )
    print(
        "----   --------  ---------  -------  --------  -------  -----  -----------  ------  -----  -------  -------"
    )
    noisy_axes = 0
    for axis, label in enumerate(("roll", "pitch", "yaw")):
        stats = axis_stats(raw_axes[axis], filtered_axes[axis])
        base_status = verdict(stats.filtered_std, quiet_threshold, noisy_threshold)
        peak = strongest_frequency(filtered_axes[axis], sample_rate_hz)
        peak_value = None if peak is None else peak[0]
        status = interpreted_verdict(base_status, peak_value, mode)
        noisy_axes += 1 if status == "noisy" else 0
        raw_mean = "n/a" if stats.raw_mean is None else f"{stats.raw_mean:8.2f}"
        raw_std = "n/a" if stats.raw_std is None else f"{stats.raw_std:7.2f}"
        residual_std = "n/a" if stats.residual_std is None else f"{stats.residual_std:7.2f}"
        attenuation = "n/a"
        if stats.attenuation is not None:
            attenuation = f"{stats.attenuation * 100.0:5.1f}%"
        peak_hz = "n/a" if peak is None else f"{peak[0]:7.1f}"
        lag_ms = "n/a"
        if stats.lag_samples is not None:
            lag_ms = f"{stats.lag_samples * 1000.0 / sample_rate_hz:6.1f}"
        corr = "n/a" if stats.correlation is None else f"{stats.correlation:5.3f}"
        print(
            f"{label:<5} "
            f"{raw_mean:>8} "
            f"{stats.filtered_mean:9.2f} "
            f"{raw_std:>7} "
            f"{stats.filtered_std:8.2f} "
            f"{residual_std:>7} "
            f"{stats.drift:5.2f} "
            f"{attenuation:>11} "
            f"{lag_ms:>6} "
            f"{corr:>5} "
            f"{peak_hz:>7} "
            f"{status}"
        )
    return noisy_axes


def print_control_report(samples: list[BlackboxSample]) -> None:
    command_axes = [[sample.command_dps[axis] for sample in samples] for axis in range(3)]
    pid_axes = [[float(sample.pid[axis]) for sample in samples] for axis in range(3)]
    motor_axes = [[float(sample.motors[motor]) for sample in samples] for motor in range(4)]
    print()
    print("Command inputs:")
    for axis, label in enumerate(("roll", "pitch", "yaw")):
        values = command_axes[axis]
        drift = values[-1] - values[0]
        print(
            f"  cmd {label:<5} mean={mean(values):7.2f} std={pstdev(values):7.2f} "
            f"p2p={max(values) - min(values):7.2f} drift={drift:7.2f} dps"
        )
    print()
    print("Control chatter:")
    for axis, label in enumerate(("roll", "pitch", "yaw")):
        values = pid_axes[axis]
        print(f"  pid {label:<5} std={pstdev(values):7.2f} p2p={max(values) - min(values):7.2f}")
    for motor, values in enumerate(motor_axes, start=1):
        print(f"  motor {motor} std={pstdev(values):7.2f} p2p={max(values) - min(values):7.2f}")


def write_csv(path: Path, samples: list[BlackboxSample]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(
            [
                "seq",
                "imu_seq",
                "flags",
                "armed",
                "imu_fresh",
                "raw_roll_dps",
                "raw_pitch_dps",
                "raw_yaw_dps",
                "gyro_roll_dps",
                "gyro_pitch_dps",
                "gyro_yaw_dps",
                "cmd_roll_dps",
                "cmd_pitch_dps",
                "cmd_yaw_dps",
                "pid_roll",
                "pid_pitch",
                "pid_yaw",
                "throttle",
                "motor1",
                "motor2",
                "motor3",
                "motor4",
            ]
        )
        for sample in samples:
            raw = sample.raw_dps or ("", "", "")
            writer.writerow(
                [
                    sample.seq,
                    sample.imu_seq,
                    sample.flags,
                    int(sample.armed),
                    int(sample.imu_fresh),
                    raw[0],
                    raw[1],
                    raw[2],
                    sample.gyro_dps[0],
                    sample.gyro_dps[1],
                    sample.gyro_dps[2],
                    sample.command_dps[0],
                    sample.command_dps[1],
                    sample.command_dps[2],
                    sample.pid[0],
                    sample.pid[1],
                    sample.pid[2],
                    sample.throttle,
                    sample.motors[0],
                    sample.motors[1],
                    sample.motors[2],
                    sample.motors[3],
                ]
            )


def main() -> int:
    args = parse_args()
    samples: list[BlackboxSample] = []
    sources: list[str] = []

    try:
        paths = args.log_files or [latest_log_file()]
        for path in paths:
            resolved = path if path.is_absolute() else REPO_ROOT / path
            samples.extend(samples_from_file(resolved))
            sources.append(str(resolved))
    except (FileNotFoundError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    if args.warmup > 0:
        samples = samples[args.warmup :]

    try:
        samples, trimmed_start, trimmed_end = trim_samples_by_seconds(
            samples,
            args.trim_start,
            args.trim_end,
        )
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    if len(samples) < 5:
        print("error: fewer than 5 BB1/BB2 samples found. Capture a longer blackbox log.", file=sys.stderr)
        return 1

    first_seq = samples[0].seq
    last_seq = samples[-1].seq
    seq_span = max(1, last_seq - first_seq)
    duration_s = seq_span / CONTROL_RATE_HZ
    sample_rate_hz = (len(samples) - 1) / duration_s if len(samples) > 1 else CONTROL_RATE_HZ

    print()
    print(f"Source: {', '.join(sources)}")
    print(f"Format: BB{max(sample.version for sample in samples)} blackbox")
    print(f"Samples: {len(samples)}  seq: {first_seq}..{last_seq}  estimated rate: {sample_rate_hz:.1f} Hz")
    if trimmed_start or trimmed_end:
        print(f"Trimmed samples: start={trimmed_start}, end={trimmed_end}")
    print(f"Armed samples: {sum(1 for sample in samples if sample.armed)}")
    print(f"Fresh IMU samples: {sum(1 for sample in samples if sample.imu_fresh)}")
    if not any(sample.raw_dps is not None for sample in samples):
        print("Raw gyro unavailable. Reflash BB2 firmware for raw-vs-filtered analysis.")
    print()

    noisy_axes = print_axis_report(
        samples,
        sample_rate_hz,
        args.quiet_threshold_dps,
        args.noisy_threshold_dps,
        args.mode,
    )
    print_control_report(samples)

    print()
    if noisy_axes:
        print("Bench read: filtered gyro has high-frequency/noise-like energy on at least one axis.")
    else:
        print("Bench read: no axis was classified as high-frequency/noise-like in this mode.")
    print("For swing tests, prefer --mode swing or --mode auto and focus on residual_std, lag_ms, corr, and peak_hz.")

    if args.csv is not None:
        csv_path = args.csv if args.csv.is_absolute() else REPO_ROOT / args.csv
        write_csv(csv_path, samples)
        print(f"CSV written: {csv_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
