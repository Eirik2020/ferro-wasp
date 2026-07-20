#!/usr/bin/env python3
"""Live IMU view from terminal_embed RTT output."""

from __future__ import annotations

import argparse
import json
import math
import os
import queue
import re
import socket
import subprocess
import sys
import threading
import time
import tkinter as tk
from collections import deque
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse

REPO_ROOT = Path(__file__).resolve().parents[1]
TERMINAL_EMBED = REPO_ROOT / "tools" / "terminal_embed.py"
DEFAULT_REMOTE_HOST = "ws://your-probe-host.local:3000"
DEFAULT_REMOTE_ELF = REPO_ROOT / "target-codex-fresh/thumbv7em-none-eabihf/release/FerroWasp"
DEFAULT_REMOTE_CHIP = "STM32F405RG"
DEFAULT_REMOTE_PROBE = "0:0:/dev/spidev0.0"
LOCAL_CONFIG = REPO_ROOT / "local_config" / "ferrowasp.local.json"
IMU_RE = re.compile(
    r"Heartbeat: IMU seq (?P<seq>\d+), "
    r"raw \[(?P<raw_x>-?\d+), (?P<raw_y>-?\d+), (?P<raw_z>-?\d+)\], "
    r"acc_mg \[(?P<acc_x>-?\d+), (?P<acc_y>-?\d+), (?P<acc_z>-?\d+)\], "
    r"rates \[(?P<gyro_x>-?\d+), (?P<gyro_y>-?\d+), (?P<gyro_z>-?\d+)\]"
)
BB_RE = re.compile(
    r"BB(?P<version>[12]) seq (?P<seq>\d+) imu (?P<imu_seq>\d+) flags (?P<flags>\d+) "
    r"(?:raw10 \[(?P<raw_roll>-?\d+), (?P<raw_pitch>-?\d+), (?P<raw_yaw>-?\d+)\] )?"
    r"gyro10 \[(?P<gyro_roll>-?\d+), (?P<gyro_pitch>-?\d+), (?P<gyro_yaw>-?\d+)\] "
    r"cmd10 \[(?P<cmd_roll>-?\d+), (?P<cmd_pitch>-?\d+), (?P<cmd_yaw>-?\d+)\] "
    r"pid \[(?P<pid_roll>-?\d+), (?P<pid_pitch>-?\d+), (?P<pid_yaw>-?\d+)\] "
    r"thr (?P<throttle>\d+) "
    r"motors \[(?P<motor1>\d+), (?P<motor2>\d+), (?P<motor3>\d+), (?P<motor4>\d+)\]"
)
ROLL_ACCEL_SIGN = 1.0
PITCH_ACCEL_SIGN = 1.0
ACCEL_GROUNDING_ALPHA = 0.08


@dataclass(frozen=True)
class ImuSample:
    seq: int
    timestamp_s: float
    acc_g: tuple[float, float, float]
    gyro_dps: tuple[float, float, float]
    raw_gyro: tuple[int, int, int]


@dataclass(frozen=True)
class ControlSample:
    version: int
    seq: int
    imu_seq: int
    timestamp_s: float
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


LiveSample = ImuSample | ControlSample


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Launch the RTT terminal embed tool and live-view accel, gyro, and attitude."
    )
    parser.add_argument(
        "--samples",
        type=int,
        default=240,
        help="Number of recent IMU samples to keep in the plots.",
    )
    parser.add_argument(
        "--attitude",
        choices=("gyro", "accel"),
        default="gyro",
        help="Use 'gyro' for gyro-only after initial accel seed, or 'accel' for continuous roll/pitch accel grounding.",
    )
    parser.add_argument(
        "--remote",
        action="store_true",
        help="Attach through the remote Pi probe-rs server instead of using terminal_embed.py defaults.",
    )
    parser.add_argument(
        "--probe-host",
        default=None,
        help=f"Remote probe-rs host URI. Defaults to FERROWASP_PROBE_HOST or {DEFAULT_REMOTE_HOST}.",
    )
    parser.add_argument(
        "--link",
        choices=("auto", "cable", "mobile", "zero", "zeromobile"),
        default="auto",
        help="Named Pi network link for --remote when --probe-host is not set.",
    )
    parser.add_argument(
        "--probe-token",
        default=None,
        help="Remote probe-rs token. Defaults to FERROWASP_PROBE_TOKEN or local_config/ferrowasp.local.json.",
    )
    parser.add_argument(
        "--elf",
        type=Path,
        default=DEFAULT_REMOTE_ELF,
        help="ELF path used for remote attach and defmt decoding.",
    )
    parser.add_argument(
        "--chip",
        default=DEFAULT_REMOTE_CHIP,
        help="Target chip name for probe-rs.",
    )
    parser.add_argument(
        "--probe",
        default=DEFAULT_REMOTE_PROBE,
        help="probe-rs probe selector for the Pi GPIO/SPI SWD backend.",
    )
    parser.add_argument(
        "--speed",
        type=int,
        default=1000,
        help="SWD speed in kHz.",
    )
    parser.add_argument(
        "--ui-hz",
        type=float,
        default=30.0,
        help="Maximum UI refresh rate. Lower this if Tk drawing is heavy.",
    )
    parser.add_argument(
        "--queue-samples",
        type=int,
        default=800,
        help="Maximum parsed samples waiting for the UI before oldest samples are dropped.",
    )
    parser.add_argument(
        "--echo-rtt",
        action="store_true",
        help="Echo every RTT line to stdout. Disabled by default because 400 Hz blackbox output can lag the viewer.",
    )
    parser.add_argument(
        "command",
        nargs=argparse.REMAINDER,
        help="Optional command after '--'. Defaults to running tools/terminal_embed.py.",
    )
    return parser.parse_args()


def remote_command_from_args(args: argparse.Namespace) -> list[str]:
    host = probe_host_from_args(args)
    token = args.probe_token or config_default("probe_token", "FERROWASP_PROBE_TOKEN")
    if not token:
        raise SystemExit(
            "Remote probe token not configured. Pass --probe-token, set "
            "FERROWASP_PROBE_TOKEN, or create local_config/ferrowasp.local.json."
        )
    return [
        sys.executable,
        str(TERMINAL_EMBED),
        "--",
        "probe-rs",
        "--host",
        host,
        "--token",
        token,
        "attach",
        str(args.elf),
        "--chip",
        args.chip,
        "--protocol",
        "swd",
        "--probe",
        args.probe,
        "--speed",
        str(args.speed),
    ]


def getenv_default(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value else default


def load_local_config() -> dict[str, object]:
    if not LOCAL_CONFIG.exists():
        return {}
    with LOCAL_CONFIG.open("r", encoding="utf-8") as config_file:
        data = json.load(config_file)
    return data if isinstance(data, dict) else {}


def config_default(name: str, env_name: str, default: str = "") -> str:
    value = os.environ.get(env_name)
    if value:
        return value
    value = load_local_config().get(name)
    return value if isinstance(value, str) and value else default


def probe_host_from_args(args: argparse.Namespace) -> str:
    return args.probe_host or remote_host_from_env_or_link(args.link)


def remote_host_from_env_or_link(link: str) -> str:
    if link == "auto":
        return config_probe_host(link, "FERROWASP_PROBE_HOST", DEFAULT_REMOTE_HOST)
    return remote_host_for_link(link)


def remote_host_for_link(link: str) -> str:
    host = config_probe_host(link)
    if not host:
        raise SystemExit(
            f"Remote probe host for link '{link}' is not configured. Pass "
            "--probe-host or set local_config/ferrowasp.local.json probe_hosts."
        )
    return host


def config_probe_host(link: str, env_name: str = "", default: str = "") -> str:
    if env_name:
        value = os.environ.get(env_name)
        if value:
            return value
    hosts = load_local_config().get("probe_hosts")
    if isinstance(hosts, dict):
        value = hosts.get(link)
        if isinstance(value, str) and value:
            return value
    return default


def check_remote_probe_reachable(host_uri: str, timeout_s: float = 2.0) -> str | None:
    parsed = urlparse(host_uri)
    host = parsed.hostname
    port = parsed.port
    if host is None or port is None:
        return f"Could not parse remote probe host URI: {host_uri}"
    if host.endswith(".local"):
        return None
    try:
        with socket.create_connection((host, port), timeout=timeout_s):
            return None
    except OSError as error:
        return (
            f"Could not reach remote probe-rs at {host_uri}: {error}. "
            "Make sure the laptop is on the same network as the selected Pi link "
            "and that probe-rs serve is running."
        )


def command_from_args(args: argparse.Namespace) -> list[str]:
    raw_command = args.command
    if not raw_command:
        if args.remote:
            return remote_command_from_args(args)
        return [sys.executable, str(TERMINAL_EMBED)]
    if raw_command[0] == "--":
        return list(raw_command[1:])
    return list(raw_command)


class ImuLiveView:
    def __init__(
        self,
        root: tk.Tk,
        command: Sequence[str],
        max_samples: int,
        attitude_mode: str,
        ui_hz: float,
        queue_samples: int,
        echo_rtt: bool,
    ) -> None:
        self.root = root
        self.command = list(command)
        self.attitude_mode = attitude_mode
        self.echo_rtt = echo_rtt
        self.poll_ms = max(16, int(1000 / max(1.0, ui_hz)))
        self.samples: deque[ImuSample] = deque(maxlen=max_samples)
        self.control_samples: deque[ControlSample] = deque(maxlen=max_samples)
        self.queue: queue.Queue[LiveSample | str | None] = queue.Queue(
            maxsize=max(32, queue_samples)
        )
        self.process: subprocess.Popen[str] | None = None
        self.dropped_samples = 0

        self.roll_rad = 0.0
        self.pitch_rad = 0.0
        self.yaw_rad = 0.0
        self.attitude_seeded = False
        self.last_sample_time: float | None = None

        self.canvas = tk.Canvas(root, width=1120, height=680, bg="#101418")
        self.canvas.pack(fill=tk.BOTH, expand=True)
        self.status = tk.StringVar(value="Starting RTT IMU live view...")
        tk.Label(root, textvariable=self.status, anchor="w").pack(fill=tk.X)

        root.title("FerroWasp IMU Live View")
        root.protocol("WM_DELETE_WINDOW", self.close)

    def start(self) -> None:
        thread = threading.Thread(target=self.reader_thread, daemon=True)
        thread.start()
        self.root.after(self.poll_ms, self.poll_queue)

    def reader_thread(self) -> None:
        try:
            self.process = subprocess.Popen(
                self.command,
                cwd=REPO_ROOT,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
            )
        except OSError as error:
            print(f"Could not start command: {error}", file=sys.stderr)
            self.enqueue_close()
            return

        assert self.process.stdout is not None
        for line in self.process.stdout:
            text = line.rstrip()
            if text and self.echo_rtt:
                print(text, flush=True)
            now = time.monotonic()
            sample = parse_imu_sample(text, now)
            if sample is None:
                sample = parse_control_sample(text, now)
            if sample is not None:
                self.enqueue_sample(sample)
            elif text:
                self.enqueue_status(text)

        self.enqueue_close()

    def enqueue_sample(self, sample: LiveSample) -> None:
        try:
            self.queue.put_nowait(sample)
        except queue.Full:
            try:
                self.queue.get_nowait()
            except queue.Empty:
                pass
            self.dropped_samples += 1
            try:
                self.queue.put_nowait(sample)
            except queue.Full:
                self.dropped_samples += 1

    def enqueue_status(self, text: str) -> None:
        try:
            self.queue.put_nowait(text)
        except queue.Full:
            pass

    def enqueue_close(self) -> None:
        try:
            self.queue.put_nowait(None)
        except queue.Full:
            try:
                self.queue.get_nowait()
            except queue.Empty:
                pass
            try:
                self.queue.put_nowait(None)
            except queue.Full:
                pass

    def poll_queue(self) -> None:
        closed = False
        processed = 0
        max_process_per_frame = max(100, self.queue.maxsize)
        while True:
            try:
                item = self.queue.get_nowait()
            except queue.Empty:
                break
            if item is None:
                closed = True
            elif isinstance(item, str):
                self.status.set(item)
            elif isinstance(item, ControlSample):
                self.control_samples.append(item)
            else:
                self.samples.append(item)
                self.update_attitude(item)
            processed += 1
            if processed >= max_process_per_frame:
                break

        self.draw()
        if closed:
            self.status.set("RTT process exited.")
            return

        self.root.after(self.poll_ms, self.poll_queue)

    def update_attitude(self, sample: ImuSample) -> None:
        if not self.attitude_seeded:
            self.seed_roll_pitch_from_accel(sample.acc_g)
            self.attitude_seeded = True

        if self.last_sample_time is not None:
            dt = max(0.0, min(1.0, sample.timestamp_s - self.last_sample_time))
            self.roll_rad += math.radians(sample.gyro_dps[0]) * dt
            self.pitch_rad += math.radians(sample.gyro_dps[1]) * dt
            self.yaw_rad += math.radians(sample.gyro_dps[2]) * dt
            if self.attitude_mode == "accel":
                self.apply_accel_grounding(sample.acc_g)
        self.last_sample_time = sample.timestamp_s

    def seed_roll_pitch_from_accel(self, acc_g: tuple[float, float, float]) -> None:
        self.roll_rad, self.pitch_rad = accel_roll_pitch(acc_g)

    def apply_accel_grounding(self, acc_g: tuple[float, float, float]) -> None:
        acc_roll, acc_pitch = accel_roll_pitch(acc_g)
        self.roll_rad = blend_angle(self.roll_rad, acc_roll, ACCEL_GROUNDING_ALPHA)
        self.pitch_rad = blend_angle(self.pitch_rad, acc_pitch, ACCEL_GROUNDING_ALPHA)

    def attitude_mode_label(self) -> str:
        if self.attitude_mode == "accel":
            return "gyro + accel grounding"
        return "gyro-only"

    def attitude_hint(self) -> str:
        if self.attitude_mode == "accel":
            return "Roll/pitch are gyro-integrated and gently grounded by accel."
        return "Roll/pitch seeded once from accel, then integrated from gyro rates."

    def draw(self) -> None:
        width = max(self.canvas.winfo_width(), 400)
        height = max(self.canvas.winfo_height(), 320)
        self.canvas.delete("all")

        if self.control_samples:
            self.draw_control_view(width, height)
            return

        self.draw_imu_view(width, height)

    def draw_imu_view(self, width: int, height: int) -> None:
        left_w = int(width * 0.58)
        self.draw_chart(
            x0=40,
            y0=48,
            x1=left_w - 24,
            y1=int(height * 0.47),
            title="Accel (g)",
            values=[sample.acc_g for sample in self.samples],
            labels=("X", "Y", "Z"),
            colors=("#ff5f56", "#35c46b", "#58a6ff"),
        )
        self.draw_chart(
            x0=40,
            y0=int(height * 0.54),
            x1=left_w - 24,
            y1=height - 60,
            title="Gyro rates (dps)",
            values=[sample.gyro_dps for sample in self.samples],
            labels=("X", "Y", "Z"),
            colors=("#ff9f43", "#b982ff", "#6ee7f9"),
        )
        self.draw_body_3d(left_w + 20, 48, width - 40, height - 60)

        if not self.samples:
            self.status.set("Waiting for IMU samples...")
            return

        sample = self.samples[-1]
        roll_deg = math.degrees(self.roll_rad)
        pitch_deg = math.degrees(self.pitch_rad)
        yaw_deg = math.degrees(self.yaw_rad)
        ax, ay, az = sample.acc_g
        gx, gy, gz = sample.gyro_dps
        self.status.set(
            f"seq={sample.seq}  acc=({ax:.3f}, {ay:.3f}, {az:.3f}) g  "
            f"gyro=({gx:.1f}, {gy:.1f}, {gz:.1f}) dps  "
            f"attitude roll={roll_deg:.1f} pitch={pitch_deg:.1f} yaw~={yaw_deg:.1f}"
        )

    def draw_control_view(self, width: int, height: int) -> None:
        top_y = 46
        gap = 44
        chart_h = max(108, int((height - 120) / 3))
        chart_w = max(320, width - 80)
        x0 = 40
        x1 = x0 + chart_w

        self.draw_chart(
            x0=x0,
            y0=top_y,
            x1=x1,
            y1=top_y + chart_h,
            title="Control rates (dps): raw vs filtered vs command",
            values=[
                (
                    sample.raw_dps[0] if sample.raw_dps is not None else sample.gyro_dps[0],
                    sample.raw_dps[1] if sample.raw_dps is not None else sample.gyro_dps[1],
                    sample.raw_dps[2] if sample.raw_dps is not None else sample.gyro_dps[2],
                    sample.gyro_dps[0],
                    sample.gyro_dps[1],
                    sample.gyro_dps[2],
                    sample.command_dps[0],
                    sample.command_dps[1],
                    sample.command_dps[2],
                )
                for sample in self.control_samples
            ],
            labels=("rR", "rP", "rY", "gR", "gP", "gY", "cR", "cP", "cY"),
            colors=(
                "#7f4f24",
                "#5a3a8a",
                "#286978",
                "#ff9f43",
                "#b982ff",
                "#6ee7f9",
                "#f2cc60",
                "#35c46b",
                "#ff5f56",
            ),
        )
        second_y = top_y + chart_h + gap
        self.draw_chart(
            x0=x0,
            y0=second_y,
            x1=x1,
            y1=second_y + chart_h,
            title="PID output",
            values=[sample.pid for sample in self.control_samples],
            labels=("R", "P", "Y"),
            colors=("#ff9f43", "#b982ff", "#6ee7f9"),
        )
        third_y = second_y + chart_h + gap
        self.draw_chart(
            x0=x0,
            y0=third_y,
            x1=x1,
            y1=min(height - 54, third_y + chart_h),
            title="Throttle and motor mix",
            values=[
                (
                    sample.throttle,
                    sample.motors[0],
                    sample.motors[1],
                    sample.motors[2],
                    sample.motors[3],
                )
                for sample in self.control_samples
            ],
            labels=("T", "M1", "M2", "M3", "M4"),
            colors=("#d7dee5", "#ff5f56", "#35c46b", "#58a6ff", "#f2cc60"),
        )

        sample = self.control_samples[-1]
        self.status.set(
            f"BB{sample.version} seq={sample.seq} imu={sample.imu_seq} armed={sample.armed} fresh={sample.imu_fresh}  "
            f"gyro=({sample.gyro_dps[0]:.1f}, {sample.gyro_dps[1]:.1f}, {sample.gyro_dps[2]:.1f}) dps  "
            f"cmd=({sample.command_dps[0]:.1f}, {sample.command_dps[1]:.1f}, {sample.command_dps[2]:.1f}) dps  "
            f"pid={sample.pid}  thr={sample.throttle}  motors={sample.motors}"
            f"{'  dropped=' + str(self.dropped_samples) if self.dropped_samples else ''}"
        )

    def draw_chart(
        self,
        *,
        x0: int,
        y0: int,
        x1: int,
        y1: int,
        title: str,
        values: Sequence[Sequence[float]],
        labels: Sequence[str],
        colors: Sequence[str],
    ) -> None:
        self.canvas.create_rectangle(x0, y0, x1, y1, outline="#33414a")
        self.canvas.create_text(x0, y0 - 18, anchor="w", fill="#d7dee5", text=title)

        if not values:
            self.canvas.create_text(
                (x0 + x1) / 2,
                (y0 + y1) / 2,
                fill="#d7dee5",
                text="Waiting for samples...",
            )
            return

        flat = [value for row in values for value in row]
        low = min(flat)
        high = max(flat)
        if high - low < 0.05:
            center = (high + low) / 2.0
            low = center - 0.025
            high = center + 0.025

        zero_y = scale_y(0.0, low, high, y0, y1)
        if y0 <= zero_y <= y1:
            self.canvas.create_line(x0, zero_y, x1, zero_y, fill="#2c363d")

        axis_count = min(len(labels), len(values[-1]), len(colors))
        for axis in range(axis_count):
            points: list[float] = []
            for idx, row in enumerate(values):
                x_pos = x0 + (idx / max(1, len(values) - 1)) * (x1 - x0)
                y_pos = scale_y(float(row[axis]), low, high, y0, y1)
                points.extend([x_pos, y_pos])
            if len(points) >= 4:
                self.canvas.create_line(*points, fill=colors[axis], width=2)
            self.canvas.create_text(
                x1 - 28 * axis_count + axis * 28,
                y0 - 18,
                fill=colors[axis],
                text=labels[axis],
            )

        self.canvas.create_text(x0, y1 + 14, anchor="w", fill="#8fa3ad", text=f"{low:.2f}")
        self.canvas.create_text(x1, y1 + 14, anchor="e", fill="#8fa3ad", text=f"{high:.2f}")

    def draw_body_3d(self, x0: int, y0: int, x1: int, y1: int) -> None:
        self.canvas.create_rectangle(x0, y0, x1, y1, outline="#33414a")
        self.canvas.create_text(
            x0,
            y0 - 18,
            anchor="w",
            fill="#d7dee5",
            text=f"Attitude ({self.attitude_mode_label()})",
        )

        cx = (x0 + x1) / 2
        cy = (y0 + y1) / 2
        scale = min(x1 - x0, y1 - y0) * 0.32

        body_points = {
            "front": (1.0, 0.0, 0.0),
            "rear": (-1.0, 0.0, 0.0),
            "right": (0.0, 1.0, 0.0),
            "left": (0.0, -1.0, 0.0),
            "top": (0.0, 0.0, -0.35),
        }
        projected = {
            name: project_point(rotate_point(point, self.roll_rad, self.pitch_rad, self.yaw_rad), cx, cy, scale)
            for name, point in body_points.items()
        }

        self.canvas.create_line(*projected["front"], *projected["rear"], fill="#d7dee5", width=4)
        self.canvas.create_line(*projected["left"], *projected["right"], fill="#d7dee5", width=4)
        self.canvas.create_line(*projected["front"], *projected["top"], fill="#ff5f56", width=2)
        self.canvas.create_oval_at(projected["front"], 7, "#ff5f56")
        self.canvas.create_oval_at(projected["rear"], 6, "#8fa3ad")
        self.canvas.create_oval_at(projected["left"], 6, "#35c46b")
        self.canvas.create_oval_at(projected["right"], 6, "#58a6ff")
        self.canvas.create_oval_at(projected["top"], 5, "#f2cc60")

        roll_deg = math.degrees(self.roll_rad)
        pitch_deg = math.degrees(self.pitch_rad)
        yaw_deg = math.degrees(self.yaw_rad)
        self.canvas.create_text(
            x0 + 18,
            y1 - 42,
            anchor="w",
            fill="#8fa3ad",
            text=f"roll {roll_deg:.1f} deg   pitch {pitch_deg:.1f} deg   yaw~ {yaw_deg:.1f} deg",
        )
        self.canvas.create_text(
            x0 + 18,
            y1 - 20,
            anchor="w",
            fill="#8fa3ad",
            text=self.attitude_hint(),
        )

    def close(self) -> None:
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
        self.root.destroy()


def accel_roll_pitch(acc_g: tuple[float, float, float]) -> tuple[float, float]:
    ax, ay, az = acc_g
    pitch_rad = PITCH_ACCEL_SIGN * math.atan2(ay, az)
    roll_rad = ROLL_ACCEL_SIGN * math.atan2(
        -ax,
        math.sqrt(ay * ay + az * az),
    )
    return roll_rad, pitch_rad


def blend_angle(current: float, target: float, alpha: float) -> float:
    delta = math.atan2(math.sin(target - current), math.cos(target - current))
    return current + alpha * delta


def parse_imu_sample(text: str, timestamp_s: float) -> ImuSample | None:
    match = IMU_RE.search(text)
    if match is None:
        return None
    return ImuSample(
        seq=int(match.group("seq")),
        timestamp_s=timestamp_s,
        acc_g=(
            int(match.group("acc_x")) / 1000.0,
            int(match.group("acc_y")) / 1000.0,
            int(match.group("acc_z")) / 1000.0,
        ),
        gyro_dps=(
            float(match.group("gyro_x")),
            float(match.group("gyro_y")),
            float(match.group("gyro_z")),
        ),
        raw_gyro=(
            int(match.group("raw_x")),
            int(match.group("raw_y")),
            int(match.group("raw_z")),
        ),
    )


def parse_control_sample(text: str, timestamp_s: float) -> ControlSample | None:
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
    return ControlSample(
        version=int(match.group("version")),
        seq=int(match.group("seq")),
        imu_seq=int(match.group("imu_seq")),
        timestamp_s=timestamp_s,
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


def scale_y(value: float, low: float, high: float, y0: int, y1: int) -> float:
    return y0 + (high - value) / (high - low) * (y1 - y0)


def rotate_point(
    point: tuple[float, float, float],
    roll: float,
    pitch: float,
    yaw: float,
) -> tuple[float, float, float]:
    x, y, z = point
    cr, sr = math.cos(roll), math.sin(roll)
    cp, sp = math.cos(pitch), math.sin(pitch)
    cy, sy = math.cos(yaw), math.sin(yaw)

    y, z = y * cr - z * sr, y * sr + z * cr
    x, z = x * cp + z * sp, -x * sp + z * cp
    x, y = x * cy - y * sy, x * sy + y * cy
    return x, y, z


def project_point(
    point: tuple[float, float, float],
    cx: float,
    cy: float,
    scale: float,
) -> tuple[float, float]:
    x, y, z = point
    return (cx + (x - y) * scale * 0.72, cy + (x + y) * scale * 0.24 - z * scale)


def create_oval_at(self: tk.Canvas, point: tuple[float, float], radius: int, fill: str) -> None:
    x, y = point
    self.create_oval(x - radius, y - radius, x + radius, y + radius, fill=fill, outline="")


setattr(tk.Canvas, "create_oval_at", create_oval_at)


def main() -> int:
    args = parse_args()
    if args.remote:
        host_uri = probe_host_from_args(args)
        error = check_remote_probe_reachable(host_uri)
        if error is not None:
            print(error, file=sys.stderr)
            return 2
    command = command_from_args(args)
    root = tk.Tk()
    view = ImuLiveView(
        root,
        command,
        max(8, args.samples),
        args.attitude,
        args.ui_hz,
        args.queue_samples,
        args.echo_rtt,
    )
    view.start()
    root.mainloop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
