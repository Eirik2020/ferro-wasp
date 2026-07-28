#!/usr/bin/env python3
"""Reject unexpectedly large or prohibited files reachable from Git refs."""

from __future__ import annotations

import subprocess
import sys
import tomllib
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_CONFIG = REPO_ROOT / "tools/release/release.toml"
PROHIBITED_SUFFIXES = {
    ".bin",
    ".dmp",
    ".elf",
    ".hex",
    ".key",
    ".logicdata",
    ".map",
    ".p12",
    ".pem",
    ".pfx",
    ".sal",
    ".uf2",
}


def git(*args: str, input_text: str | None = None) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        input=input_text,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout


def main() -> int:
    with RELEASE_CONFIG.open("rb") as source:
        maximum_bytes = tomllib.load(source)["maximum_public_blob_bytes"]

    objects = git("rev-list", "--objects", "--all")
    details = git(
        "cat-file",
        "--batch-check=%(objecttype) %(objectsize) %(objectname) %(rest)",
        input_text=objects,
    )

    failures: list[str] = []
    checked_blobs = 0
    largest_blob = 0
    for line in details.splitlines():
        object_type, raw_size, _object_id, *path_parts = line.split(" ", 3)
        if object_type != "blob":
            continue
        checked_blobs += 1
        size = int(raw_size)
        largest_blob = max(largest_blob, size)
        path = path_parts[0] if path_parts else ""
        suffix = Path(path).suffix.lower()
        if size > maximum_bytes:
            failures.append(
                f"{path or '<unresolved path>'}: {size} bytes exceeds "
                f"{maximum_bytes}"
            )
        if suffix in PROHIBITED_SUFFIXES:
            failures.append(f"{path}: prohibited release-artifact suffix {suffix}")

    candidate_paths = git(
        "ls-files", "--cached", "--others", "--exclude-standard", "-z"
    ).split("\0")
    checked_candidate_files = 0
    for path in candidate_paths:
        if not path:
            continue
        candidate = REPO_ROOT / path
        if not candidate.is_file():
            continue
        checked_candidate_files += 1
        size = candidate.stat().st_size
        suffix = candidate.suffix.lower()
        if size > maximum_bytes:
            failures.append(
                f"candidate {path}: {size} bytes exceeds {maximum_bytes}"
            )
        if suffix in PROHIBITED_SUFFIXES:
            failures.append(f"candidate {path}: prohibited suffix {suffix}")

    if failures:
        for failure in failures:
            print(f"public-history check: {failure}", file=sys.stderr)
        return 1

    print(
        f"Public history ready: {checked_blobs} blobs checked; "
        f"largest is {largest_blob} bytes; "
        f"{checked_candidate_files} candidate files checked."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
