#!/usr/bin/env python3
"""Create a compact FerroWasp BB2 flight report with plots and event slices."""

from __future__ import annotations

import argparse
import csv
import re
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, pstdev

import matplotlib.pyplot as plt

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOG_DIR = REPO_ROOT / "logs" / "remote_probe"
DEFAULT_REPORT_DIR = DEFAULT_LOG_DIR / "reports"
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

AXES = ("roll", "pitch", "yaw")
COLORS = {"roll": "tab:red", "pitch": "tab:green", "yaw": "tab:blue"}


@dataclass
class Event:
    name: str
    index: int
    axis: str
    value: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate FerroWasp flight report plots and a markdown summary."
    )
    parser.add_argument(
        "source",
        nargs="?",
        type=Path,
        help="BB2 .log or analyzer .csv. Defaults to newest .log/.csv in logs/remote_probe.",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        help="Report directory. Defaults to logs/remote_probe/reports/<source-stem>.",
    )
    parser.add_argument(
        "--trim-start",
        type=float,
        default=0.0,
        help="Discard this many seconds from the start before reporting.",
    )
    parser.add_argument(
        "--trim-end",
        type=float,
        default=0.0,
        help="Discard this many seconds from the end before reporting.",
    )
    parser.add_argument(
        "--sample-rate",
        type=float,
        help="Override estimated sample rate in Hz.",
    )
    parser.add_argument(
        "--zoom",
        type=float,
        default=2.5,
        help="Seconds on either side of worst-event zoom plots.",
    )
    return parser.parse_args()


def latest_source() -> Path:
    candidates = list(DEFAULT_LOG_DIR.glob("*.log")) + list(DEFAULT_LOG_DIR.glob("*.csv"))
    if not candidates:
        raise FileNotFoundError(f"No .log or .csv files found in {DEFAULT_LOG_DIR}")
    return max(candidates, key=lambda path: path.stat().st_mtime)


def sample_from_match(match: re.Match[str]) -> dict[str, float]:
    flags = int(match.group("flags"))
    raw = {"raw_roll_dps": 0.0, "raw_pitch_dps": 0.0, "raw_yaw_dps": 0.0}
    if match.group("raw_roll") is not None:
        raw = {
            "raw_roll_dps": int(match.group("raw_roll")) / 10.0,
            "raw_pitch_dps": int(match.group("raw_pitch")) / 10.0,
            "raw_yaw_dps": int(match.group("raw_yaw")) / 10.0,
        }
    return {
        "seq": float(match.group("seq")),
        "imu_seq": float(match.group("imu_seq")),
        "flags": float(flags),
        "armed": float(1 if flags & 0x01 else 0),
        "imu_fresh": float(1 if flags & 0x02 else 0),
        **raw,
        "gyro_roll_dps": int(match.group("gyro_roll")) / 10.0,
        "gyro_pitch_dps": int(match.group("gyro_pitch")) / 10.0,
        "gyro_yaw_dps": int(match.group("gyro_yaw")) / 10.0,
        "cmd_roll_dps": int(match.group("cmd_roll")) / 10.0,
        "cmd_pitch_dps": int(match.group("cmd_pitch")) / 10.0,
        "cmd_yaw_dps": int(match.group("cmd_yaw")) / 10.0,
        "pid_roll": float(match.group("pid_roll")),
        "pid_pitch": float(match.group("pid_pitch")),
        "pid_yaw": float(match.group("pid_yaw")),
        "throttle": float(match.group("throttle")),
        "motor1": float(match.group("motor1")),
        "motor2": float(match.group("motor2")),
        "motor3": float(match.group("motor3")),
        "motor4": float(match.group("motor4")),
    }


def load_log(path: Path) -> list[dict[str, float]]:
    rows: list[dict[str, float]] = []
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            match = BB_RE.search(line)
            if match is not None:
                rows.append(sample_from_match(match))
    return rows


def load_csv(path: Path) -> list[dict[str, float]]:
    rows: list[dict[str, float]] = []
    with path.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            rows.append({key: float(value) if value != "" else 0.0 for key, value in row.items()})
    return rows


def load_samples(path: Path) -> list[dict[str, float]]:
    if path.suffix.lower() == ".csv":
        return load_csv(path)
    return load_log(path)


def estimate_sample_rate(rows: list[dict[str, float]], override: float | None) -> float:
    if override and override > 0:
        return override
    if len(rows) < 2:
        return CONTROL_RATE_HZ
    seq_span = max(1.0, rows[-1]["seq"] - rows[0]["seq"])
    duration = seq_span / CONTROL_RATE_HZ
    return (len(rows) - 1) / duration


def prepare_rows(
    rows: list[dict[str, float]], sample_rate: float, trim_start: float, trim_end: float
) -> list[dict[str, float]]:
    start = max(0, int(round(trim_start * sample_rate)))
    end = len(rows) - max(0, int(round(trim_end * sample_rate)))
    if start >= end:
        raise ValueError("trim would remove all samples")
    trimmed = rows[start:end]
    for index, row in enumerate(trimmed):
        row["t"] = index / sample_rate
        for axis in AXES:
            row[f"err_{axis}_dps"] = row[f"cmd_{axis}_dps"] - row[f"gyro_{axis}_dps"]
    return trimmed


def values(rows: list[dict[str, float]], key: str) -> list[float]:
    return [row[key] for row in rows]


def subset(rows: list[dict[str, float]], pred) -> list[dict[str, float]]:
    return [row for row in rows if pred(row)]


def safe_mean(values_: list[float]) -> float:
    return mean(values_) if values_ else 0.0


def safe_std(values_: list[float]) -> float:
    return pstdev(values_) if len(values_) > 1 else 0.0


def find_events(rows: list[dict[str, float]]) -> list[Event]:
    candidates = subset(rows, lambda row: row["armed"] >= 1 and row["throttle"] >= 500) or rows
    events: list[Event] = []
    for axis in AXES:
        events.append(
            Event(
                name=f"max_abs_{axis}_error",
                index=max(range(len(candidates)), key=lambda i: abs(candidates[i][f"err_{axis}_dps"])),
                axis=axis,
                value=max(candidates, key=lambda row: abs(row[f"err_{axis}_dps"]))[f"err_{axis}_dps"],
            )
        )
    for axis in AXES:
        events.append(
            Event(
                name=f"max_abs_{axis}_pid",
                index=max(range(len(candidates)), key=lambda i: abs(candidates[i][f"pid_{axis}"])),
                axis=axis,
                value=max(candidates, key=lambda row: abs(row[f"pid_{axis}"]))[f"pid_{axis}"],
            )
        )
    # Map event indices back to the original row list by sequence number.
    mapped: list[Event] = []
    for event in events:
        candidate = candidates[event.index]
        original_index = min(
            range(len(rows)), key=lambda idx: abs(rows[idx]["seq"] - candidate["seq"])
        )
        mapped.append(Event(event.name, original_index, event.axis, event.value))
    return mapped


def plot_setpoint_vs_measured(rows: list[dict[str, float]], out_dir: Path) -> Path:
    fig, axs = plt.subplots(4, 1, figsize=(15, 11), sharex=True)
    fig.suptitle("Setpoint vs measured rate")
    for ax, axis in zip(axs[:3], AXES):
        ax.plot(values(rows, "t"), values(rows, f"cmd_{axis}_dps"), "--", color=COLORS[axis], label=f"{axis} setpoint")
        ax.plot(values(rows, "t"), values(rows, f"gyro_{axis}_dps"), color=COLORS[axis], alpha=0.75, label=f"{axis} measured")
        ax.axhline(0, color="black", linewidth=0.6, alpha=0.4)
        ax.set_ylabel("rate dps")
        ax.grid(True, alpha=0.25)
        ax.legend(ncol=2, fontsize=8)
    plot_motors(axs[3], rows)
    axs[3].set_xlabel("time (s)")
    fig.tight_layout()
    path = out_dir / "setpoint_vs_measured.png"
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return path


def plot_error_pid(rows: list[dict[str, float]], out_dir: Path) -> Path:
    fig, axs = plt.subplots(3, 1, figsize=(15, 9), sharex=True)
    fig.suptitle("Rate tracking error and PID output")
    for axis in AXES:
        axs[0].plot(values(rows, "t"), values(rows, f"err_{axis}_dps"), color=COLORS[axis], label=f"{axis} error")
    axs[0].axhline(0, color="black", linewidth=0.6, alpha=0.4)
    axs[0].set_ylabel("cmd - gyro (dps)")
    axs[0].grid(True, alpha=0.25)
    axs[0].legend(ncol=3, fontsize=8)
    for axis in AXES:
        axs[1].plot(values(rows, "t"), values(rows, f"pid_{axis}"), color=COLORS[axis], label=f"{axis} pid")
    axs[1].set_ylabel("PID output")
    axs[1].grid(True, alpha=0.25)
    axs[1].legend(ncol=3, fontsize=8)
    plot_motors(axs[2], rows)
    axs[2].set_xlabel("time (s)")
    fig.tight_layout()
    path = out_dir / "rate_error_pid_motors.png"
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return path


def plot_event_zoom(rows: list[dict[str, float]], event: Event, out_dir: Path, zoom_s: float) -> Path:
    center_t = rows[event.index]["t"]
    zoom_rows = subset(rows, lambda row: center_t - zoom_s <= row["t"] <= center_t + zoom_s)
    fig, axs = plt.subplots(4, 1, figsize=(15, 11), sharex=True)
    fig.suptitle(f"{event.name} zoom at {center_t:.2f}s")
    for axis in AXES:
        axs[0].plot(values(zoom_rows, "t"), values(zoom_rows, f"err_{axis}_dps"), color=COLORS[axis], label=f"{axis} error")
    axs[0].axhline(0, color="black", linewidth=0.6, alpha=0.4)
    axs[0].axvline(center_t, color="black", linestyle=":")
    axs[0].set_ylabel("cmd - gyro")
    axs[0].grid(True, alpha=0.25)
    axs[0].legend(ncol=3, fontsize=8)
    for axis in AXES:
        axs[1].plot(values(zoom_rows, "t"), values(zoom_rows, f"cmd_{axis}_dps"), "--", color=COLORS[axis], label=f"cmd {axis}")
        axs[1].plot(values(zoom_rows, "t"), values(zoom_rows, f"gyro_{axis}_dps"), color=COLORS[axis], alpha=0.65, label=f"gyro {axis}")
    axs[1].axvline(center_t, color="black", linestyle=":")
    axs[1].set_ylabel("rate dps")
    axs[1].grid(True, alpha=0.25)
    axs[1].legend(ncol=3, fontsize=7)
    for axis in AXES:
        axs[2].plot(values(zoom_rows, "t"), values(zoom_rows, f"pid_{axis}"), color=COLORS[axis], label=f"{axis} pid")
    axs[2].axvline(center_t, color="black", linestyle=":")
    axs[2].set_ylabel("PID output")
    axs[2].grid(True, alpha=0.25)
    axs[2].legend(ncol=3, fontsize=8)
    plot_motors(axs[3], zoom_rows)
    axs[3].axvline(center_t, color="black", linestyle=":")
    axs[3].set_xlabel("time (s)")
    fig.tight_layout()
    path = out_dir / f"{event.name}_zoom.png"
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return path


def plot_motors(ax, rows: list[dict[str, float]]) -> None:
    ax.plot(values(rows, "t"), values(rows, "throttle"), color="black", linewidth=1.0, label="throttle")
    for motor, color in ((1, "tab:red"), (2, "tab:orange"), (3, "tab:purple"), (4, "tab:cyan")):
        ax.plot(values(rows, "t"), values(rows, f"motor{motor}"), color=color, alpha=0.75, label=f"motor {motor}")
    ax.set_ylabel("motor/throttle")
    ax.grid(True, alpha=0.25)
    ax.legend(ncol=5, fontsize=8)


def write_summary(
    path: Path,
    source: Path,
    rows: list[dict[str, float]],
    sample_rate: float,
    plots: list[Path],
    events: list[Event],
) -> None:
    armed = subset(rows, lambda row: row["armed"] >= 1)
    throttle_on = subset(armed, lambda row: row["throttle"] >= 500)
    centered_yaw = subset(throttle_on, lambda row: abs(row["cmd_yaw_dps"]) <= 5)
    all_centered = subset(
        throttle_on,
        lambda row: all(abs(row[f"cmd_{axis}_dps"]) <= 50 for axis in AXES),
    )

    lines = [
        f"# Flight Report: {source.name}",
        "",
        f"- samples: `{len(rows)}`",
        f"- estimated sample rate: `{sample_rate:.1f} Hz`",
        f"- armed samples: `{len(armed)}`",
        f"- throttle-on samples (`thr >= 500`): `{len(throttle_on)}`",
        f"- centered-yaw throttle-on samples: `{len(centered_yaw)}`",
        f"- roughly centered-stick throttle-on samples (`abs cmd <= 50 dps`): `{len(all_centered)}`",
        "",
        "## Plots",
        "",
    ]
    lines += [f"- [{plot.name}]({plot.name})" for plot in plots]
    lines += ["", "## Tracking Error Summary", ""]
    lines += ["| Slice | Axis | Error mean dps | Error std dps | Gyro mean dps | Cmd mean dps | PID mean |", "|---|---|---:|---:|---:|---:|---:|"]
    for label, sample_rows in (
        ("armed", armed),
        ("throttle_on", throttle_on),
        ("centered_yaw", centered_yaw),
        ("all_centered", all_centered),
    ):
        for axis in AXES:
            err = values(sample_rows, f"err_{axis}_dps") if sample_rows else []
            gyro = values(sample_rows, f"gyro_{axis}_dps") if sample_rows else []
            cmd = values(sample_rows, f"cmd_{axis}_dps") if sample_rows else []
            pid = values(sample_rows, f"pid_{axis}") if sample_rows else []
            lines.append(
                f"| {label} | {axis} | {safe_mean(err):.2f} | {safe_std(err):.2f} | "
                f"{safe_mean(gyro):.2f} | {safe_mean(cmd):.2f} | {safe_mean(pid):.2f} |"
            )
    lines += ["", "## Worst Events", ""]
    lines += ["| Event | Time s | Axis | Value | Cmd dps | Gyro dps | PID | Throttle |", "|---|---:|---|---:|---:|---:|---:|---:|"]
    for event in events:
        row = rows[event.index]
        axis = event.axis
        lines.append(
            f"| {event.name} | {row['t']:.2f} | {axis} | {event.value:.2f} | "
            f"{row[f'cmd_{axis}_dps']:.2f} | {row[f'gyro_{axis}_dps']:.2f} | "
            f"{row[f'pid_{axis}']:.2f} | {row['throttle']:.0f} |"
        )
    lines += [
        "",
        "## Reading Notes",
        "",
        "- `error = commanded rate - measured gyro rate`.",
        "- Large error with PID in the same direction means the controller is trying to correct.",
        "- Large error with saturated motors points to authority or mixing limits.",
        "- Opposite sign motion during simple single-axis inputs points to sign, axis, or mixer bugs.",
    ]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    source = args.source or latest_source()
    rows = load_samples(source)
    if not rows:
        raise SystemExit(f"No BB1/BB2 samples found in {source}")
    sample_rate = estimate_sample_rate(rows, args.sample_rate)
    rows = prepare_rows(rows, sample_rate, args.trim_start, args.trim_end)
    out_dir = args.out_dir or DEFAULT_REPORT_DIR / source.stem
    out_dir.mkdir(parents=True, exist_ok=True)

    events = find_events(rows)
    worst_error_events = [event for event in events if event.name.startswith("max_abs_") and event.name.endswith("_error")]
    plots = [
        plot_setpoint_vs_measured(rows, out_dir),
        plot_error_pid(rows, out_dir),
    ]
    for event in worst_error_events:
        plots.append(plot_event_zoom(rows, event, out_dir, args.zoom))

    summary = out_dir / "report.md"
    write_summary(summary, source, rows, sample_rate, plots, events)
    print(f"Report: {summary}")
    for plot in plots:
        print(f"Plot:   {plot}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
