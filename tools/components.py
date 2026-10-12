#!/usr/bin/env python3
"""Content IDs for FerroWasp's testable components, which ones a passing run
record still proves, and which catalog tests are due on which board.

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

    for field in ("source_roots", "hardware_targets"):
        values = data.get(field)
        if not isinstance(values, list) or not values or not all(
            isinstance(item, str) and item for item in values
        ):
            errors.append(f"{label}.{field} must be a non-empty string list")

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


def _records(records_root: Path, *, with_components: bool) -> list[dict[str, Any]]:
    """Passing records, oldest first; only schema 2 ones if `with_components`."""

    records = []
    for path in sorted(records_root.rglob("*.json")):
        try:
            record = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if not isinstance(record, dict) or record.get("result") != "pass":
            continue
        if with_components and not isinstance(record.get("components"), list):
            continue
        records.append(record)
    return sorted(records, key=lambda record: str(record.get("completed_at_utc")))


def _passing_records(records_root: Path) -> list[dict[str, Any]]:
    return _records(records_root, with_components=True)


TIER_ORDER = [
    "software",
    "embedded-build",
    "bench-unpowered",
    "bench-powered-props-off",
    "preflight",
    "flight",
]
HARDWARE_TIERS = set(TIER_ORDER[2:])


def due_tests(
    catalog: dict[str, Any],
    hardware_targets: list[str],
    current: dict[str, str],
    records: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """One row per active catalog test and target it runs on.

    A common hardware test runs on every hardware target; a common software
    or build test runs once, as `common`. A row is `current` when the last
    pass with component evidence still matches every component it named,
    `due` when one of them changed, and `never` when no pass names components.
    """

    rows: list[dict[str, Any]] = []
    for entry in catalog.get("tests", []):
        if not isinstance(entry, dict) or entry.get("status") != "active":
            continue
        test_id, tier, target = entry.get("id"), entry.get("tier"), entry.get("target")
        if target == "common" and tier in HARDWARE_TIERS:
            targets = hardware_targets
        else:
            targets = [target]
        for run_target in targets:
            passes = [
                record
                for record in records
                if record.get("test_id") == test_id
                and record.get("target") == run_target
                and isinstance(record.get("components"), list)
            ]
            row = {"test_id": test_id, "tier": tier, "target": run_target}
            if not passes:
                row.update(state="never", run_id=None, changed=[])
            else:
                last = passes[-1]
                changed = [
                    entry["id"]
                    for entry in last["components"]
                    if isinstance(entry, dict)
                    and current.get(entry.get("id")) != entry.get("content_sha256")
                ]
                row.update(
                    state="due" if changed else "current",
                    run_id=last.get("run_id"),
                    changed=changed,
                )
            rows.append(row)
    rank = {tier: index for index, tier in enumerate(TIER_ORDER)}
    return sorted(rows, key=lambda row: (row["target"], rank.get(row["tier"], len(rank))))


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


def _load(root: Path) -> tuple[dict[str, Any], Path, dict[str, Any]]:
    registry = json.loads(root.joinpath(*REGISTRY_PATH.parts).read_text(encoding="utf-8"))
    policy = registry["test_evidence"]
    data = json.loads((root / policy["components"]).read_text(encoding="utf-8"))
    errors = validate_component_registry(data, policy["components"])
    if errors:
        raise ValueError("; ".join(errors))
    catalog = json.loads((root / registry["test_catalog"]).read_text(encoding="utf-8"))
    return data, root / policy["records_root"], catalog


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
    due = commands.add_parser(
        "due",
        help="list catalog tests by board: current, due, or never passed with components",
    )
    due.add_argument(
        "--fail-if-due",
        action="store_true",
        help="exit 2 when any test is due or has never passed with components",
    )
    args = parser.parse_args(argv)

    try:
        data, records_root, catalog = _load(args.root)
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

    if args.command == "due":
        rows = due_tests(catalog, data["hardware_targets"], current, _passing_records(records_root))
        legacy = {
            (record.get("test_id"), record.get("target"))
            for record in _records(records_root, with_components=False)
            if not isinstance(record.get("components"), list)
        }
        target = None
        for row in rows:
            if row["target"] != target:
                target = row["target"]
                print(target)
            if row["state"] == "current":
                detail = row["run_id"]
            elif row["state"] == "due":
                detail = f"changed {', '.join(row['changed'])} since {row['run_id']}"
            elif (row["test_id"], row["target"]) in legacy:
                detail = "only schema 1 passes, which name no components"
            else:
                detail = "no pass yet"
            print(f"  {row['state']:<8} {row['test_id']:<20} {row['tier']:<24} {detail}")
        outstanding = sum(row["state"] != "current" for row in rows)
        print(f"{outstanding} of {len(rows)} test runs are due or have no component evidence")
        return 2 if args.fail_if_due and outstanding else 0

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
