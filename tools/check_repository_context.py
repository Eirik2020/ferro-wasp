#!/usr/bin/env python3
"""Validate FerroWasp documentation routing and context-size budgets."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any

if __package__:
    from .app_context import validate_routes as validate_app_context_routes
    from .test_evidence import validate_evidence_records
else:
    from app_context import validate_routes as validate_app_context_routes
    from test_evidence import validate_evidence_records


REGISTRY_PATH = PurePosixPath("project_meta/DOCUMENT_REGISTRY.json")
ALLOWED_LIFECYCLES = {"live", "durable", "historical", "generated"}
ALLOWED_CONTEXTS = {"default", "targeted", "exclude"}
CURRENT_STATE_HEADING = re.compile(r"^## Current State(?:\s|$)", re.MULTILINE)
MARKDOWN_HEADING = re.compile(r"^#{1,6}\s+(.+?)\s*#*\s*$", re.MULTILINE)
TEST_ID = re.compile(r"^[A-Z][A-Z0-9]*(?:-[A-Z0-9]+){2,}$")
ALLOWED_TEST_TARGETS = {"common", "fcu3", "foxeer-f405-v2"}
TEST_TIER_ORDER = {
    "software": 0,
    "embedded-build": 1,
    "bench-unpowered": 2,
    "bench-powered-props-off": 3,
    "preflight": 4,
    "flight": 5,
}
ALLOWED_TEST_STATUSES = {"active", "needs-review", "retired"}
USER_EXECUTION_TIERS = {
    "bench-unpowered",
    "bench-powered-props-off",
    "preflight",
    "flight",
}
TARGET_TEST_PROCEDURE_PREFIX = "project_meta/testing/targets/"
ARCHIVE_DIRECTORY_NAME = "archive"
FORBIDDEN_TEST_RUN_FIELDS = {
    "artifact",
    "date",
    "log",
    "passed",
    "result",
    "sha256",
}
AGENT_INSTRUCTION_FILE = "AGENTS.md"
AGENT_DISCOVERY_IGNORED_DIRS = {
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    ".venv",
    "__pycache__",
    "book",
    "target",
    "venv",
}
SHA256_HEX = re.compile(r"^[0-9A-Fa-f]{64}$")
ADR_INDEX_PATH = PurePosixPath("project_meta/ARCHITECTURE_DECISIONS.md")
ADR_DIRECTORY = PurePosixPath("project_meta/decisions")
ADR_FILENAME = re.compile(r"^(ADR-\d{4})\.md$")
ADR_H1 = re.compile(r"^# (.+?)\s*$", re.MULTILINE)
ADR_STATUS_LINE = re.compile(r"^Status:[ \t]*(.*?)[ \t]*$", re.MULTILINE)
ADR_INDEX_ROW = re.compile(
    r"^\| \[(ADR-\d{4})\]\(decisions/(ADR-\d{4}\.md)\) "
    r"\| (.*?) \| (.*?) \|\s*$",
    re.MULTILINE,
)
ADR_REFERENCE = re.compile(r"\bADR-\d{4}\b")
ADR_SUPERSESSION = re.compile(r"\bsuperseded by (ADR-\d{4})\b", re.IGNORECASE)
ADR_INDEX_MAX_BYTES = 8192
ADR_RECORD_MAX_BYTES = 4096


def _repository_path(root: Path, raw_path: str) -> tuple[Path | None, str | None]:
    if not raw_path or "\\" in raw_path:
        return None, f"path must be a non-empty forward-slash path: {raw_path!r}"

    relative = PurePosixPath(raw_path)
    if relative.is_absolute() or ".." in relative.parts:
        return None, f"path must stay within the repository: {raw_path!r}"

    return root.joinpath(*relative.parts), None


def _positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def _discover_agent_instruction_paths(root: Path) -> set[str]:
    discovered: set[str] = set()

    for directory, child_dirs, filenames in os.walk(
        root,
        topdown=True,
        onerror=lambda _error: None,
    ):
        child_dirs[:] = [
            name for name in child_dirs if name not in AGENT_DISCOVERY_IGNORED_DIRS
        ]
        if AGENT_INSTRUCTION_FILE not in filenames:
            continue

        path = Path(directory) / AGENT_INSTRUCTION_FILE
        discovered.add(path.relative_to(root).as_posix())

    return discovered


def _string_list(
    value: Any,
    label: str,
    errors: list[str],
    *,
    allow_empty: bool,
) -> list[str]:
    if not isinstance(value, list):
        errors.append(f"{label} must be a list")
        return []
    if not value and not allow_empty:
        errors.append(f"{label} must not be empty")

    strings: list[str] = []
    for index, item in enumerate(value):
        if not isinstance(item, str) or not item.strip():
            errors.append(f"{label}[{index}] must be a non-empty string")
        else:
            strings.append(item)
    return strings


def _load_registry(root: Path, errors: list[str]) -> dict[str, Any]:
    registry_file = root.joinpath(*REGISTRY_PATH.parts)
    try:
        data = json.loads(registry_file.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"cannot read {REGISTRY_PATH.as_posix()}: {error}")
        return {}

    if not isinstance(data, dict):
        errors.append("document registry root must be a JSON object")
        return {}
    if data.get("schema_version") != 1:
        errors.append("document registry schema_version must be 1")
    return data


def _validate_immutable_archives(
    root: Path,
    value: Any,
    errors: list[str],
) -> None:
    if value is None:
        return
    if not isinstance(value, list):
        errors.append("document registry 'immutable_archives' must be a list")
        return

    seen: set[str] = set()
    for index, entry in enumerate(value):
        label = f"immutable_archives[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue

        raw_path = entry.get("path")
        expected_bytes = entry.get("bytes")
        expected_sha256 = entry.get("sha256")
        if not isinstance(raw_path, str):
            errors.append(f"{label}.path must be a string")
            continue
        if raw_path in seen:
            errors.append(f"duplicate immutable archive: {raw_path}")
        seen.add(raw_path)

        path, path_error = _repository_path(root, raw_path)
        if path_error:
            errors.append(f"{label}: {path_error}")
            continue
        assert path is not None

        if not _positive_int(expected_bytes):
            errors.append(f"{raw_path}: bytes must be a positive integer")
        if not isinstance(expected_sha256, str) or not SHA256_HEX.fullmatch(
            expected_sha256
        ):
            errors.append(f"{raw_path}: sha256 must be 64 hexadecimal characters")

        if not path.is_file():
            errors.append(f"immutable archive does not exist: {raw_path}")
            continue

        if _positive_int(expected_bytes) and path.stat().st_size != expected_bytes:
            errors.append(
                f"{raw_path}: immutable size {path.stat().st_size} does not match "
                f"{expected_bytes}"
            )

        if isinstance(expected_sha256, str) and SHA256_HEX.fullmatch(expected_sha256):
            digest = hashlib.sha256()
            try:
                with path.open("rb") as archive:
                    for chunk in iter(lambda: archive.read(1024 * 1024), b""):
                        digest.update(chunk)
            except OSError as error:
                errors.append(f"cannot hash immutable archive {raw_path}: {error}")
            else:
                actual_sha256 = digest.hexdigest().upper()
                if actual_sha256 != expected_sha256.upper():
                    errors.append(
                        f"{raw_path}: immutable SHA-256 {actual_sha256} does not "
                        f"match {expected_sha256.upper()}"
                    )


def validate_architecture_decisions(root: Path) -> list[str]:
    """Validate the bounded ADR index and its standalone decision records."""

    root = root.resolve()
    errors: list[str] = []
    index_path = root.joinpath(*ADR_INDEX_PATH.parts)
    decisions_path = root.joinpath(*ADR_DIRECTORY.parts)

    if not index_path.exists() and not decisions_path.exists():
        return errors
    if not index_path.is_file():
        errors.append(f"ADR index does not exist: {ADR_INDEX_PATH.as_posix()}")
        return errors
    if not decisions_path.is_dir():
        errors.append(f"ADR directory does not exist: {ADR_DIRECTORY.as_posix()}")
        return errors

    records: dict[str, tuple[str | None, str | None, str]] = {}
    heading_ids: set[str] = set()
    for path in sorted(decisions_path.glob("*.md")):
        filename_match = ADR_FILENAME.fullmatch(path.name)
        if filename_match is None:
            errors.append(
                f"unexpected ADR filename: {path.relative_to(root).as_posix()}"
            )
            continue

        file_id = filename_match.group(1)
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            errors.append(f"cannot read {path.relative_to(root).as_posix()}: {error}")
            continue

        headings = ADR_H1.findall(text)
        title: str | None = None
        if len(headings) != 1:
            errors.append(f"{file_id}: expected exactly one H1 heading")
        else:
            heading_match = re.fullmatch(r"(ADR-\d{4}):\s*(\S.*?)\s*", headings[0])
            if heading_match is None:
                errors.append(f"{file_id}: H1 must contain an ADR ID and title")
            else:
                heading_id, title = heading_match.groups()
                if heading_id != file_id:
                    errors.append(
                        f"{file_id}: H1 ID {heading_id} does not match filename"
                    )
                if heading_id in heading_ids:
                    errors.append(f"duplicate ADR H1 ID: {heading_id}")
                heading_ids.add(heading_id)

        status_lines = ADR_STATUS_LINE.findall(text)
        status: str | None = None
        if len(status_lines) != 1 or not status_lines[0].strip():
            errors.append(f"{file_id}: expected exactly one non-empty Status line")
        else:
            status = status_lines[0].strip()

        records[file_id] = (title, status, text)

    try:
        index_text = index_path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        errors.append(f"cannot read {ADR_INDEX_PATH.as_posix()}: {error}")
        return errors

    index_records: dict[str, tuple[str, str]] = {}
    for adr_id, filename, title, status in ADR_INDEX_ROW.findall(index_text):
        if adr_id in index_records:
            errors.append(f"duplicate ADR index entry: {adr_id}")
            continue
        expected_filename = f"{adr_id}.md"
        if filename != expected_filename:
            errors.append(
                f"{adr_id}: index link {filename} does not match {expected_filename}"
            )
        index_records[adr_id] = (title.strip(), status.strip())

    record_ids = set(records)
    index_ids = set(index_records)
    for adr_id in sorted(index_ids - record_ids):
        errors.append(f"ADR index entry does not resolve: {adr_id}")
    for adr_id in sorted(record_ids - index_ids):
        errors.append(f"orphan ADR file missing from index: {adr_id}")

    for adr_id in sorted(record_ids & index_ids):
        title, status, text = records[adr_id]
        index_title, index_status = index_records[adr_id]
        if title is not None and index_title != title:
            errors.append(
                f"{adr_id}: index title {index_title!r} does not match {title!r}"
            )
        if status is not None and index_status != status:
            errors.append(
                f"{adr_id}: index status {index_status!r} does not match {status!r}"
            )

        for reference in sorted(set(ADR_REFERENCE.findall(text))):
            if reference not in record_ids:
                errors.append(f"{adr_id}: unknown ADR reference {reference}")
        for successor in ADR_SUPERSESSION.findall(text):
            if successor == adr_id:
                errors.append(f"{adr_id}: supersession target must not be itself")
            elif successor not in record_ids:
                errors.append(f"{adr_id}: unknown supersession target {successor}")

    return errors


def validate_test_catalog(root: Path, raw_path: str) -> list[str]:
    """Validate test definitions without interpreting or executing procedures."""

    root = root.resolve()
    errors: list[str] = []
    catalog_path, path_error = _repository_path(root, raw_path)
    if path_error:
        return [f"test catalog: {path_error}"]
    assert catalog_path is not None

    try:
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return [f"cannot read test catalog {raw_path}: {error}"]

    if not isinstance(catalog, dict):
        return ["test catalog root must be a JSON object"]
    if catalog.get("schema_version") != 1:
        errors.append("test catalog schema_version must be 1")

    tests = catalog.get("tests")
    if not isinstance(tests, list):
        errors.append("test catalog 'tests' must be a list")
        return errors

    definitions: dict[str, dict[str, Any]] = {}
    heading_cache: dict[Path, set[str]] = {}

    for index, entry in enumerate(tests):
        label = f"tests[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue

        test_id = entry.get("id")
        if not isinstance(test_id, str) or not TEST_ID.fullmatch(test_id):
            errors.append(f"{label}.id must be a stable uppercase test ID")
            test_id = label
        elif test_id in definitions:
            errors.append(f"duplicate test ID: {test_id}")
        else:
            definitions[test_id] = entry

        summary = entry.get("summary")
        if not isinstance(summary, str) or not summary.strip():
            errors.append(f"{test_id}: summary must be a non-empty string")

        target = entry.get("target")
        if target not in ALLOWED_TEST_TARGETS:
            errors.append(f"{test_id}: invalid target {target!r}")

        tier = entry.get("tier")
        if tier not in TEST_TIER_ORDER:
            errors.append(f"{test_id}: invalid tier {tier!r}")

        status = entry.get("status")
        if status not in ALLOWED_TEST_STATUSES:
            errors.append(f"{test_id}: invalid status {status!r}")

        for field in sorted(FORBIDDEN_TEST_RUN_FIELDS & entry.keys()):
            errors.append(
                f"{test_id}: run field {field!r} belongs in evidence, not the catalog"
            )

        triggers = entry.get("triggers")
        trigger_count = 0
        if not isinstance(triggers, dict):
            errors.append(f"{test_id}: triggers must be an object")
        else:
            for field in ("paths", "features", "invocations"):
                values = _string_list(
                    triggers.get(field),
                    f"{test_id}.triggers.{field}",
                    errors,
                    allow_empty=True,
                )
                trigger_count += len(values)
                if field == "paths":
                    for trigger_path in values:
                        relative = PurePosixPath(trigger_path)
                        if (
                            "\\" in trigger_path
                            or relative.is_absolute()
                            or ".." in relative.parts
                        ):
                            errors.append(
                                f"{test_id}: invalid repository path trigger "
                                f"{trigger_path!r}"
                            )
            if trigger_count == 0:
                errors.append(f"{test_id}: at least one trigger is required")

        procedure = entry.get("procedure")
        if not isinstance(procedure, dict):
            errors.append(f"{test_id}: procedure must be an object")
        else:
            procedure_path = procedure.get("path")
            heading = procedure.get("heading")
            if not isinstance(procedure_path, str):
                errors.append(f"{test_id}.procedure.path must be a string")
            elif not isinstance(heading, str) or not heading.strip():
                errors.append(f"{test_id}.procedure.heading must be a non-empty string")
            else:
                if (
                    status == "active"
                    and ARCHIVE_DIRECTORY_NAME in PurePosixPath(procedure_path).parts
                ):
                    errors.append(
                        f"{test_id}: active procedure must not point into an archive"
                    )
                if (
                    status == "active"
                    and target != "common"
                    and tier in USER_EXECUTION_TIERS
                    and not procedure_path.startswith(TARGET_TEST_PROCEDURE_PREFIX)
                ):
                    errors.append(
                        f"{test_id}: active target hardware procedure must use "
                        f"{TARGET_TEST_PROCEDURE_PREFIX}"
                    )
                path, procedure_error = _repository_path(root, procedure_path)
                if procedure_error:
                    errors.append(f"{test_id}.procedure: {procedure_error}")
                elif path is None or not path.is_file():
                    errors.append(
                        f"{test_id}: procedure file does not exist: {procedure_path}"
                    )
                elif path.suffix.lower() != ".md":
                    errors.append(f"{test_id}: procedure must reference Markdown")
                else:
                    if path not in heading_cache:
                        try:
                            text = path.read_text(encoding="utf-8")
                        except (OSError, UnicodeError) as error:
                            errors.append(
                                f"{test_id}: cannot read procedure {procedure_path}: "
                                f"{error}"
                            )
                            heading_cache[path] = set()
                        else:
                            heading_cache[path] = set(MARKDOWN_HEADING.findall(text))
                    if heading not in heading_cache[path]:
                        errors.append(
                            f"{test_id}: heading {heading!r} not found in "
                            f"{procedure_path}"
                        )

        _string_list(
            entry.get("prerequisites"),
            f"{test_id}.prerequisites",
            errors,
            allow_empty=True,
        )

        requires_user = entry.get("requires_user_execution")
        if not isinstance(requires_user, bool):
            errors.append(f"{test_id}: requires_user_execution must be boolean")
        elif tier in USER_EXECUTION_TIERS and not requires_user:
            errors.append(f"{test_id}: tier {tier!r} requires user execution")

        stop_conditions = _string_list(
            entry.get("stop_conditions"),
            f"{test_id}.stop_conditions",
            errors,
            allow_empty=tier not in USER_EXECUTION_TIERS,
        )
        if tier in USER_EXECUTION_TIERS and not stop_conditions:
            errors.append(f"{test_id}: hardware tiers require stop conditions")

        _string_list(
            entry.get("evidence"),
            f"{test_id}.evidence",
            errors,
            allow_empty=False,
        )

    for test_id, entry in definitions.items():
        target = entry.get("target")
        tier = entry.get("tier")
        prerequisites = entry.get("prerequisites")
        if not isinstance(prerequisites, list):
            continue

        has_preflight = False
        seen_prerequisites: set[str] = set()
        for prerequisite_id in prerequisites:
            if not isinstance(prerequisite_id, str):
                continue
            if prerequisite_id in seen_prerequisites:
                errors.append(f"{test_id}: duplicate prerequisite {prerequisite_id}")
                continue
            seen_prerequisites.add(prerequisite_id)

            prerequisite = definitions.get(prerequisite_id)
            if prerequisite is None:
                errors.append(f"{test_id}: unknown prerequisite {prerequisite_id}")
                continue
            if prerequisite_id == test_id:
                errors.append(f"{test_id}: test cannot require itself")
                continue

            prerequisite_target = prerequisite.get("target")
            if prerequisite_target not in {"common", target}:
                errors.append(
                    f"{test_id}: prerequisite {prerequisite_id} targets "
                    f"{prerequisite_target!r}"
                )

            prerequisite_tier = prerequisite.get("tier")
            if tier in TEST_TIER_ORDER and prerequisite_tier in TEST_TIER_ORDER:
                if TEST_TIER_ORDER[prerequisite_tier] > TEST_TIER_ORDER[tier]:
                    errors.append(
                        f"{test_id}: prerequisite {prerequisite_id} has a later tier"
                    )
            if prerequisite_tier == "preflight":
                has_preflight = True

            if (
                entry.get("status") == "active"
                and prerequisite.get("status") != "active"
            ):
                errors.append(
                    f"{test_id}: active test requires non-active {prerequisite_id}"
                )

        if tier == "flight" and not has_preflight:
            errors.append(f"{test_id}: flight tests require a preflight prerequisite")

    visited: set[str] = set()
    visiting: set[str] = set()

    def visit(test_id: str, chain: list[str]) -> None:
        if test_id in visited:
            return
        if test_id in visiting:
            cycle_start = chain.index(test_id) if test_id in chain else 0
            cycle = chain[cycle_start:] + [test_id]
            errors.append(f"test prerequisite cycle: {' -> '.join(cycle)}")
            return

        visiting.add(test_id)
        entry = definitions[test_id]
        prerequisites = entry.get("prerequisites")
        if isinstance(prerequisites, list):
            for prerequisite_id in prerequisites:
                if isinstance(prerequisite_id, str) and prerequisite_id in definitions:
                    visit(prerequisite_id, chain + [test_id])
        visiting.remove(test_id)
        visited.add(test_id)

    for test_id in definitions:
        visit(test_id, [])

    return errors


def validate_repository(root: Path) -> list[str]:
    """Return concise validation failures; an empty list means success."""

    root = root.resolve()
    errors: list[str] = []
    registry = _load_registry(root, errors)
    _validate_immutable_archives(root, registry.get("immutable_archives"), errors)
    documents = registry.get("documents", [])
    budgets = registry.get("context_budgets", [])

    if not isinstance(documents, list):
        errors.append("document registry 'documents' must be a list")
        documents = []
    if not isinstance(budgets, list):
        errors.append("document registry 'context_budgets' must be a list")
        budgets = []

    registered: set[str] = set()
    for index, entry in enumerate(documents):
        label = f"documents[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue

        raw_path = entry.get("path")
        if not isinstance(raw_path, str):
            errors.append(f"{label}.path must be a string")
            continue

        path, path_error = _repository_path(root, raw_path)
        if path_error:
            errors.append(f"{label}: {path_error}")
            continue
        assert path is not None

        if raw_path in registered:
            errors.append(f"duplicate document registration: {raw_path}")
        registered.add(raw_path)

        if not raw_path.startswith("project_meta/") or not raw_path.endswith(".md"):
            errors.append(f"registered document must be project_meta/**/*.md: {raw_path}")

        role = entry.get("role")
        if not isinstance(role, str) or not role.strip():
            errors.append(f"{raw_path}: role must be a non-empty string")

        lifecycle = entry.get("lifecycle")
        if lifecycle not in ALLOWED_LIFECYCLES:
            errors.append(f"{raw_path}: invalid lifecycle {lifecycle!r}")

        context = entry.get("context")
        if context not in ALLOWED_CONTEXTS:
            errors.append(f"{raw_path}: invalid context {context!r}")
        if lifecycle == "historical" and context != "exclude":
            errors.append(f"{raw_path}: historical documents must use context 'exclude'")

        is_adr_index = raw_path == ADR_INDEX_PATH.as_posix()
        is_adr_record = (
            PurePosixPath(raw_path).parent == ADR_DIRECTORY
            and ADR_FILENAME.fullmatch(PurePosixPath(raw_path).name) is not None
        )
        if is_adr_index:
            if role != "architecture-decision-index":
                errors.append(f"{raw_path}: role must be 'architecture-decision-index'")
            if lifecycle != "durable" or context != "targeted":
                errors.append(f"{raw_path}: ADR index must be durable and targeted")
            if entry.get("max_bytes") != ADR_INDEX_MAX_BYTES:
                errors.append(
                    f"{raw_path}: max_bytes must be {ADR_INDEX_MAX_BYTES}"
                )
        elif is_adr_record:
            if role != "architecture-decision":
                errors.append(f"{raw_path}: role must be 'architecture-decision'")
            if lifecycle != "durable" or context != "targeted":
                errors.append(f"{raw_path}: ADR records must be durable and targeted")
            if entry.get("max_bytes") != ADR_RECORD_MAX_BYTES:
                errors.append(
                    f"{raw_path}: max_bytes must be {ADR_RECORD_MAX_BYTES}"
                )

        if not path.is_file():
            errors.append(f"registered document does not exist: {raw_path}")
            continue

        max_bytes = entry.get("max_bytes")
        if max_bytes is not None:
            if not _positive_int(max_bytes):
                errors.append(f"{raw_path}: max_bytes must be a positive integer")
            elif path.stat().st_size > max_bytes:
                errors.append(
                    f"{raw_path}: {path.stat().st_size} bytes exceeds budget {max_bytes}"
                )

        max_headings = entry.get("max_current_state_headings")
        if max_headings is not None:
            if not _positive_int(max_headings):
                errors.append(
                    f"{raw_path}: max_current_state_headings must be a positive integer"
                )
            else:
                heading_count = len(
                    CURRENT_STATE_HEADING.findall(path.read_text(encoding="utf-8"))
                )
                if heading_count > max_headings:
                    errors.append(
                        f"{raw_path}: {heading_count} Current State headings exceeds "
                        f"limit {max_headings}"
                    )

    docs_root = root / "project_meta"
    discovered = {
        path.relative_to(root).as_posix()
        for path in docs_root.rglob("*.md")
        if path.is_file()
    }
    for raw_path in sorted(discovered - registered):
        errors.append(f"unregistered project document: {raw_path}")
    for raw_path in sorted(registered - discovered):
        errors.append(f"registry entry is not a discovered project document: {raw_path}")

    errors.extend(validate_architecture_decisions(root))

    budget_paths: set[str] = set()
    for index, entry in enumerate(budgets):
        label = f"context_budgets[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue

        raw_path = entry.get("path")
        max_bytes = entry.get("max_bytes")
        if not isinstance(raw_path, str):
            errors.append(f"{label}.path must be a string")
            continue
        if raw_path in budget_paths:
            errors.append(f"duplicate context budget: {raw_path}")
        budget_paths.add(raw_path)

        path, path_error = _repository_path(root, raw_path)
        if path_error:
            errors.append(f"{label}: {path_error}")
            continue
        assert path is not None

        if not _positive_int(max_bytes):
            errors.append(f"{raw_path}: max_bytes must be a positive integer")
        elif not path.is_file():
            errors.append(f"context-budget file does not exist: {raw_path}")
        elif path.stat().st_size > max_bytes:
            errors.append(
                f"{raw_path}: {path.stat().st_size} bytes exceeds budget {max_bytes}"
            )

    discovered_agent_instructions = _discover_agent_instruction_paths(root)
    budgeted_agent_instructions = {
        raw_path
        for raw_path in budget_paths
        if PurePosixPath(raw_path).name == AGENT_INSTRUCTION_FILE
    }
    for raw_path in sorted(
        discovered_agent_instructions - budgeted_agent_instructions
    ):
        errors.append(f"unbudgeted agent instruction file: {raw_path}")

    test_catalog = registry.get("test_catalog")
    has_testing_docs = any(path.startswith("project_meta/testing/") for path in registered)
    if has_testing_docs and not isinstance(test_catalog, str):
        errors.append("document registry must identify test_catalog")
    elif isinstance(test_catalog, str):
        errors.extend(validate_test_catalog(root, test_catalog))

    test_evidence = registry.get("test_evidence")
    has_evidence_docs = any(
        path.startswith("project_meta/testing/evidence/") for path in registered
    )
    if has_evidence_docs and test_evidence is None:
        errors.append("document registry must identify test_evidence")
    elif test_evidence is not None and isinstance(test_catalog, str):
        errors.extend(validate_evidence_records(root, test_evidence, test_catalog))

    if (root / "tools" / "app_context.py").is_file():
        errors.extend(validate_app_context_routes(root))

    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (defaults to the parent of tools/)",
    )
    args = parser.parse_args(argv)

    errors = validate_repository(args.root)
    if errors:
        print("repository context check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("repository context check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
