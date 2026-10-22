"""Record a bench gate's result with reqtool (design §2).

The gate tools judge a run and describe how it was done; `reqtool record`
fingerprints it, checks the hardware rules, hashes the artifacts, and writes
the schema 3 record under `project_meta/testing/evidence/runs/`. The record is
left for review; nothing is committed.
"""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
REQTOOL = REPO_ROOT / "tools/reqtool/target/release/reqtool"
DETAILS_FILE = "reqtool-details.json"

# What reqtool accepts as an artifact ID: lowercase words joined by hyphens,
# led by a letter.
ARTIFACT_ID = re.compile(r"^[a-z][a-z0-9-]{0,63}$")


class RecordError(Exception):
    """reqtool refused the record, or could not be run."""


def artifact_id(path: Path) -> str:
    words = re.sub(r"[^a-z0-9]+", "-", path.stem.lower()).strip("-")
    if not words[:1].isalpha():
        words = f"capture-{words}"
    return words[:64].rstrip("-")


def git(*args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(REPO_ROOT), *args], capture_output=True, text=True, check=True
    ).stdout.strip()


def source_state() -> tuple[str, str]:
    """The checked-out revision and any uncommitted change to tracked files,
    to show the source did not change during a run."""

    return git("rev-parse", "HEAD"), git("status", "--porcelain", "--untracked-files=no")


def details(
    *,
    firmware_sha256: str,
    features: list[str],
    target_triple: str,
    propellers: str,
    actuator_power: str,
    equipment: list[str],
    environment: list[str],
    commands: list[dict[str, Any]],
    artifacts: list[tuple[Path, str]],
    measurements: dict[str, Any],
    observations: list[str],
    limitations: list[str],
    configuration: dict[str, Any] | None = None,
    root: Path = REPO_ROOT,
) -> dict[str, Any]:
    """A run's details as `reqtool record --details` reads them. Artifacts are
    named by path only; reqtool hashes them."""

    return {
        "execution": {"performed_by": "user", "user_confirmed": True},
        "candidate": {
            "firmware_sha256": firmware_sha256,
            "features": features,
            "target_triple": target_triple,
            "profile": "release",
            "configuration": configuration or {},
        },
        "conditions": {
            "propellers": propellers,
            "actuator_power": actuator_power,
            "equipment": equipment,
            "environment": environment,
        },
        "commands": commands,
        "artifacts": [
            {"id": artifact_id(path), "kind": kind, "path": path.relative_to(root).as_posix()}
            for path, kind in artifacts
        ],
        "measurements": measurements,
        "observations": observations,
        "limitations": limitations,
        "stop_conditions_triggered": [],
    }


def command(
    procedure: str, board: str, result: str, details_path: Path, root: Path = REPO_ROOT
) -> list[str]:
    return [
        str(REQTOOL),
        "--repository",
        str(root),
        "record",
        "--procedure",
        procedure,
        "--board",
        board,
        "--result",
        result,
        "--details",
        str(details_path),
    ]


def record(procedure: str, board: str, result: str, run_details: dict[str, Any], capture: Path) -> str:
    """Writes the details beside the captures and has reqtool record the run.
    Returns reqtool's report of what it wrote."""

    details_path = capture / DETAILS_FILE
    details_path.write_text(json.dumps(run_details, indent=2) + "\n")
    if not REQTOOL.is_file():
        raise RecordError(
            "build reqtool first (cargo build --release in tools/reqtool); the details are kept in "
            f"{details_path.relative_to(REPO_ROOT)}"
        )
    done = subprocess.run(
        command(procedure, board, result, details_path), capture_output=True, text=True
    )
    if done.returncode != 0:
        raise RecordError((done.stderr or done.stdout).strip())
    return done.stdout.strip()
