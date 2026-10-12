#!/usr/bin/env python3
"""Content IDs for FerroWasp's testable components, and which ones a passing
run record still proves.

A component's content ID is the SHA-256 of its files' SHA-256s and paths, read
from the working tree, so an uncommitted change is a different ID. Markdown is
left out: documentation does not change what a component does.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath
from typing import Any


REGISTRY_PATH = PurePosixPath("project_meta/DOCUMENT_REGISTRY.json")
COMPONENT_ID = re.compile(r"^[a-z][a-z0-9-]{0,63}$")


def validate_component_registry(data: Any, label: str) -> list[str]:
    """Return structural errors in a component registry."""

    if not isinstance(data, dict):
        return [f"{label} must be an object"]
    errors: list[str] = []
    if data.get("schema_version") != 1:
        errors.append(f"{label} schema_version must be 1")

    roots = data.get("source_roots")
    if not isinstance(roots, list) or not roots or not all(
        isinstance(item, str) and item for item in roots
    ):
        errors.append(f"{label}.source_roots must be a non-empty string list")

    components = data.get("components")
    if not isinstance(components, list) or not components:
        return errors + [f"{label}.components must be a non-empty list"]

    seen_ids: set[str] = set()
    seen_paths: set[str] = set()
    for index, component in enumerate(components):
        item = f"{label}.components[{index}]"
        if not isinstance(component, dict) or set(component) != {"id", "paths"}:
            errors.append(f"{item} must have exactly 'id' and 'paths'")
            continue
        component_id = component["id"]
        if not isinstance(component_id, str) or not COMPONENT_ID.fullmatch(component_id):
            errors.append(f"{item}.id must be lowercase kebab-case")
        elif component_id in seen_ids:
            errors.append(f"duplicate component id: {component_id}")
        else:
            seen_ids.add(component_id)
        paths = component["paths"]
        if not isinstance(paths, list) or not paths:
            errors.append(f"{item}.paths must be a non-empty list")
            continue
        for path in paths:
            if (
                not isinstance(path, str)
                or not path
                or "\\" in path
                or PurePosixPath(path).is_absolute()
                or ".." in PurePosixPath(path).parts
            ):
                errors.append(f"{item}.paths entries must be repository paths")
            elif path in seen_paths:
                errors.append(f"path {path} belongs to more than one component")
            else:
                seen_paths.add(path)
    return errors


def component_ids(data: dict[str, Any]) -> set[str]:
    return {
        component["id"]
        for component in data.get("components", [])
        if isinstance(component, dict) and isinstance(component.get("id"), str)
    }


def _under(path: str, prefix: str) -> bool:
    return path == prefix or path.startswith(prefix.rstrip("/") + "/")


def _source_files(root: Path, roots: list[str]) -> list[str]:
    listed = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-co", "--exclude-standard", "-z"],
        check=True,
        capture_output=True,
    ).stdout.decode("utf-8")
    return sorted(
        path
        for path in set(listed.split("\0"))
        if path
        and not path.endswith(".md")
        and any(_under(path, source_root) for source_root in roots)
        and (root / path).is_file()
    )


def assign_files(
    data: dict[str, Any],
    files: list[str],
) -> tuple[dict[str, list[str]], list[str]]:
    """Give each file to the component with the longest matching path."""

    owned: dict[str, list[str]] = {component["id"]: [] for component in data["components"]}
    prefixes = [
        (path, component["id"])
        for component in data["components"]
        for path in component["paths"]
    ]
    uncovered: list[str] = []
    for file in files:
        matches = [(len(path), owner) for path, owner in prefixes if _under(file, path)]
        if matches:
            owned[max(matches)[1]].append(file)
        else:
            uncovered.append(file)
    return owned, uncovered


def content_id(root: Path, files: list[str]) -> str:
    digest = hashlib.sha256()
    for file in sorted(files):
        file_digest = hashlib.sha256((root / file).read_bytes()).hexdigest()
        digest.update(f"{file_digest}  {file}\n".encode("utf-8"))
    return digest.hexdigest()


def current_content_ids(root: Path, data: dict[str, Any]) -> tuple[dict[str, str], list[str]]:
    """Return each component's content ID, and the source files no component owns."""

    owned, uncovered = assign_files(data, _source_files(root, data["source_roots"]))
    return {component: content_id(root, files) for component, files in owned.items()}, uncovered


def coverage_errors(root: Path, data: dict[str, Any]) -> list[str]:
    owned, uncovered = assign_files(data, _source_files(root, data["source_roots"]))
    errors = [f"source file belongs to no component: {file}" for file in uncovered]
    errors.extend(
        f"component {component} owns no files" for component, files in owned.items() if not files
    )
    return errors


def _passing_records(records_root: Path) -> list[dict[str, Any]]:
    records = []
    for path in sorted(records_root.rglob("*.json")):
        try:
            record = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if (
            isinstance(record, dict)
            and record.get("result") == "pass"
            and isinstance(record.get("components"), list)
        ):
            records.append(record)
    return sorted(records, key=lambda record: str(record.get("completed_at_utc")))


def component_status(
    current: dict[str, str],
    records: list[dict[str, Any]],
) -> dict[str, dict[str, Any]]:
    """For each component: the passing runs that prove its current content,
    and, when none does, the tests that proved an earlier content."""

    status: dict[str, dict[str, Any]] = {
        component: {"proven_by": [], "earlier_tests": []} for component in current
    }
    for record in records:
        for entry in record["components"]:
            if not isinstance(entry, dict):
                continue
            component = entry.get("id")
            if component not in status:
                continue
            if entry.get("content_sha256") == current[component]:
                status[component]["proven_by"].append(record.get("run_id"))
            elif record.get("test_id") not in status[component]["earlier_tests"]:
                status[component]["earlier_tests"].append(record.get("test_id"))
    return status


def _load(root: Path) -> tuple[dict[str, Any], Path]:
    registry = json.loads(root.joinpath(*REGISTRY_PATH.parts).read_text(encoding="utf-8"))
    policy = registry["test_evidence"]
    data = json.loads((root / policy["components"]).read_text(encoding="utf-8"))
    errors = validate_component_registry(data, policy["components"])
    if errors:
        raise ValueError("; ".join(errors))
    return data, root / policy["records_root"]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (defaults to the parent of tools/)",
    )
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("ids", help="print every component's current content ID")
    commands.add_parser(
        "status",
        help="list components no passing run record proves at their current content",
    )
    record = commands.add_parser(
        "record",
        help="print the 'components' entry for a run record",
    )
    record.add_argument("component", nargs="+")
    args = parser.parse_args(argv)

    try:
        data, records_root = _load(args.root)
    except (OSError, KeyError, ValueError, json.JSONDecodeError) as error:
        print(f"cannot load the component registry: {error}", file=sys.stderr)
        return 1
    current, uncovered = current_content_ids(args.root, data)
    for file in uncovered:
        print(f"warning: source file belongs to no component: {file}", file=sys.stderr)

    if args.command == "ids":
        for component, digest in current.items():
            print(f"{digest[:12]}  {component}")
        return 0

    if args.command == "record":
        unknown = [component for component in args.component if component not in current]
        if unknown:
            print(f"unknown component: {', '.join(unknown)}", file=sys.stderr)
            return 1
        entries = [
            {"id": component, "content_sha256": current[component]}
            for component in args.component
        ]
        print(json.dumps(entries, indent=2))
        return 0

    status = component_status(current, _passing_records(records_root))
    unproven = 0
    for component, entry in status.items():
        if entry["proven_by"]:
            print(f"proven    {component}: {entry['proven_by'][-1]}")
        elif entry["earlier_tests"]:
            unproven += 1
            print(f"changed   {component}: rerun {', '.join(entry['earlier_tests'])}")
        else:
            unproven += 1
            print(f"untested  {component}")
    print(f"{unproven} of {len(status)} components lack a passing record at their current content")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
