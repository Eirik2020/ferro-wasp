#!/usr/bin/env python3
"""Analyze FerroWasp 400 Hz BB1/BB2 blackbox RTT logs."""

from __future__ import annotations

import argparse
import binascii
from collections import Counter
import csv
import math
import re
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, median, pstdev
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parents[1]
LOG_DIRS = (
    REPO_ROOT / "logs" / "remote_probe",
    REPO_ROOT / "logs" / "terminal_embed",
)
CONTROL_RATE_HZ = 400.0
FLASH_PAGE_LEN = 256
FLASH_RECORD_LEN = 48
FLASH_PAGE_DATA_LEN = 240
FLASH_RECORD_STRUCT = struct.Struct("<IIIH3h3h3h3hH4H")

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
    timestamp_us: int | None = None
    flight_id: int | None = None

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


@dataclass(frozen=True)
class SequenceStats:
    valid_pairs: int
    contiguous_control_pairs: int
    missing_blackbox_frames: int
    repeated_imu_samples: int
    imu_samples_per_control_tick: float | None
    estimated_imu_rate_hz: float | None
    contiguous_imu_delta_counts: tuple[tuple[int, int], ...]


@dataclass(frozen=True)
class FlashLogStats:
    page_count: int
    record_count: int
    flight_ids: tuple[int, ...]
    first_page_sequence: int | None
    last_page_sequence: int | None
    partial_page_count: int
    final_page_records: int | None


@dataclass(frozen=True)
class TimestampStats:
    contiguous_pairs: int
    mean_interval_us: float
    std_interval_us: float
    min_interval_us: int
    max_interval_us: int
    p99_abs_jitter_us: int
    intervals_over_3000_us: int


@dataclass(frozen=True)
class RateBiasAxisStats:
    mean_dps: float
    median_dps: float
    std_dps: float
    mean_abs_dps: float
    signed_rotation_degrees: float


@dataclass(frozen=True)
class CenteredDriftStats:
    sample_count: int
    observed_duration_s: float
    integrated_duration_s: float
    axes: tuple[RateBiasAxisStats, RateBiasAxisStats, RateBiasAxisStats]


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
        "--flight-id",
        help="Analyze one onboard-flash flight ID, or 'latest'.",
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
        "--flight-window-throttle-min",
        type=int,
        metavar="VALUE",
        help=(
            "Analyze only the longest contiguous armed/fresh-IMU interval at or "
            "above this throttle. Useful for excluding takeoff, motor cut, and ground impacts."
        ),
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
    parser.add_argument(
        "--drift-report",
        action="store_true",
        help=(
            "Report signed mean gyro rate while armed, throttle-on, and all rate "
            "commands are centered. This is the useful drift metric for assessing I-term need."
        ),
    )
    parser.add_argument(
        "--drift-throttle-min",
        type=int,
        default=500,
        metavar="VALUE",
        help="Minimum logged throttle included by --drift-report (default: 500).",
    )
    parser.add_argument(
        "--drift-command-threshold-dps",
        type=float,
        default=0.5,
        metavar="DPS",
        help=(
            "Maximum absolute command on every axis included by --drift-report "
            "(default: 0.5 dps)."
        ),
    )
    parser.add_argument(
        "--drift-settle-seconds",
        type=float,
        default=0.5,
        metavar="SECONDS",
        help=(
            "Require commands and throttle to satisfy the drift filter continuously "
            "for this long before including samples (default: 0.5 s)."
        ),
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
    if path.suffix.lower() == ".fwbb":
        return samples_from_flash_file(path)
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        return samples_from_lines(handle)


def samples_from_flash_file(path: Path) -> list[BlackboxSample]:
    return samples_from_flash_bytes(path.read_bytes())


def validated_flash_pages(data: bytes) -> list[tuple[bytes, int, int, int]]:
    if len(data) % FLASH_PAGE_LEN:
        page_index = len(data) // FLASH_PAGE_LEN
        raise OSError(
            f"truncated flash page {page_index}: {len(data) % FLASH_PAGE_LEN} bytes"
        )
    pages: list[tuple[bytes, int, int, int]] = []
    for page_start in range(0, len(data), FLASH_PAGE_LEN):
        page_index = page_start // FLASH_PAGE_LEN
        page = data[page_start : page_start + FLASH_PAGE_LEN]
        if page[FLASH_PAGE_DATA_LEN : FLASH_PAGE_DATA_LEN + 2] != b"FB":
            raise OSError(f"invalid flash page magic at page {page_index}")
        if page[FLASH_PAGE_DATA_LEN + 2] != 1:
            raise OSError(f"unsupported flash page version at page {page_index}")
        record_count = page[FLASH_PAGE_DATA_LEN + 3]
        if record_count > FLASH_PAGE_DATA_LEN // FLASH_RECORD_LEN:
            raise OSError(
                f"invalid flash record count {record_count} at page {page_index}"
            )
        expected_crc = int.from_bytes(page[-4:], "little")
        if binascii.crc32(page[:-4]) != expected_crc:
            raise OSError(f"flash page CRC mismatch at page {page_index}")
        flight_id = int.from_bytes(
            page[FLASH_PAGE_DATA_LEN + 4 : FLASH_PAGE_DATA_LEN + 8], "little"
        )
        page_sequence = int.from_bytes(
            page[FLASH_PAGE_DATA_LEN + 8 : FLASH_PAGE_DATA_LEN + 12], "little"
        )
        pages.append((page, flight_id, page_sequence, record_count))
    return pages


def flash_log_stats(data: bytes) -> FlashLogStats:
    pages = validated_flash_pages(data)
    return FlashLogStats(
        page_count=len(pages),
        record_count=sum(page[3] for page in pages),
        flight_ids=tuple(sorted({page[1] for page in pages})),
        first_page_sequence=pages[0][2] if pages else None,
        last_page_sequence=pages[-1][2] if pages else None,
        partial_page_count=sum(
            page[3] < FLASH_PAGE_DATA_LEN // FLASH_RECORD_LEN for page in pages
        ),
        final_page_records=pages[-1][3] if pages else None,
    )


def samples_from_flash_bytes(data: bytes) -> list[BlackboxSample]:
    samples: list[BlackboxSample] = []
    for page, flight_id, _, record_count in validated_flash_pages(data):
        for record_index in range(record_count):
            start = record_index * FLASH_RECORD_LEN
            values = FLASH_RECORD_STRUCT.unpack_from(page, start)
            samples.append(
                BlackboxSample(
                    version=2,
                    seq=values[1],
                    imu_seq=values[2],
                    flags=values[3],
                    raw_dps=tuple(value / 10.0 for value in values[4:7]),
                    gyro_dps=tuple(value / 10.0 for value in values[7:10]),
                    command_dps=tuple(value / 10.0 for value in values[10:13]),
                    pid=tuple(values[13:16]),
                    throttle=values[16],
                    motors=tuple(values[17:21]),
                    timestamp_us=values[0],
                    flight_id=flight_id,
                )
            )
    return samples


def select_flight_samples(
    samples: list[BlackboxSample], requested: str | None
) -> tuple[list[BlackboxSample], int | None]:
    if requested is None:
        return samples, None

    available = sorted(
        {sample.flight_id for sample in samples if sample.flight_id is not None}
    )
    if not available:
        raise ValueError("--flight-id requires an onboard .fwbb log")

    if requested.lower() == "latest":
        selected = available[-1]
    else:
        try:
            selected = int(requested, 10)
        except ValueError as error:
            raise ValueError("--flight-id must be an integer or 'latest'") from error
        if selected not in available:
            choices = ",".join(str(value) for value in available)
            raise ValueError(
                f"flight ID {selected} is unavailable; available IDs: {choices}"
            )

    return [sample for sample in samples if sample.flight_id == selected], selected


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


def longest_flight_window(
    samples: list[BlackboxSample], throttle_min: int
) -> list[BlackboxSample]:
    """Return the longest contiguous armed, fresh-IMU, throttle-on run."""

    best_start = 0
    best_end = 0
    run_start: int | None = None
    previous: BlackboxSample | None = None

    for index, sample in enumerate(samples):
        contiguous = (
            previous is not None
            and previous.flight_id == sample.flight_id
            and u32_forward_delta(previous.seq, sample.seq) == 1
        )
        eligible = (
            sample.armed and sample.imu_fresh and sample.throttle >= throttle_min
        )

        if eligible:
            if run_start is None or not contiguous:
                run_start = index
        elif run_start is not None:
            if index - run_start > best_end - best_start:
                best_start, best_end = run_start, index
            run_start = None

        previous = sample

    if run_start is not None and len(samples) - run_start > best_end - best_start:
        best_start, best_end = run_start, len(samples)

    return samples[best_start:best_end]


def timestamp_stats(samples: list[BlackboxSample]) -> TimestampStats | None:
    intervals: list[int] = []
    for previous, current in zip(samples, samples[1:]):
        if previous.timestamp_us is None or current.timestamp_us is None:
            continue
        if current.seq - previous.seq != 1:
            continue
        intervals.append((current.timestamp_us - previous.timestamp_us) & 0xFFFF_FFFF)

    if not intervals:
        return None

    absolute_jitter = sorted(abs(interval - 2500) for interval in intervals)
    p99_index = min(len(absolute_jitter) - 1, math.ceil(len(absolute_jitter) * 0.99) - 1)
    return TimestampStats(
        contiguous_pairs=len(intervals),
        mean_interval_us=mean(intervals),
        std_interval_us=pstdev(intervals),
        min_interval_us=min(intervals),
        max_interval_us=max(intervals),
        p99_abs_jitter_us=absolute_jitter[p99_index],
        intervals_over_3000_us=sum(interval > 3000 for interval in intervals),
    )


def centered_drift_stats(
    samples: list[BlackboxSample],
    throttle_min: int = 500,
    command_threshold_dps: float = 0.5,
    settle_seconds: float = 0.5,
) -> CenteredDriftStats | None:
    """Measure rate bias with sticks centered and enough throttle for flight.

    All three commands must be centered so pilot input on one axis cannot be
    mistaken for passive drift on another. The signed mean rate is the primary
    I-term diagnostic. The signed rotation is integrated only across adjacent
    selected samples, so command/throttle gaps and flight boundaries are not
    bridged.
    """

    settle_samples = max(0, math.ceil(settle_seconds * CONTROL_RATE_HZ))
    selected: list[BlackboxSample] = []
    qualifying_run = 0
    previous: BlackboxSample | None = None
    for sample in samples:
        contiguous = (
            previous is not None
            and previous.flight_id == sample.flight_id
            and u32_forward_delta(previous.seq, sample.seq) == 1
        )
        qualifies = (
            sample.armed
            and sample.imu_fresh
            and sample.throttle >= throttle_min
            and all(
                abs(command) <= command_threshold_dps
                for command in sample.command_dps
            )
        )
        if not qualifies:
            qualifying_run = 0
        else:
            qualifying_run = qualifying_run + 1 if contiguous else 1
            if qualifying_run > settle_samples:
                selected.append(sample)
        previous = sample

    if not selected:
        return None

    signed_rotation = [0.0, 0.0, 0.0]
    integrated_duration_s = 0.0
    for previous, current in zip(selected, selected[1:]):
        if previous.flight_id != current.flight_id:
            continue
        if u32_forward_delta(previous.seq, current.seq) != 1:
            continue

        if previous.timestamp_us is not None and current.timestamp_us is not None:
            interval_us = u32_forward_delta(previous.timestamp_us, current.timestamp_us)
            if interval_us == 0 or interval_us > 10_000:
                continue
            interval_s = interval_us / 1_000_000.0
        else:
            interval_s = 1.0 / CONTROL_RATE_HZ

        integrated_duration_s += interval_s
        for axis in range(3):
            signed_rotation[axis] += (
                (previous.gyro_dps[axis] + current.gyro_dps[axis])
                * 0.5
                * interval_s
            )

    axis_stats_values: list[RateBiasAxisStats] = []
    for axis in range(3):
        values = [sample.gyro_dps[axis] for sample in selected]
        axis_stats_values.append(
            RateBiasAxisStats(
                mean_dps=mean(values),
                median_dps=median(values),
                std_dps=pstdev(values),
                mean_abs_dps=mean(abs(value) for value in values),
                signed_rotation_degrees=signed_rotation[axis],
            )
        )

    return CenteredDriftStats(
        sample_count=len(selected),
        observed_duration_s=len(selected) / CONTROL_RATE_HZ,
        integrated_duration_s=integrated_duration_s,
        axes=tuple(axis_stats_values),
    )


def print_centered_drift_report(
    samples: list[BlackboxSample],
    throttle_min: int,
    command_threshold_dps: float,
    settle_seconds: float,
) -> None:
    print()
    print("Centered-command throttle-on rate bias:")
    print(
        f"  filter: armed, fresh IMU, throttle >= {throttle_min}, "
        f"all |commands| <= {command_threshold_dps:.2f} dps, "
        f"settled >= {settle_seconds:.2f} s"
    )
    print(
        "  flight  samples  time_s   roll mean/std   pitch mean/std    yaw mean/std"
    )
    print(
        "  ------  -------  ------   -------------   --------------   -------------"
    )

    flight_ids = sorted(
        {sample.flight_id for sample in samples if sample.flight_id is not None}
    )
    groups: list[tuple[str, list[BlackboxSample]]] = []
    if len(flight_ids) > 1:
        groups.extend(
            (
                str(flight_id),
                [sample for sample in samples if sample.flight_id == flight_id],
            )
            for flight_id in flight_ids
        )
    groups.append(("all", samples))

    overall: CenteredDriftStats | None = None
    for label, group_samples in groups:
        stats = centered_drift_stats(
            group_samples,
            throttle_min=throttle_min,
            command_threshold_dps=command_threshold_dps,
            settle_seconds=settle_seconds,
        )
        if stats is None:
            print(f"  {label:>6}        0     0.0   insufficient centered samples")
            continue
        if label == "all":
            overall = stats
        roll, pitch, yaw = stats.axes
        print(
            f"  {label:>6}  {stats.sample_count:7d}  {stats.observed_duration_s:6.2f}   "
            f"{roll.mean_dps:+6.2f}/{roll.std_dps:5.2f}   "
            f"{pitch.mean_dps:+7.2f}/{pitch.std_dps:5.2f}   "
            f"{yaw.mean_dps:+6.2f}/{yaw.std_dps:5.2f}"
        )

    if overall is None:
        print("  No samples passed the drift filter; lower the thresholds only if justified.")
        return

    roll, pitch, yaw = overall.axes
    print(
        "  signed integrated rotation over contiguous selected intervals "
        f"({overall.integrated_duration_s:.2f} s): "
        f"roll {roll.signed_rotation_degrees:+.1f} deg, "
        f"pitch {pitch.signed_rotation_degrees:+.1f} deg, "
        f"yaw {yaw.signed_rotation_degrees:+.1f} deg"
    )
    print(
        "  Positive/negative values follow logged gyro signs. With a centered "
        "setpoint, the corresponding mean PID error has the opposite sign."
    )


def u32_forward_delta(previous: int, current: int) -> int:
    return (current - previous) & 0xFFFF_FFFF


def sequence_stats(samples: list[BlackboxSample]) -> SequenceStats:
    """Compare IMU progress with control progress, tolerating dropped RTT frames.

    A BB2 line is emitted once per control iteration. Dividing the IMU sequence
    advance by the control sequence advance therefore estimates the sensor sample
    rate without mistaking missing RTT lines for missing IMU samples.
    """

    valid_pairs = 0
    contiguous_control_pairs = 0
    missing_blackbox_frames = 0
    repeated_imu_samples = 0
    total_control_delta = 0
    total_imu_delta = 0
    contiguous_deltas: Counter[int] = Counter()

    for previous, current in zip(samples, samples[1:]):
        control_delta = u32_forward_delta(previous.seq, current.seq)
        imu_delta = u32_forward_delta(previous.imu_seq, current.imu_seq)

        # A reboot or concatenated capture appears as a huge unsigned jump. Do
        # not let that boundary dominate the rate estimate.
        if control_delta == 0 or control_delta > int(CONTROL_RATE_HZ * 60.0):
            continue
        if imu_delta > 100_000:
            continue

        valid_pairs += 1
        total_control_delta += control_delta
        total_imu_delta += imu_delta
        if control_delta == 1:
            contiguous_control_pairs += 1
            contiguous_deltas[imu_delta] += 1
            if imu_delta == 0:
                repeated_imu_samples += 1
        else:
            missing_blackbox_frames += control_delta - 1

    samples_per_tick = None
    estimated_rate_hz = None
    if total_control_delta:
        samples_per_tick = total_imu_delta / total_control_delta
        estimated_rate_hz = samples_per_tick * CONTROL_RATE_HZ

    return SequenceStats(
        valid_pairs=valid_pairs,
        contiguous_control_pairs=contiguous_control_pairs,
        missing_blackbox_frames=missing_blackbox_frames,
        repeated_imu_samples=repeated_imu_samples,
        imu_samples_per_control_tick=samples_per_tick,
        estimated_imu_rate_hz=estimated_rate_hz,
        contiguous_imu_delta_counts=tuple(sorted(contiguous_deltas.items())),
    )


def print_sequence_report(samples: list[BlackboxSample]) -> None:
    stats = sequence_stats(samples)
    print()
    print("Sequence/timing:")
    if stats.estimated_imu_rate_hz is None:
        print("  insufficient valid sequence pairs")
        return

    print(
        f"  estimated IMU rate={stats.estimated_imu_rate_hz:.1f} Hz "
        f"({stats.imu_samples_per_control_tick:.3f} samples/control tick)"
    )
    print(
        f"  repeated IMU samples={stats.repeated_imu_samples}/"
        f"{stats.contiguous_control_pairs} contiguous control pairs"
    )
    print(f"  missing BB2 frames={stats.missing_blackbox_frames}")
    histogram = ", ".join(
        f"{delta}:{count}" for delta, count in stats.contiguous_imu_delta_counts
    )
    print(f"  contiguous IMU delta histogram: {histogram or 'none'}")


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


def strongest_frequency(
    values: list[float],
    sample_rate_hz: float,
    min_frequency_hz: float = 0.5,
    max_frequency_hz: float = 100.0,
) -> tuple[float, float] | None:
    """Find the strongest frequency across bounded representative windows.

    The previous whole-log DFT capped the bin number rather than frequency, so
    a long flight could silently restrict the search to only a few hertz. A
    bounded 1024-sample window keeps runtime predictable while retaining
    coverage through 100 Hz at the 400 Hz control rate.
    """

    count = len(values)
    if count < 16:
        return None

    window_count = min(count, 1024)
    if count == window_count:
        starts = [0]
    else:
        candidate_count = min(8, max(2, math.ceil(count / window_count)))
        starts = [
            round(index * (count - window_count) / (candidate_count - 1))
            for index in range(candidate_count)
        ]

    min_bin = max(1, math.ceil(min_frequency_hz * window_count / sample_rate_hz))
    max_bin = min(
        window_count // 2,
        math.floor(max_frequency_hz * window_count / sample_rate_hz),
    )
    if min_bin > max_bin:
        return None

    best_frequency = 0.0
    best_magnitude = 0.0
    for start in starts:
        window = values[start : start + window_count]
        center = mean(window)
        centered = [value - center for value in window]
        for bin_index in range(min_bin, max_bin + 1):
            real = 0.0
            imag = 0.0
            for sample_index, value in enumerate(centered):
                phase = 2.0 * math.pi * bin_index * sample_index / window_count
                real += value * math.cos(phase)
                imag -= value * math.sin(phase)
            magnitude = math.sqrt(real * real + imag * imag) / window_count
            if magnitude > best_magnitude:
                best_magnitude = magnitude
                best_frequency = bin_index * sample_rate_hz / window_count
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
                "timestamp_us",
                "flight_id",
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
                    sample.timestamp_us if sample.timestamp_us is not None else "",
                    sample.flight_id if sample.flight_id is not None else "",
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
    flash_summaries: list[FlashLogStats] = []

    try:
        paths = args.log_files or [latest_log_file()]
        for path in paths:
            resolved = path if path.is_absolute() else REPO_ROOT / path
            if resolved.suffix.lower() == ".fwbb":
                data = resolved.read_bytes()
                flash_summaries.append(flash_log_stats(data))
                samples.extend(samples_from_flash_bytes(data))
            else:
                samples.extend(samples_from_file(resolved))
            sources.append(str(resolved))
    except (FileNotFoundError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    try:
        samples, selected_flight_id = select_flight_samples(samples, args.flight_id)
    except ValueError as error:
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

    flight_window_input_count = len(samples)
    if args.flight_window_throttle_min is not None:
        samples = longest_flight_window(samples, args.flight_window_throttle_min)
        if not samples:
            print(
                "error: no contiguous armed/fresh-IMU samples meet "
                f"--flight-window-throttle-min {args.flight_window_throttle_min}.",
                file=sys.stderr,
            )
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
    for stats in flash_summaries:
        flight_ids = ",".join(str(value) for value in stats.flight_ids) or "none"
        print(
            f"Flash pages: {stats.page_count} CRC-valid; records: {stats.record_count}; "
            f"flight IDs: {flight_ids}; page sequence: "
            f"{stats.first_page_sequence}..{stats.last_page_sequence}; "
            f"partial pages: {stats.partial_page_count}; "
            f"final page: {stats.final_page_records}/{FLASH_PAGE_DATA_LEN // FLASH_RECORD_LEN} records"
        )
    if selected_flight_id is not None:
        print(f"Selected flight ID: {selected_flight_id}")
    elif len({sample.flight_id for sample in samples if sample.flight_id is not None}) > 1:
        print(
            "Warning: multiple flight IDs are combined; select --flight-id latest "
            "or a numeric ID for timing and missing-frame conclusions."
        )
    print(f"Format: BB{max(sample.version for sample in samples)} blackbox")
    print(f"Samples: {len(samples)}  seq: {first_seq}..{last_seq}  estimated rate: {sample_rate_hz:.1f} Hz")
    if trimmed_start or trimmed_end:
        print(f"Trimmed samples: start={trimmed_start}, end={trimmed_end}")
    if args.flight_window_throttle_min is not None:
        print(
            "Flight window: "
            f"{len(samples)}/{flight_window_input_count} samples in longest contiguous "
            f"armed/fresh interval with throttle >= {args.flight_window_throttle_min}"
        )
    print(f"Armed samples: {sum(1 for sample in samples if sample.armed)}")
    print(f"Fresh IMU samples: {sum(1 for sample in samples if sample.imu_fresh)}")
    timing = timestamp_stats(samples)
    if timing is not None:
        print(
            "Control timestamps: "
            f"pairs={timing.contiguous_pairs} mean={timing.mean_interval_us:.1f} us "
            f"std={timing.std_interval_us:.1f} us min/max="
            f"{timing.min_interval_us}/{timing.max_interval_us} us "
            f"p99 |jitter|={timing.p99_abs_jitter_us} us "
            f">3000 us={timing.intervals_over_3000_us}"
        )
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
    print_sequence_report(samples)
    print_control_report(samples)
    if args.drift_report:
        print_centered_drift_report(
            samples,
            throttle_min=args.drift_throttle_min,
            command_threshold_dps=args.drift_command_threshold_dps,
            settle_seconds=args.drift_settle_seconds,
        )

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
